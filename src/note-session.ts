import type { Note, Paper } from "./api";
import { mountEditor, type Editor } from "./note-editor";
import { h } from "./ui";

/// One open note being written in: Ledger's editor, saved a moment after typing stops, the
/// footer saying whether it's kept, and B / I / ☐. Ledger's Notes and the pop-out window both
/// write through this, so they save and recover the same way.

/// Shown faintly in an empty note: what it can hold.
export const PROMPT = "# A title\nAnything below it. - [ ] makes a checkbox, **bold**, *italic*.";
export const SAVE_AFTER_MS = 500;
/// A save that failed (the database busy, say) is tried again after this.
export const RETRY_AFTER_MS = 3000;
export const SYNC_AFTER_MS = 30_000;

/// The papers a note can be written on, as the Paper dropdown lists them.
export const PAPERS: [Paper, string][] = [
  ["lined", "Lined"],
  ["blank", "Blank"],
  ["dotted", "Dotted"],
  ["grid", "Grid"],
];

export function wordCount(body: string): number {
  return body.replace(/[#*_`>\[\]()-]/g, " ").split(/\s+/).filter((w) => /\w/.test(w)).length;
}

/// A toolbar button that acts on the note without taking the keyboard from it, so the
/// selection stays.
export function noteTool(name: string, label: string, text: string, run: () => void): HTMLButtonElement {
  const b = h("button", { class: "note-tool", "data-tool": name, title: label, "aria-label": label }, text) as HTMLButtonElement;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", run);
  return b;
}

export type SessionActions = {
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  setPaper(id: string, paper: Paper): Promise<void>;
  /** Typing stopped a while ago: time to let combined computers have it. */
  syncSoon(): void;
};

export type SessionHooks = {
  /** The note as kept, after each save. */
  kept(note: Note): void;
  /** It was deleted elsewhere meanwhile. */
  gone(): void;
  /** Typed in (after the footer says so). */
  typed?(): void;
  /** More of the footer, after whether it's kept and its words. */
  footExtra?(note: Note): Node[];
};

export type NoteSession = {
  editor: Editor;
  note(): Note;
  /** Typed since the last save: what's on screen is newer than what's kept. */
  dirty(): boolean;
  /** Saves what's typed, if anything. A failure leaves it unsaved, to be tried again. */
  save(): Promise<void>;
  /** A newer copy from elsewhere: taken (the caret kept) unless typing here is waiting to be
   *  kept. Whether it was. */
  take(fresh: Note): boolean;
  /** Puts this text in place of what's there, as typed here, and saves it now. */
  replace(body: string): Promise<void>;
  /** The note is put away (another opened, or deleted): nothing more is saved. */
  close(): void;
  foot: HTMLElement;
  /** B, I and ☐, as a group. */
  tools(): HTMLElement;
  /** The Paper dropdown: the same one each time, so it can be put back in a redrawn bar. */
  paper: HTMLElement;
};

export function openNoteSession(box: HTMLElement, first: Note, act: SessionActions, hooks: SessionHooks): NoteSession {
  let note = first;
  let dirty = false;
  // The last save failed: the footer says it'll be tried again.
  let failed = false;
  let closed = false;
  let saveTimer = 0;
  let syncTimer = 0;
  const foot = h("div", { class: "note-foot", "aria-live": "polite" });

  const drawFoot = () => {
    const words = wordCount(editor.value());
    foot.replaceChildren(
      h("span", {}, failed ? "Not saved yet. Trying again…" : dirty ? "Saving…" : "Saved"),
      h("span", {}, `${words} word${words === 1 ? "" : "s"}`),
      ...(hooks.footExtra?.(note) ?? []),
    );
  };

  const save = async () => {
    window.clearTimeout(saveTimer);
    if (closed || !dirty) return;
    const body = editor.value();
    // Typing while it saves marks it dirty again, for the next save.
    dirty = false;
    let kept: Note | null;
    try {
      kept = await act.save(note.id, body);
    } catch {
      if (closed) return;
      // Still unsaved: kept on screen, not taken over by an older copy, and tried again.
      dirty = true;
      failed = true;
      drawFoot();
      saveTimer = window.setTimeout(() => void save(), RETRY_AFTER_MS);
      return;
    }
    if (closed) return;
    failed = false;
    if (!kept) return hooks.gone();
    // The paper is what's chosen here: a save that set out before a new choice brings the old one.
    note = { ...kept, paper: note.paper };
    hooks.kept(kept);
    drawFoot();
  };

  const editor = mountEditor(
    box,
    first.body,
    () => {
      dirty = true;
      drawFoot();
      hooks.typed?.();
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => void save(), SAVE_AFTER_MS);
      window.clearTimeout(syncTimer);
      syncTimer = window.setTimeout(() => act.syncSoon(), SYNC_AFTER_MS);
    },
    PROMPT,
  );
  drawFoot();

  // The paper is drawn by the stylesheet, on the box around the editor.
  const paperPick = h("select", { class: "note-paper-pick", "aria-label": "Paper", title: "Paper" }) as HTMLSelectElement;
  paperPick.replaceChildren(...PAPERS.map(([value, label]) => h("option", { value }, label)));
  const showPaper = () => {
    box.dataset.paper = note.paper;
    paperPick.value = note.paper;
  };
  showPaper();
  paperPick.addEventListener("change", async () => {
    const paper = paperPick.value as Paper;
    const before = note.paper;
    if (closed || paper === before) return;
    note = { ...note, paper };
    showPaper();
    hooks.kept(note);
    try {
      await act.setPaper(note.id, paper);
    } catch {
      // Not kept: back to the paper that is, rather than show one that will be lost.
      if (closed || note.paper !== paper) return;
      note = { ...note, paper: before };
      showPaper();
      hooks.kept(note);
    }
  });

  return {
    editor,
    note: () => note,
    dirty: () => dirty,
    save,
    take: (fresh) => {
      if (dirty || closed) return false;
      if (fresh.body !== editor.value()) editor.set(fresh.body);
      note = fresh;
      showPaper();
      drawFoot();
      return true;
    },
    replace: (body) => {
      editor.set(body);
      dirty = true;
      return save();
    },
    close: () => {
      closed = true;
      window.clearTimeout(saveTimer);
    },
    foot,
    tools: () =>
      h(
        "span",
        { class: "note-tools" },
        noteTool("bold", "Bold (Ctrl+B)", "B", () => editor.wrap("**")),
        noteTool("italic", "Italic (Ctrl+I)", "I", () => editor.wrap("*")),
        noteTool("box", "Checkbox", "☐", () => editor.box()),
      ),
    paper: h("span", { class: "note-paper" }, paperPick),
  };
}
