import type { Note } from "./api";
import { notePreview, wordCount } from "./ledger-notes";
import { mountEditor, type Editor } from "./note-editor";
import { h } from "./ui";

/// A Ledger note popped out into a small window of its own, to keep in view beside other apps,
/// or the list of notes to pick one from or start a new one. Ledger's editor, a slim bar, and a
/// footer. Ledger may have the same note open, so a change from elsewhere is taken only while
/// nothing typed here is waiting to be kept.

/// What the pop-out asks of the page around it.
export type PopoutActions = {
  /** null when there's no such note (any more). */
  load(id: string): Promise<Note | null>;
  /** Notes in order, those matching `query` when it isn't empty. */
  search(query: string): Promise<Note[]>;
  create(): Promise<Note>;
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  setTitle(title: string): void;
  onTop(): Promise<boolean>;
  setOnTop(on: boolean): Promise<void>;
  openInLedger(id: string): void;
  /** Typing stopped a while ago: time to let combined computers have it. */
  syncSoon(): void;
};

/// A note changed in some window: its id, the window's label, and whether it was deleted.
export type NoteChange = { id: string; from: string; gone: boolean };

export type Popout = {
  /** Shows this note, keeping the one shown before; the list if it's gone. */
  show(id: string): Promise<void>;
  /** Shows the list of notes, keeping the one shown before. */
  list(): Promise<void>;
  /** A note changed somewhere. */
  changed(change: NoteChange): Promise<void>;
  /** Reads the note or the list again, as after a sync. */
  refresh(): Promise<void>;
  /** Saves what's typed now. */
  flush(): Promise<void>;
};

/// This window's label, as changes made here are told back to it.
export const LABEL = "note";

const SAVE_AFTER_MS = 500;
const SYNC_AFTER_MS = 30_000;
/// Shown faintly in an empty note, as in Ledger.
const PROMPT = "# A title\nAnything below it. - [ ] makes a checkbox, **bold**, *italic*.";

export function mountPopout(root: HTMLElement, act: PopoutActions): Popout {
  let open: Note | null = null;
  let editor: Editor | null = null;
  let listing = false;
  // Typed since the last save: what's on screen is newer than what's kept.
  let dirty = false;
  let saveTimer = 0;
  let syncTimer = 0;
  let asked = 0;

  const tool = (name: string, label: string, text: string, run: () => void) => {
    const b = h("button", { class: "note-tool", "data-tool": name, title: label, "aria-label": label }, text);
    // Pressed without taking the keyboard from the note, so the selection stays.
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", run);
    return b;
  };

  // Floating is the window's, whatever it shows.
  let onTop = true;
  const pin = tool("on-top", "Keep above other windows", "📌", () => {
    onTop = !onTop;
    drawPin();
    void act.setOnTop(onTop);
  });
  const drawPin = () => pin.setAttribute("aria-pressed", String(onTop));
  drawPin();
  void act.onTop().then((on) => {
    onTop = on;
    drawPin();
  });

  const shell = h("div", { class: "popout" });
  root.replaceChildren(shell);

  // ---------- one note ----------

  const foot = h("div", { class: "note-foot", "aria-live": "polite" });
  const drawFoot = () => {
    if (!editor) return;
    const words = wordCount(editor.value());
    foot.replaceChildren(h("span", {}, dirty ? "Saving…" : "Saved"), h("span", {}, `${words} word${words === 1 ? "" : "s"}`));
  };

  const take = (n: Note) => {
    open = n;
    act.setTitle(n.title);
  };

  const save = async () => {
    window.clearTimeout(saveTimer);
    if (!open || !editor || !dirty) return;
    const id = open.id;
    const body = editor.value();
    dirty = false;
    const kept = await act.save(id, body);
    if (open?.id !== id) return;
    if (!kept) return view.list();
    take(kept);
    drawFoot();
  };

  const drawNote = (n: Note) => {
    listing = false;
    take(n);
    dirty = false;
    const back = tool("list", "All notes", "☰", () => void view.list());
    const ledger = tool("ledger", "Open in Ledger", "↗", async () => {
      if (!open) return;
      const id = open.id;
      await save();
      act.openInLedger(id);
    });
    const bar = h(
      "div",
      { class: "popout-bar" },
      back,
      h(
        "span",
        { class: "note-tools" },
        tool("bold", "Bold (Ctrl+B)", "B", () => editor?.wrap("**")),
        tool("italic", "Italic (Ctrl+I)", "I", () => editor?.wrap("*")),
        tool("box", "Checkbox", "☐", () => editor?.box()),
      ),
      h("span", { class: "popout-window-tools" }, pin, ledger),
    );
    const box = h("div", { class: "popout-body" });
    shell.replaceChildren(bar, box, foot);
    editor = mountEditor(
      box,
      n.body,
      () => {
        dirty = true;
        drawFoot();
        window.clearTimeout(saveTimer);
        saveTimer = window.setTimeout(() => void save(), SAVE_AFTER_MS);
        window.clearTimeout(syncTimer);
        syncTimer = window.setTimeout(() => act.syncSoon(), SYNC_AFTER_MS);
      },
      PROMPT,
    );
    drawFoot();
    editor.focus();
  };

  // ---------- the list ----------

  const search = h("input", { class: "filter popout-search", type: "search", placeholder: "Search notes", "aria-label": "Search notes", autocomplete: "off" }) as HTMLInputElement;
  const rows = h("div", { class: "popout-list", role: "list" });
  const newNote = h("button", { class: "btn popout-new" }, "+ New");
  newNote.addEventListener("click", async () => {
    const n = await act.create();
    search.value = "";
    drawNote(n);
  });
  search.addEventListener("input", () => void loadList());

  const loadList = async () => {
    const mine = ++asked;
    const found = await act.search(search.value.trim());
    // Only the latest answer, should an earlier search finish after it; and only while listing.
    if (mine !== asked || !listing) return;
    if (!found.length) {
      rows.replaceChildren(h("p", { class: "muted popout-empty" }, search.value.trim() ? "No notes match." : "No notes yet. + New starts one."));
      return;
    }
    rows.replaceChildren(
      ...found.map((n) => {
        const preview = notePreview(n.body);
        const row = h(
          "button",
          { class: `popout-row${n.pinned ? " pinned" : ""}`, "data-note": n.id, role: "listitem" },
          h("strong", {}, n.title),
          ...(preview ? [h("span", { class: "popout-row-preview muted" }, preview)] : []),
        );
        row.addEventListener("click", () => void view.show(n.id));
        return row;
      }),
    );
  };

  const view: Popout = {
    show: async (id) => {
      await save();
      const n = await act.load(id);
      if (!n) return view.list();
      drawNote(n);
    },
    list: async () => {
      await save();
      open = null;
      editor = null;
      dirty = false;
      listing = true;
      act.setTitle("Notes");
      shell.replaceChildren(h("div", { class: "popout-bar" }, search, newNote, h("span", { class: "popout-window-tools" }, pin)), rows);
      await loadList();
      search.focus();
    },
    changed: async (c) => {
      if (c.from === LABEL) return;
      if (listing) return loadList();
      if (!open || c.id !== open.id) return;
      if (c.gone) return view.list();
      await view.refresh();
    },
    refresh: async () => {
      if (listing) return loadList();
      if (!open || dirty) return;
      const id = open.id;
      const fresh = await act.load(id);
      // Typed in, or switched to another note, while it was being read.
      if (open?.id !== id || dirty || !editor) return;
      if (!fresh) return view.list();
      if (fresh.body !== editor.value()) editor.set(fresh.body);
      take(fresh);
      drawFoot();
    },
    flush: () => save(),
  };
  return view;
}
