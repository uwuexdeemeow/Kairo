import type { Note, Paper } from "./api";
import { renameBubble, renameField } from "./ledger-tasks";
import { logo } from "./logos";
import { openNoteSession, type NoteSession } from "./note-session";
import { h } from "./ui";

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
  const popList = h("button", { class: "link-btn notes-pop-out", title: "All your notes in a small window, beside your work" }, "Pop out");
  popList.addEventListener("click", () => act.popOut());
  const list = h("div", { class: "notes-list", role: "list" });
  const pane = h("section", { class: "note-pane", "aria-label": "Note" });
  const shell = h("div", { class: "notes" }, h("div", { class: "notes-side" }, search, h("div", { class: "notes-side-actions" }, create, popList), list), pane);
  root.replaceChildren(shell);

  let notes: Note[] = [];
  let open: Note | null = null;
  // The open note being written in: its editor, saving and footer.
  let session: NoteSession | null = null;
  let asked = 0;

  const drawList = () => {
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
    // The open one is saved through its session, which shows the kept note when it's done.
    if (open?.id === n.id && session) return session.replace(retitle(session.editor.value(), title));
    const kept = await act.save(n.id, retitle(n.body, title));
    if (kept) notes = notes.map((x) => (x.id === n.id ? kept : x));
    drawList();
  };

  const load = async () => {
    const mine = ++asked;
    const found = await act.search(search.value.trim());
    // Only the latest answer, should an earlier search finish after it.
    if (mine === asked) notes = found;
  };

  /// Saves what's typed: whether it's kept. A note that isn't stays open, its footer saying so,
  /// rather than being put away with the typing lost.
  const save = async (): Promise<boolean> => (session ? session.save() : true);

  const closeNote = () => {
    session?.close();
    session = null;
    open = null;
    shell.classList.remove("reading");
    drawList();
  };

  /// With no note open, the pane offers the notes edited last, and a new one.
  const drawWelcome = () => {
    const newNote = h("button", { class: "btn primary notes-recent-new" }, "+ New note");
    newNote.addEventListener("click", () => void createNote());
    const tip = h("p", { class: "muted notes-recent-tip" }, "Pop out a note to keep it beside your work · Ctrl+B bold, Ctrl+I italic · - [ ] makes a checkbox");
    const recent = [...notes].sort((a, b) => b.editedMs - a.editedMs).slice(0, 3);
    if (!recent.length) {
      const art = h("div", { class: "notes-empty-art", "aria-hidden": "true" });
      art.innerHTML = logo("ledger", 88);
      pane.replaceChildren(h("div", { class: "notes-recent notes-recent-none" }, art, newNote, tip));
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
      h("div", { class: "notes-recent" }, h("h2", {}, "Pick up where you left off"), h("div", { class: "notes-recent-cards" }, ...cards), h("div", { class: "notes-recent-foot" }, newNote, tip)),
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
      if (await save()) closeNote();
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
    const popOut = h("button", { class: "link-btn note-pop-out", title: "Keep it in view in a small window of its own" }, "Pop out");
    popOut.addEventListener("click", async () => {
      // Saved first, so the window opens on what's typed.
      if (await save()) act.popOut(n.id);
    });
    const del = h("button", { class: "link-btn note-delete" }, "Delete");
    del.addEventListener("click", () => {
      // Asked already: pressing again points at the question rather than asking twice.
      const asking = pane.querySelector<HTMLElement>(".note-confirm");
      if (asking) {
        asking.querySelector<HTMLButtonElement>(".btn")?.focus();
        return;
      }
      const yes = h("button", { class: "btn danger" }, "Delete");
      const keep = h("button", { class: "link-btn note-keep" }, "Keep");
      const confirm = h("div", { class: "note-confirm", role: "alert" }, h("span", {}, `Delete “${n.title}”? Tasks attached to it lose the link.`), yes, keep);
      // Kept: the question goes, and the keyboard goes back to Delete where it was.
      const dismiss = () => {
        confirm.remove();
        del.focus();
      };
      keep.addEventListener("click", dismiss);
      confirm.addEventListener("keydown", (e) => {
        if (e.key !== "Escape") return;
        // The question's own Esc: it doesn't also close what's behind it.
        e.stopPropagation();
        dismiss();
      });
      yes.addEventListener("click", async () => {
        if (yes.hasAttribute("disabled")) return;
        yes.setAttribute("disabled", "");
        // Nothing typed is saved into a note being deleted.
        session?.close();
        await act.remove(n.id);
        closeNote();
        await load();
        drawList();
      });
      top.after(confirm);
      // The keyboard goes to the question, on its safe answer.
      keep.focus();
    });
    const tools = session ? [session.tools(), session.paper] : [];
    // Writing tools, then the note's own: Pop out and Pin, and apart from them Delete.
    top.replaceChildren(...parts, h("span", { class: "note-top-buttons" }, ...tools, popOut, pin, h("span", { class: "note-top-sep", "aria-hidden": "true" }), del));
  };

  const showLimit = () => (limit.textContent = session && session.editor.value().length >= MAX_CHARS ? "This note is at its limit." : "");

  const view: NotesView = {
    open: async (id) => {
      if (!(await save())) return;
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
  };

  search.addEventListener("input", async () => {
    await load();
    drawList();
  });
  async function createNote() {
    const n = await act.create();
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
