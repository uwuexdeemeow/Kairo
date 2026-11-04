import type { Note, Paper } from "./api";
import { glyph } from "./glyphs";
import { renameBubble, renameField } from "./ledger-tasks";
import { logo } from "./logos";
import { openNoteSession, type NoteSession } from "./note-session";
import { h } from "./ui";
import { mountUndo } from "./undo";

export { wordCount } from "./note-session";

/// What the Notes view asks the page to do.
export type NotesActions = {
  /** Notes in order, those matching `query` when it isn't empty. */
  search(query: string): Promise<Note[]>;
  create(): Promise<Note>;
  /** null when the note was deleted elsewhere. */
  save(id: string, body: string): Promise<Note | null>;
  pin(id: string, pinned: boolean): Promise<void>;
  setPaper(id: string, paper: Paper): Promise<void>;
  remove(id: string): Promise<void>;
  openTask(id: string): void;
  /** Shows the note in a small window of its own; no note, the list of notes. */
  popOut(id?: string): void;
  /** Typing stopped a while ago: time to let combined computers have it. */
  syncSoon(): void;
};

export type NotesView = {
  open(id: string): Promise<void>;
  /** Reads the list again, as after a sync; an open note takes a newer text unless it's being typed in. */
  refresh(): Promise<void>;
  /** Saves what's typed now, as before closing. */
  flush(): Promise<void>;
  /** Starts a new note and opens it, ready to type. */
  create(): Promise<void>;
  /** Makes a delete waiting on its Undo final, as when Notes is left. */
  finishDelete(): void;
};

const MAX_CHARS = 100_000;

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
  const popList = h("button", { class: "link-btn notes-pop-out", title: "All your notes in a small window, beside your work" }, "Pop out list");
  // "Deleted “Plan” · Undo", above the list while a delete can be taken back.
  const undoLine = h("p", { class: "task-undo notes-undo", role: "status", hidden: "" });
  // What didn't work, until the next thing does.
  const status = h("p", { class: "task-status notes-status", role: "status" });
  const say = (text: string) => (status.textContent = text);
  const noteUndo = mountUndo(undoLine);
  /// Notes deleted but still waiting on their Undo.
  const deletingNotes = new Set<string>();
  popList.addEventListener("click", () => act.popOut());
  const list = h("div", { class: "notes-list", role: "list" });
  const pane = h("section", { class: "note-pane", "aria-label": "Note" });
  const shell = h("div", { class: "notes" }, h("div", { class: "notes-side" }, search, h("div", { class: "notes-side-actions" }, create, popList), undoLine, status, list), pane);
  root.replaceChildren(shell);

  let notes: Note[] = [];
  let open: Note | null = null;
  // The open note being written in: its editor, saving and footer.
  let session: NoteSession | null = null;
  let asked = 0;

  /// Which note's row the keyboard was on, kept through redraws; and how each row renames, for F2.
  let focusedNote: string | null = null;
  let hadListFocus = false;
  const renamers = new WeakMap<HTMLElement, () => void>();

  const drawList = () => {
    hadListFocus = list.contains(document.activeElement);
    if (!open) drawWelcome();
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
        // The note itself is a button, named by its title; the rename bubble sits beside it, not
        // inside, so neither is a control within a control.
        const opener = h(
          "button",
          { class: "note-row-open", "aria-label": n.title },
          title,
          ...(preview ? [h("span", { class: "note-row-preview muted" }, preview)] : []),
          h("span", { class: "note-row-meta" }, ...chips.map((c) => h("span", { class: "chip" }, c))),
        );
        // Hovering the row shows a bubble to rename the note where it is: its first line changes.
        // The keyboard renames with F2 instead, so the bubble is no Tab stop of its own.
        opener.tabIndex = -1;
        opener.setAttribute("aria-describedby", "note-keys");
        const bubble = renameBubble(n.title, () => {
          const field = renameField(n.title, "note-rename", (text) => void rename(n, text), () => {
            field.replaceWith(opener);
            bubble.hidden = false;
            opener.focus();
          });
          opener.replaceWith(field);
          bubble.hidden = true;
          field.focus();
          field.select();
        });
        bubble.tabIndex = -1;
        renamers.set(opener, () => bubble.click());
        const row = h(
          "div",
          { class: `note-row${n.pinned ? " pinned" : ""}${open?.id === n.id ? " open" : ""}`, "data-note": n.id, role: "listitem" },
          opener,
          bubble,
        );
        // Anywhere on the card opens it; Enter or Space on the button comes here as a click.
        row.addEventListener("click", () => void view.open(n.id));
        return row;
      }),
      h("p", { id: "note-keys", class: "sr-only" }, "Up and down arrows move between notes. Enter opens, F2 renames."),
    );
    // The list is one Tab stop: the open note, or the one the keyboard was on, or the first.
    const openers = [...list.querySelectorAll<HTMLElement>(".note-row-open")];
    const current = openers.find((b) => b.closest<HTMLElement>("[data-note]")!.dataset.note === (focusedNote ?? open?.id)) ?? openers[0];
    if (current) current.tabIndex = 0;
    if (focusedNote && current && hadListFocus) current.focus();
  };

  list.addEventListener("focusin", (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>("[data-note]");
    if (row) focusedNote = row.dataset.note ?? null;
  });
  // The list's keys: up and down move, Home and End go to the ends, F2 renames.
  list.addEventListener("keydown", (e) => {
    const from = e.target as HTMLElement;
    if (!from.classList.contains("note-row-open")) return;
    if (e.key === "F2") {
      e.preventDefault();
      return renamers.get(from)?.();
    }
    const openers = [...list.querySelectorAll<HTMLElement>(".note-row-open")];
    const at = openers.indexOf(from);
    const to = ({ ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: openers.length - 1 } as Record<string, number>)[e.key];
    if (to === undefined || !openers[to]) return;
    e.preventDefault();
    from.tabIndex = -1;
    openers[to].tabIndex = 0;
    openers[to].focus();
  });

  /// Renames a note from the list: its text with the new first line, kept; an open one's editor
  /// shows it too.
  const rename = async (n: Note, title: string) => {
    // The open one is saved through its session, which shows the kept note when it's done.
    if (open?.id === n.id && session) return session.replace(retitle(session.editor.value(), title));
    const kept = await act.save(n.id, retitle(n.body, title));
    if (kept) notes = notes.map((x) => (x.id === n.id ? kept : x));
    drawList();
  };

  const load = async () => {
    const mine = ++asked;
    let found: Note[];
    try {
      found = await act.search(search.value.trim());
    } catch {
      // What's shown stays, rather than an empty list that looks like no notes.
      if (mine === asked) say("Couldn't read your notes. Try again in a moment.");
      return;
    }
    // Only the latest answer, should an earlier search finish after it.
    // A note waiting on its Undo stays out of sight.
    if (mine === asked) notes = found.filter((x) => !deletingNotes.has(x.id));
  };

  /// Saves what's typed: whether it's kept. A note that isn't stays open, its footer saying so,
  /// rather than being put away with the typing lost.
  const save = async (): Promise<boolean> => (session ? session.save() : true);
  /// Saves before the note is put away; one that can't be says why it's staying open.
  const keep = async (): Promise<boolean> => {
    const kept = await save();
    if (!kept) session?.blocked();
    return kept;
  };

  const closeNote = () => {
    session?.close();
    session = null;
    open = null;
    shell.classList.remove("reading");
    drawList();
  };

  /// With no note open, the pane offers the notes edited last, and a new one.
  const drawWelcome = () => {
    // "+ New note" is the list's, beside it: one, not two.
    // A few tips, one to a line, rather than one run-on line.
    const tip = h(
      "ul",
      { class: "muted notes-recent-tip" },
      h("li", {}, h("kbd", {}, "Ctrl+N"), " starts a note"),
      h("li", {}, h("kbd", {}, "Ctrl+B"), " bold, ", h("kbd", {}, "Ctrl+I"), " italic, “- [ ]” a checkbox"),
      h("li", {}, "Pop out list keeps your notes beside your work"),
    );
    const recent = [...notes].sort((a, b) => b.editedMs - a.editedMs).slice(0, 3);
    if (!recent.length) {
      const art = h("div", { class: "notes-empty-art", "aria-hidden": "true" });
      art.innerHTML = logo("ledger", 88);
      pane.replaceChildren(h("div", { class: "notes-recent notes-recent-none" }, art, tip));
      return;
    }
    const cards = recent.map((n) => {
      const preview = notePreview(n.body);
      const chips = [n.tasks.length ? `${n.tasks.length} task${n.tasks.length === 1 ? "" : "s"}` : "", `Edited ${editedText(n.editedMs, now())}`].filter(Boolean);
      const card = h(
        "button",
        { class: `notes-recent-card${n.pinned ? " pinned" : ""}`, "data-note": n.id },
        h("strong", {}, n.title),
        h("span", { class: "notes-recent-preview muted" }, preview || "Nothing under the title yet."),
        h("span", { class: "note-row-meta" }, ...chips.map((c) => h("span", { class: "chip" }, c))),
      );
      card.addEventListener("click", () => void view.open(n.id));
      return card;
    });
    pane.replaceChildren(
      h("div", { class: "notes-recent" }, h("h2", {}, "Pick up where you left off"), h("div", { class: "notes-recent-cards" }, ...cards), h("div", { class: "notes-recent-foot" }, tip)),
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
      if (await keep()) closeNote();
    });
    const parts: Node[] = [back];
    if (n.tasks.length) {
      parts.push(
        h(
          "span",
          { class: "note-tasks muted" },
          "Attached to: ",
          ...n.tasks.flatMap(([id, title], i) => {
            const link = h("button", { class: "link-btn link-out" }, title, glyph("out"));
            link.addEventListener("click", () => act.openTask(id));
            return i ? [", ", link] : [link];
          }),
        ),
      );
    }
    const pin = h("button", { class: "link-btn note-pin" }, n.pinned ? "Unpin" : "Pin");
    pin.addEventListener("click", async () => {
      try {
        await act.pin(n.id, !n.pinned);
        say("");
      } catch {
        return say(`Couldn't ${n.pinned ? "unpin" : "pin"} it. Try again in a moment.`);
      }
      if (open?.id === n.id) open = { ...n, pinned: !n.pinned };
      drawTop();
      await load();
      drawList();
    });
    const popOut = h("button", { class: "link-btn note-pop-out", title: "Keep it in view in a small window of its own" }, "Pop out");
    popOut.addEventListener("click", async () => {
      // Saved first, so the window opens on what's typed.
      if (await keep()) act.popOut(n.id);
    });
    // Deleted at once, as a task is: Undo (or Ctrl+Z) brings it back until Notes is left. Its tasks keep their link until then.
    const del = h("button", { class: "link-btn note-delete" }, "Delete");
    del.addEventListener("click", async () => {
      // What's typed is kept first, so Undo brings back all of it.
      await save();
      const at = notes.findIndex((x) => x.id === n.id);
      const neighbour = notes[at + 1] ?? notes[at - 1];
      const rowAfter = () => {
        const row = neighbour && [...list.querySelectorAll<HTMLElement>("[data-note]")].find((r) => r.dataset.note === neighbour.id);
        return row?.querySelector<HTMLElement>(".note-row-open") ?? row ?? search;
      };
      noteUndo.offer({
        what: n.title,
        finish: () => {
          deletingNotes.delete(n.id);
          void act.remove(n.id).then(
            () => load().then(drawList),
            () => {
              say("Couldn't delete it, so it's back in the list.");
              void load().then(drawList);
            },
          );
        },
        undo: () => {
          deletingNotes.delete(n.id);
          void load().then(() => view.open(n.id));
        },
        focusAfter: rowAfter,
      });
      deletingNotes.add(n.id);
      notes = notes.filter((x) => x.id !== n.id);
      closeNote();
      // The keyboard goes to the note beside it, ready for the next; the Undo line says what went.
      rowAfter()?.focus();
    });
    const tools = session ? [session.tools(), session.paper] : [];
    // Two groups: the writing tools, then the note's own, Pop out and Pin with Delete well apart
    // at the far end.
    top.replaceChildren(
      ...parts,
      h("span", { class: "note-top-write", role: "group", "aria-label": "Writing" }, ...tools),
      h("span", { class: "note-top-buttons", role: "group", "aria-label": "This note" }, popOut, pin, h("span", { class: "note-top-sep", "aria-hidden": "true" }), del),
    );
  };

  const showLimit = () => (limit.textContent = session && session.editor.value().length >= MAX_CHARS ? "This note is at its limit." : "");

  const view: NotesView = {
    open: async (id) => {
      if (!(await keep())) return;
      let n = notes.find((x) => x.id === id);
      if (!n) {
        search.value = "";
        await load();
        n = notes.find((x) => x.id === id);
      }
      if (!n) return;
      session?.close();
      open = n;
      const box = h("div", { class: "note-body" });
      // Under the sheet: whether what's typed is kept yet, its words, when it was edited.
      session = openNoteSession(box, n, act, {
        kept: (kept) => {
          open = kept;
          notes = notes.map((x) => (x.id === kept.id ? kept : x));
          drawTop();
          drawList();
        },
        // Deleted on another computer meanwhile.
        gone: () => closeNote(),
        typed: () => showLimit(),
        footExtra: (note) => [h("span", {}, `Edited ${editedText(note.editedMs, now())}`)],
      });
      pane.replaceChildren(h("div", { class: "note-sheet" }, top, box, session.foot), limit);
      shell.classList.add("reading");
      drawTop();
      showLimit();
      drawList();
    },
    refresh: async () => {
      await load();
      if (open && session) {
        const fresh = notes.find((x) => x.id === open!.id);
        if (!fresh && !search.value.trim()) closeNote();
        else if (fresh && session.take(fresh)) {
          open = fresh;
          drawTop();
        }
      }
      drawList();
    },
    flush: async () => {
      await save();
    },
    create: () => createNote(),
    finishDelete: () => noteUndo.finish(),
  };

  search.addEventListener("input", async () => {
    await load();
    drawList();
  });
  async function createNote() {
    let n: Note;
    try {
      n = await act.create();
      say("");
    } catch {
      return void say("Couldn't start a note. Try again in a moment.");
    }
    search.value = "";
    await load();
    if (!notes.some((x) => x.id === n.id)) notes = [n, ...notes];
    await view.open(n.id);
    session?.editor.focus();
  }
  create.addEventListener("click", () => void createNote());

  void load().then(drawList);
  return view;
}
