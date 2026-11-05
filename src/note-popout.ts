import type { Note, Paper } from "./api";
import { notePreview } from "./ledger-notes";
import { noteTool, openNoteSession, type NoteSession } from "./note-session";
import { glyph } from "./glyphs";
import { IS_MAC, h } from "./ui";

/// A Ledger note popped out into a small window of its own, to keep in view beside other apps,
/// or the list of notes to pick one from or start a new one. Writing goes through the same
/// session as Ledger's (`note-session`); Ledger may have the same note open, so a change from
/// elsewhere is taken only while nothing typed here is waiting to be kept.

/// What the pop-out asks of the page around it.
export type PopoutActions = {
  /** null when there's no such note (any more). */
  load(id: string): Promise<Note | null>;
  /** Notes in order, those matching `query` when it isn't empty. */
  search(query: string): Promise<Note[]>;
  /** A new note, starting with `body` (empty, or a title to begin with). */
  create(body: string): Promise<Note>;
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  setPaper(id: string, paper: Paper): Promise<void>;
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

export function mountPopout(root: HTMLElement, act: PopoutActions): Popout {
  let session: NoteSession | null = null;
  let listing = false;
  let asked = 0;

  // Floating is the window's, whatever it shows.
  let onTop = true;
  const pin = noteTool("on-top", "Keep above other windows", glyph("pin"), () => {
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

  /// Puts the note shown away, keeping what's typed: a note whose typing isn't kept yet stays,
  /// its footer saying so. Whether it was put away.
  const putAway = async (): Promise<boolean> => {
    if (!session) return true;
    const s = session;
    if (!(await s.save())) {
      s.blocked();
      return false;
    }
    s.close();
    if (session === s) session = null;
    return true;
  };

  // ---------- one note ----------

  const drawNote = (n: Note) => {
    listing = false;
    act.setTitle(n.title);
    const box = h("div", { class: "popout-body" });
    const s = openNoteSession(box, n, act, {
      kept: (kept) => act.setTitle(kept.title),
      gone: () => void view.list(),
    });
    session = s;
    const back = noteTool("list", "All notes", glyph("list"), () => void view.list());
    const ledger = noteTool("ledger", "Open in Ledger", glyph("open"), async () => {
      if (await s.save()) act.openInLedger(s.note().id);
      else s.blocked();
    });
    shell.replaceChildren(h("div", { class: "popout-bar" }, back, s.tools(), h("span", { class: "popout-window-tools" }, s.paper, pin, ledger)), box, s.foot);
    s.editor.focus();
  };

  // ---------- the list ----------

  const search = h("input", { class: "filter popout-search", type: "search", placeholder: "Search notes", "aria-label": "Search notes", autocomplete: "off" }) as HTMLInputElement;
  const rows = h("div", { class: "popout-list", role: "list" });
  const newNote = h("button", { class: "btn primary popout-new", "aria-keyshortcuts": `${IS_MAC ? "Meta" : "Control"}+N` }, "+ New");
  // What went wrong last, said under the bar until the next try.
  const status = h("p", { class: "popout-status", role: "status" });
  const say = (text: string) => (status.textContent = text);

  /// A new note, opened ready to type: what's searched for becomes its title.
  const startNote = async () => {
    if (!(await putAway())) return;
    const title = listing ? search.value.trim() : "";
    try {
      const n = await act.create(title ? `${title}\n` : "");
      search.value = "";
      say("");
      drawNote(n);
    } catch {
      if (!listing) await view.list();
      say("Couldn't start a note. Try again.");
    }
  };
  newNote.addEventListener("click", () => void startNote());
  search.addEventListener("input", () => void loadList());

  // The keyboard way through the list: Enter opens the top match, the arrows move between notes.
  const rowButtons = () => [...rows.querySelectorAll<HTMLButtonElement>(".popout-row")];
  search.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      rowButtons()[0]?.click();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      rowButtons()[0]?.focus();
    }
  });
  rows.addEventListener("keydown", (e) => {
    const all = rowButtons();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    const to = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: all.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    if (to < 0) search.focus();
    else all[Math.min(to, all.length - 1)].focus();
  });
  // Esc in a note goes back to the list; Ctrl+N (Command+N) starts a note from either.
  root.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    if (e.key === "Escape" && !listing && session) {
      e.preventDefault();
      void view.list();
    } else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      void startNote();
    }
  });

  const loadList = async () => {
    const mine = ++asked;
    let found: Note[];
    try {
      found = await act.search(search.value.trim());
    } catch {
      if (mine !== asked || !listing) return;
      const again = h("button", { class: "link-btn" }, "Try again");
      again.addEventListener("click", () => void loadList());
      rows.replaceChildren(h("p", { class: "muted popout-empty" }, "Couldn't read your notes. ", again));
      return;
    }
    // Only the latest answer, should an earlier search finish after it; and only while listing.
    if (mine !== asked || !listing) return;
    if (!found.length) {
      const q = search.value.trim();
      rows.replaceChildren(h("p", { class: "muted popout-empty" }, q ? `No notes match. + New starts one called “${q}”.` : "No notes yet. + New starts one."));
      return;
    }
    rows.replaceChildren(
      ...found.map((n) => {
        const preview = notePreview(n.body);
        // A button inside each list item, so it's still said to be one that can be pressed.
        const row = h(
          "button",
          { class: `popout-row${n.pinned ? " pinned" : ""}`, "data-note": n.id },
          h("strong", {}, n.title),
          // The ribbon is drawn; screen readers are told.
          ...(n.pinned ? [h("span", { class: "sr-only" }, ", pinned")] : []),
          ...(preview ? [h("span", { class: "popout-row-preview muted" }, preview)] : []),
        );
        row.addEventListener("click", () => void view.show(n.id));
        return h("div", { role: "listitem" }, row);
      }),
    );
  };

  const view: Popout = {
    show: async (id) => {
      if (!(await putAway())) return;
      let n: Note | null;
      try {
        n = await act.load(id);
      } catch {
        await view.list();
        say("Couldn't open that note. Try again.");
        return;
      }
      if (!n) return view.list();
      say("");
      drawNote(n);
    },
    list: async () => {
      if (!(await putAway())) return;
      listing = true;
      act.setTitle("Notes");
      shell.replaceChildren(h("div", { class: "popout-bar" }, search, newNote, h("span", { class: "popout-window-tools" }, pin)), status, rows);
      await loadList();
      search.focus();
    },
    changed: async (c) => {
      if (c.from === LABEL) return;
      if (listing) return loadList();
      if (!session || c.id !== session.note().id) return;
      if (c.gone) {
        session.close();
        session = null;
        return view.list();
      }
      await view.refresh();
    },
    refresh: async () => {
      if (listing) return loadList();
      const s = session;
      if (!s || s.dirty()) return;
      const fresh = await act.load(s.note().id);
      // Switched to another note while it was being read.
      if (session !== s) return;
      if (!fresh) {
        s.close();
        session = null;
        return view.list();
      }
      if (s.take(fresh)) act.setTitle(fresh.title);
    },
    flush: async () => {
      await session?.save();
    },
  };
  return view;
}
