import type { Note } from "./api";
import { renameBubble, renameField } from "./ledger-tasks";
import { logo } from "./logos";
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
/// A note renamed: its first line with text says `title` instead, its leading #s and spaces kept.
/// An empty note becomes just the title.
export function retitle(body: string, title: string): string {
  const lines = body.split("\n");
  const at = lines.findIndex((l) => l.trim() !== "");
  if (at < 0) return title;
  const lead = /^[#\s]*/.exec(lines[at])![0];
  lines[at] = lead + title;
  return lines.join("\n");
}

/// What a note's card shows under its title: the text after the title line, its Markdown marks
/// taken out, on one line.
export function notePreview(body: string): string {
  const lines = body.split("\n");
  const title = lines.findIndex((l) => l.trim() !== "");
  return lines
    .slice(title + 1)
    .join(" ")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(^|\s)[-*] \[[ xX]\] /g, "$1")
    .replace(/(^|\s)(#{1,3}|[-*]|\d+\.) /g, "$1")
    .replace(/\*\*|[*_`]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function wordCount(body: string): number {
  return body.replace(/[#*_`>\[\]()-]/g, " ").split(/\s+/).filter((w) => /\w/.test(w)).length;
}

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
  const shell = h("div", { class: "notes" }, h("div", { class: "notes-side" }, search, create, list), pane);
  root.replaceChildren(shell);

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
      const searching = !!search.value.trim();
      const art = h("div", { class: "notes-empty-art", "aria-hidden": "true" });
      if (!searching) art.innerHTML = logo("ledger", 56);
      list.replaceChildren(art, h("p", { class: "muted notes-empty" }, searching ? "No notes match." : "Write anything: plans, lists, things to remember."));
      return;
    }
    list.replaceChildren(
      ...notes.map((n) => {
        const chips = [n.tasks.length ? `${n.tasks.length} task${n.tasks.length === 1 ? "" : "s"}` : "", editedText(n.editedMs, now())].filter(Boolean);
        const preview = notePreview(n.body);
        const title = h("span", { class: "note-row-title" }, n.title);
        // Hovering the row shows a bubble to rename the note where it is: its first line changes.
        const bubble = renameBubble(n.title, () => {
          const field = renameField(n.title, "note-rename", (text) => void rename(n, text), () => {
            field.replaceWith(title);
            bubble.hidden = false;
          });
          title.replaceWith(field);
          bubble.hidden = true;
          field.focus();
          field.select();
        });
        const row = h(
          "div",
          { class: `note-row${n.pinned ? " pinned" : ""}${open?.id === n.id ? " open" : ""}`, "data-note": n.id, role: "listitem", tabindex: "0" },
          title,
          ...(preview ? [h("span", { class: "note-row-preview muted" }, preview)] : []),
          h("span", { class: "note-row-meta" }, ...chips.map((c) => h("span", { class: "chip" }, c))),
          bubble,
        );
        row.addEventListener("click", () => void view.open(n.id));
        row.addEventListener("keydown", (e) => {
          if (e.key === "Enter" && e.target === row) void view.open(n.id);
        });
        return row;
      }),
    );
  };

  /// Renames a note from the list: its text with the new first line, kept; an open one's editor
  /// shows it too.
  const rename = async (n: Note, title: string) => {
    const isOpen = open?.id === n.id && editor;
    const body = retitle(isOpen ? editor!.value() : n.body, title);
    if (isOpen) {
      editor!.set(body);
      dirty = false;
      window.clearTimeout(saveTimer);
    }
    const kept = await act.save(n.id, body);
    if (kept) {
      notes = notes.map((x) => (x.id === n.id ? kept : x));
      if (open?.id === n.id) {
        open = kept;
        drawTop();
      }
    }
    drawList();
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
      drawFoot();
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
    shell.classList.remove("reading");
    drawList();
  };

  /// Under the sheet: whether what's typed is kept yet, its words, when it was edited.
  const foot = h("div", { class: "note-foot", "aria-live": "polite" });
  const drawFoot = () => {
    if (!open || !editor) return;
    const words = wordCount(editor.value());
    foot.replaceChildren(
      h("span", {}, dirty ? "Saving…" : "Saved"),
      h("span", {}, `${words} word${words === 1 ? "" : "s"}`),
      h("span", {}, `Edited ${editedText(open.editedMs, now())}`),
    );
  };

  const top = h("div", { class: "note-top" });
  const limit = h("p", { class: "muted note-limit", "aria-live": "polite" });

  const drawTop = () => {
    if (!open) return;
    const n = open;
    // On a narrow window the list and the note take turns; this goes back to the list.
    const back = h("button", { class: "link-btn note-back" }, "← Notes");
    back.addEventListener("click", async () => {
      await save();
      closeNote();
    });
    const parts: Node[] = [back];
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
      // Asked already: pressing again points at the question rather than asking twice.
      const asking = pane.querySelector<HTMLElement>(".note-confirm");
      if (asking) {
        asking.querySelector<HTMLButtonElement>(".btn")?.focus();
        return;
      }
      const yes = h("button", { class: "btn" }, "Delete");
      const keep = h("button", { class: "link-btn note-keep" }, "Keep");
      const confirm = h("div", { class: "note-confirm", role: "alert" }, h("span", {}, `Delete “${n.title}”? Tasks attached to it lose the link.`), yes, keep);
      keep.addEventListener("click", () => confirm.remove());
      yes.addEventListener("click", async () => {
        if (yes.hasAttribute("disabled")) return;
        yes.setAttribute("disabled", "");
        dirty = false;
        window.clearTimeout(saveTimer);
        await act.remove(n.id);
        closeNote();
        await load();
        drawList();
      });
      top.after(confirm);
    });
    const tool = (name: string, label: string, text: string, run: () => void) => {
      const b = h("button", { class: "note-tool", "data-tool": name, title: label, "aria-label": label }, text);
      // Pressed without taking the keyboard from the note, so the selection stays.
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", run);
      return b;
    };
    const tools = h(
      "span",
      { class: "note-tools" },
      tool("bold", "Bold (Ctrl+B)", "B", () => editor?.wrap("**")),
      tool("italic", "Italic (Ctrl+I)", "I", () => editor?.wrap("*")),
      tool("box", "Checkbox", "☐", () => editor?.box()),
    );
    top.replaceChildren(...parts, h("span", { class: "note-top-buttons" }, tools, pin, del));
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
      pane.replaceChildren(h("div", { class: "note-sheet" }, top, box, foot), limit);
      shell.classList.add("reading");
      drawTop();
      editor = mountEditor(box, n.body, () => {
        dirty = true;
        drawFoot();
        showLimit();
        window.clearTimeout(saveTimer);
        saveTimer = window.setTimeout(() => void save(), SAVE_AFTER_MS);
        window.clearTimeout(syncTimer);
        syncTimer = window.setTimeout(() => act.syncSoon(), SYNC_AFTER_MS);
      });
      showLimit();
      drawFoot();
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
