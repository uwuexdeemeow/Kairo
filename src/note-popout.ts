import type { Note } from "./api";
import { notePreview } from "./ledger-notes";
import { noteTool, openNoteSession, type NoteSession } from "./note-session";
import { h } from "./ui";

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

export function mountPopout(root: HTMLElement, act: PopoutActions): Popout {
  let session: NoteSession | null = null;
  let listing = false;
  let asked = 0;

  // Floating is the window's, whatever it shows.
  let onTop = true;
  const pin = noteTool("on-top", "Keep above other windows", "📌", () => {
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

  /// Puts the note shown away, keeping what's typed.
  const putAway = async () => {
    if (!session) return;
    const s = session;
    await s.save();
    s.close();
    if (session === s) session = null;
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
    const back = noteTool("list", "All notes", "☰", () => void view.list());
    const ledger = noteTool("ledger", "Open in Ledger", "↗", async () => {
      await s.save();
      act.openInLedger(s.note().id);
    });
    shell.replaceChildren(h("div", { class: "popout-bar" }, back, s.tools(), h("span", { class: "popout-window-tools" }, pin, ledger)), box, s.foot);
    s.editor.focus();
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
      await putAway();
      const n = await act.load(id);
      if (!n) return view.list();
      drawNote(n);
    },
    list: async () => {
      await putAway();
      listing = true;
      act.setTitle("Notes");
      shell.replaceChildren(h("div", { class: "popout-bar" }, search, newNote, h("span", { class: "popout-window-tools" }, pin)), rows);
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
