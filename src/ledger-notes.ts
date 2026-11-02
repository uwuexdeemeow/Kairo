import type { Note } from "./api";
import { mountEditor, type Editor } from "./note-editor";
import { h } from "./ui";

/// What the Notes view asks the page to do.
export type NotesActions = {
  /** Notes in order, those matching `query` when it isn't empty. */
  search(query: string): Promise<Note[]>;
  create(): Promise<Note>;
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  pin(id: string, pinned: boolean): Promise<void>;
  remove(id: string): Promise<void>;
  openTask(id: string): void;
  /** Typing stopped a while ago: time to let combined computers have it. */
  syncSoon(): void;
};

export type NotesView = {
  open(id: string): Promise<void>;
  /** Reads the list again, as after a sync; an open note takes a newer text unless it's being typed in. */
  refresh(): Promise<void>;
  /** Saves what's typed now, as before closing. */
  flush(): Promise<void>;
};

const SAVE_AFTER_MS = 500;
const SYNC_AFTER_MS = 30_000;
const MAX_CHARS = 100_000;

/// "9:14" today, "Yesterday", or "28 Oct".
export function editedText(ms: number, now: number): string {
  const d = new Date(ms);
  const day = (t: Date) => `${t.getFullYear()}-${t.getMonth()}-${t.getDate()}`;
  const today = new Date(now);
  if (day(d) === day(today)) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (day(d) === day(yesterday)) return "Yesterday";
  return `${d.getDate()} ${d.toLocaleDateString("en-GB", { month: "short" })}`;
}

/// Draws the Notes view in `root`: the list with its search on the left, the open note on the
/// right.
export function mountNotes(root: HTMLElement, act: NotesActions, now: () => number = Date.now): NotesView {
  const search = h("input", { class: "filter notes-search", type: "search", placeholder: "Search", "aria-label": "Search notes", autocomplete: "off" }) as HTMLInputElement;
  const create = h("button", { class: "btn notes-new" }, "+ New note");
  const list = h("div", { class: "notes-list", role: "list" });
  const pane = h("section", { class: "note-pane", "aria-label": "Note" });
  root.replaceChildren(h("div", { class: "notes" }, h("aside", { class: "notes-side" }, search, create, list), pane));

  let notes: Note[] = [];
  let open: Note | null = null;
  let editor: Editor | null = null;
  // Typed since the last save: what's on screen is newer than what's kept.
  let dirty = false;
  let saveTimer = 0;
  let syncTimer = 0;
  let asked = 0;

  const drawList = () => {
    if (!notes.length) {
      list.replaceChildren(h("p", { class: "muted notes-empty" }, search.value.trim() ? "No notes match." : "Write anything: plans, lists, things to remember."));
      return;
    }
    list.replaceChildren(
      ...notes.map((n) => {
        const meta = [n.tasks.length ? `${n.tasks.length} task${n.tasks.length === 1 ? "" : "s"}` : "", editedText(n.editedMs, now())].filter(Boolean).join(" · ");
        const row = h(
          "button",
          { class: `note-row${n.pinned ? " pinned" : ""}${open?.id === n.id ? " open" : ""}`, "data-note": n.id, role: "listitem" },
          h("span", { class: "note-row-title" }, n.title),
          h("span", { class: "note-row-meta muted" }, meta),
        );
        row.addEventListener("click", () => void view.open(n.id));
        return row;
      }),
    );
  };

  const load = async () => {
    const mine = ++asked;
    const found = await act.search(search.value.trim());
    // Only the latest answer, should an earlier search finish after it.
    if (mine === asked) notes = found;
  };

  const save = async () => {
    window.clearTimeout(saveTimer);
    if (!open || !editor || !dirty) return;
    const id = open.id;
    const body = editor.value();
    dirty = false;
    const kept = await act.save(id, body);
    if (open?.id !== id) return;
    if (kept) {
      open = kept;
      drawTop();
    } else {
      // Deleted on another computer meanwhile.
      closeNote();
    }
    notes = notes.map((n) => (n.id === id && kept ? kept : n));
    drawList();
  };

  const closeNote = () => {
    open = null;
    editor = null;
    dirty = false;
    pane.replaceChildren();
    drawList();
  };

  const top = h("div", { class: "note-top" });
  const limit = h("p", { class: "muted note-limit", "aria-live": "polite" });

  const drawTop = () => {
    if (!open) return;
    const n = open;
    const parts: Node[] = [];
    if (n.tasks.length) {
      parts.push(
        h(
          "span",
          { class: "note-tasks muted" },
          "Attached to: ",
          ...n.tasks.flatMap(([id, title], i) => {
            const link = h("button", { class: "link-btn" }, `${title} ↗`);
            link.addEventListener("click", () => act.openTask(id));
            return i ? [", ", link] : [link];
          }),
        ),
      );
    }
    const pin = h("button", { class: "link-btn note-pin" }, n.pinned ? "Unpin" : "Pin");
    pin.addEventListener("click", async () => {
      await act.pin(n.id, !n.pinned);
      if (open?.id === n.id) open = { ...n, pinned: !n.pinned };
      drawTop();
      await load();
      drawList();
    });
    const del = h("button", { class: "link-btn note-delete" }, "Delete");
    del.addEventListener("click", () => {
      const yes = h("button", { class: "btn" }, "Delete");
      const keep = h("button", { class: "link-btn note-keep" }, "Keep");
      const confirm = h("div", { class: "note-confirm", role: "alert" }, h("span", {}, `Delete “${n.title}”? Tasks attached to it lose the link.`), yes, keep);
      keep.addEventListener("click", () => confirm.remove());
      yes.addEventListener("click", async () => {
        dirty = false;
        window.clearTimeout(saveTimer);
        await act.remove(n.id);
        closeNote();
        await load();
        drawList();
      });
      top.after(confirm);
    });
    top.replaceChildren(...parts, h("span", { class: "note-top-buttons" }, pin, del));
  };

  const showLimit = () => (limit.textContent = editor && editor.value().length >= MAX_CHARS ? "This note is at its limit." : "");

  const view: NotesView = {
    open: async (id) => {
      await save();
      let n = notes.find((x) => x.id === id);
      if (!n) {
        search.value = "";
        await load();
        n = notes.find((x) => x.id === id);
      }
      if (!n) return;
      open = n;
      const box = h("div", { class: "note-body" });
      pane.replaceChildren(top, box, limit);
      drawTop();
      editor = mountEditor(box, n.body, () => {
        dirty = true;
        showLimit();
        window.clearTimeout(saveTimer);
        saveTimer = window.setTimeout(() => void save(), SAVE_AFTER_MS);
        window.clearTimeout(syncTimer);
        syncTimer = window.setTimeout(() => act.syncSoon(), SYNC_AFTER_MS);
      });
      showLimit();
      drawList();
    },
    refresh: async () => {
      await load();
      if (open) {
        const fresh = notes.find((x) => x.id === open!.id);
        if (!fresh && !search.value.trim()) closeNote();
        else if (fresh && !dirty && editor) {
          if (fresh.body !== editor.value()) editor.set(fresh.body);
          open = fresh;
          drawTop();
        }
      }
      drawList();
    },
    flush: () => save(),
  };

  search.addEventListener("input", async () => {
    await load();
    drawList();
  });
  create.addEventListener("click", async () => {
    const n = await act.create();
    search.value = "";
    await load();
    if (!notes.some((x) => x.id === n.id)) notes = [n, ...notes];
    await view.open(n.id);
    editor?.focus();
  });

  void load().then(drawList);
  return view;
}
