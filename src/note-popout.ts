import type { Note } from "./api";
import { wordCount } from "./ledger-notes";
import { mountEditor, type Editor } from "./note-editor";
import { h } from "./ui";

/// A Ledger note popped out into a small window of its own, to keep in view beside other apps:
/// Ledger's editor, a slim bar, and a footer. Ledger may have the same note open, so a change
/// from elsewhere is taken only while nothing typed here is waiting to be kept.

/// What the pop-out asks of the page around it.
export type PopoutActions = {
  /** null when there's no such note (any more). */
  load(id: string): Promise<Note | null>;
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  setTitle(title: string): void;
  close(): void;
  onTop(): Promise<boolean>;
  setOnTop(on: boolean): Promise<void>;
  openInLedger(id: string): void;
  /** Typing stopped a while ago: time to let combined computers have it. */
  syncSoon(): void;
};

/// A note changed in some window: its id, the window's label, and whether it was deleted.
export type NoteChange = { id: string; from: string; gone: boolean };

export type Popout = {
  /** Shows this note, keeping the one shown before. */
  show(id: string): Promise<void>;
  /** A note changed somewhere. */
  changed(change: NoteChange): Promise<void>;
  /** Reads the note again, as after a sync. */
  refresh(): Promise<void>;
  /** Saves what's typed now. */
  flush(): Promise<void>;
};

/// This window's label, as changes made here are told back to it.
export const LABEL = "note";

const SAVE_AFTER_MS = 500;
const SYNC_AFTER_MS = 30_000;

export function mountPopout(root: HTMLElement, act: PopoutActions): Popout {
  let open: Note | null = null;
  let editor: Editor | null = null;
  // Typed since the last save: what's on screen is newer than what's kept.
  let dirty = false;
  let saveTimer = 0;
  let syncTimer = 0;

  const tool = (name: string, label: string, text: string, run: () => void) => {
    const b = h("button", { class: "note-tool", "data-tool": name, title: label, "aria-label": label }, text);
    // Pressed without taking the keyboard from the note, so the selection stays.
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", run);
    return b;
  };

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

  const ledger = tool("ledger", "Open in Ledger", "↗", async () => {
    if (!open) return;
    const id = open.id;
    await save();
    act.openInLedger(id);
  });

  const bar = h(
    "div",
    { class: "popout-bar" },
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
  const foot = h("div", { class: "note-foot", "aria-live": "polite" });
  root.replaceChildren(h("div", { class: "popout" }, bar, box, foot));

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
    if (!kept) return act.close();
    take(kept);
    drawFoot();
  };

  const view: Popout = {
    show: async (id) => {
      await save();
      const n = await act.load(id);
      if (!n) return act.close();
      take(n);
      dirty = false;
      editor = mountEditor(box, n.body, () => {
        dirty = true;
        drawFoot();
        window.clearTimeout(saveTimer);
        saveTimer = window.setTimeout(() => void save(), SAVE_AFTER_MS);
        window.clearTimeout(syncTimer);
        syncTimer = window.setTimeout(() => act.syncSoon(), SYNC_AFTER_MS);
      });
      drawFoot();
      editor.focus();
    },
    changed: async (c) => {
      if (!open || c.id !== open.id || c.from === LABEL) return;
      if (c.gone) return act.close();
      await view.refresh();
    },
    refresh: async () => {
      if (!open || dirty) return;
      const id = open.id;
      const fresh = await act.load(id);
      // Typed in, or switched to another note, while it was being read.
      if (open?.id !== id || dirty || !editor) return;
      if (!fresh) return act.close();
      if (fresh.body !== editor.value()) editor.set(fresh.body);
      take(fresh);
      drawFoot();
    },
    flush: () => save(),
  };
  return view;
}
