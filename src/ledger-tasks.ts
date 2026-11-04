import type { Task, TaskGroup } from "./api";
import { calendar, isoDate, parseDate, todayIso } from "./calendar";
import { glyph } from "./glyphs";
import { logo } from "./logos";
import { timeSelect } from "./time-select";
import { fmtDur, h } from "./ui";

/// How long the ink stroke takes before a ticked task moves to Done.
const INK_MS = 280;
const stillPlease = () => !!window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;

/// What Ledger's task list and detail panel ask the page to do.
export type LedgerActions = {
  /** Quick-add text, shortcuts and all. */
  add(text: string): void | Promise<void>;
  /** What the field's text reads as, for under it. */
  preview(text: string): Promise<string>;
  done(id: string, done: boolean): void;
  start(id: string): void;
  select(id: string): void;
  rename(id: string, title: string): void;
  plan(id: string, due: string | null, priority: number, remind: number | null): void;
  remove(id: string): void;
  /** How long Start's focus session is, to say on it. */
  focusMinutes?(): number;
  /** Ends the running focus session. */
  stop?(): void;
  openNote(id: string): void;
  /** Attaches a note to a task, or (null) detaches it. */
  attach(task: string, note: string | null): void;
  /** A new note headed with the task's title, attached to it. */
  newNoteFor(task: string, title: string): void;
};

/// A note as the task panel names it.
export type NoteName = { id: string; title: string };

/// The running focus session's task, by id (null for a session on no task), or null with none.
export type RunningSession = { task: string | null; endsMs?: number } | null;

/// Whole minutes left in a session ending at `endsMs`, at least one while it runs.
export function minutesLeft(endsMs: number, now: number): number {
  return Math.max(1, Math.ceil((endsMs - now) / 60_000));
}

const GROUPS: [TaskGroup, string][] = [
  ["overdue", "Overdue"],
  ["today", "Today"],
  ["upcoming", "Upcoming"],
  ["none", "No date"],
];

const PRIORITY = ["Low", "Medium", "High"];

/// Ledger's line on the dashboard: "3 due today · 1 overdue", or "Nothing due today".
export function ledgerLine(tasks: Task[]): string {
  const today = tasks.filter((t) => t.group === "today").length;
  const late = tasks.filter((t) => t.group === "overdue").length;
  const parts = [today ? `${today} due today` : "", late ? `${late} overdue` : ""].filter(Boolean);
  return parts.length ? parts.join(" · ") : "Nothing due today";
}

/// The line under the Tasks title: "3 due today · 1 overdue · 4 open".
export function taskSummary(tasks: Task[]): string {
  const open = tasks.filter((t) => t.doneMs === null).length;
  if (!tasks.length) return "Nothing to do yet";
  const today = tasks.filter((t) => t.group === "today").length;
  const late = tasks.filter((t) => t.group === "overdue").length;
  const due = [today ? `${today} due today` : "", late ? `${late} overdue` : ""].filter(Boolean);
  return [...(due.length ? due : ["Nothing due"]), `${open} open`].join(" · ");
}

const HINT = "Try “essay draft !h fri” for High priority, due Friday";

/// "Fri 6 Nov" for "2026-11-06".
export function dayText(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const weekday = date.toLocaleDateString("en-GB", { weekday: "short" });
  const month = date.toLocaleDateString("en-GB", { month: "short" });
  return `${weekday} ${d} ${month}`;
}

/// How each open task's card turns its title into a field, for F2.
const renamers = new WeakMap<HTMLElement, () => void>();

function row(t: Task, session: RunningSession, act: LedgerActions, selected: string | null = null): HTMLElement {
  const done = t.doneMs !== null;
  // Named for its task, so a screen reader moving down the list knows which one it would tick.
  // Reached with the list's keys (Space), not Tab: the list is one Tab stop.
  const tick = h("input", { type: "checkbox", tabindex: "-1", "aria-label": done ? `Reopen “${t.title}”` : `Mark “${t.title}” done` }) as HTMLInputElement;
  tick.checked = done;
  const open = t.id === selected;
  const el = h("div", { class: `task-row task-card${done ? " done" : ""}${t.group === "overdue" ? " ribbon" : ""}${open ? " open" : ""}`, "data-task": t.id }, tick);
  tick.addEventListener("click", () => {
    // Ticking off draws an ink stroke through it first; reopening is at once.
    if (done || stillPlease()) return act.done(t.id, !done);
    el.classList.add("inking");
    window.setTimeout(() => act.done(t.id, true), INK_MS);
  });
  // Medium is every task's to begin with, so it wears no mark: High a filled dot, Low a faint ring.
  if (t.priority !== 1) {
    const name = PRIORITY[Math.min(t.priority, 2)];
    el.append(h("span", { class: `prio-dot prio-${name.toLowerCase()}`, role: "img", title: `${name} priority`, "aria-label": `${name} priority` }));
  }
  const title = h("button", { class: "task-title", tabindex: "-1", title: "Details", "aria-expanded": String(open), "aria-controls": "task-detail", "aria-describedby": "task-keys" }, t.title);
  title.addEventListener("click", () => act.select(t.id));
  // F2 turns the title into a field, to rename it where it is (the panel's title renames it too);
  // no bubble on the card, where it only added a control to find. A done task isn't renamed.
  const main = h("div", { class: "task-main" }, title);
  if (!done)
    renamers.set(el, () => {
      const field = renameField(t.title, "task-rename", (text) => act.rename(t.id, text), () => field.replaceWith(title));
      title.replaceWith(field);
      field.focus();
      field.select();
    });
  el.append(main);
  // Its details as chips: when it's due (today's group says that already), its reminder, its
  // note, the time spent on it.
  const chips = h("div", { class: "task-chips" });
  if (t.dueDay && t.group !== "today") chips.append(h("span", { class: `task-day chip${t.group === "overdue" ? " late" : ""}` }, h("span", { class: "sr-only" }, "Due "), dayText(t.dueDay)));
  if (t.remindMs !== null) {
    const at = new Date(t.remindMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    // What each chip means is in words too, unseen: a screen reader reads a span's text, not a label on it.
    chips.append(h("span", { class: "task-bell chip", title: `Reminder at ${at}` }, h("span", { class: "sr-only" }, "Reminder at "), glyph("bell"), at));
  }
  if (t.note) chips.append(h("span", { class: "task-note chip", title: "Has a note" }, glyph("note"), h("span", { class: "sr-only" }, "Has a note")));
  if (t.focusedMs > 0) chips.append(h("span", { class: "task-time chip", title: "Focused on it" }, h("span", { class: "sr-only" }, "Focused "), fmtDur(t.focusedMs)));
  el.append(chips);
  if (!done) {
    // While a session runs, Start would end it, so only the running task's card says anything.
    if (session && t.id === session.task) {
      // The session acts on real apps: how long it has left, and a way to end it, where it shows.
      const left = session.endsMs ? ` · ${minutesLeft(session.endsMs, Date.now())}m left` : "";
      const stop = h("button", { class: "link-btn task-stop", tabindex: "-1", "aria-label": `Stop focusing on ${t.title}` }, "Stop");
      stop.addEventListener("click", () => act.stop?.());
      el.append(h("span", { class: "task-now" }, `Focusing now${left} · `, stop));
    } else if (!session) {
      const minutes = act.focusMinutes?.() ?? 25;
      // Its length on it, as well as in words: the session it starts blocks apps for that long.
      // Said in full where it can be: it blocks distracting apps, not just a timer.
      const start = h(
        "button",
        { class: "task-start", tabindex: "-1", title: `Focus ${minutes} min: blocks distracting apps`, "aria-label": `Focus ${minutes} minutes on ${t.title}, blocking distracting apps` },
        glyph("play"),
        h("span", { class: "task-start-len" }, `${minutes}m`),
      );
      start.addEventListener("click", () => act.start(t.id));
      el.append(start);
    }
  }
  return el;
}

/// Draws the Tasks view: the field to add one (with what it reads under it), the open tasks by
/// group, and the done ones folded away under "Done (n)".
/// The small "Rename" bubble shown beside a name under the pointer.
export function renameBubble(name: string, rename: () => void): HTMLElement {
  const bubble = h("button", { class: "rename-bubble", "aria-label": `Rename ${name}`, title: "Rename" }, glyph("pencil"), "Rename");
  bubble.addEventListener("click", (e) => {
    e.stopPropagation();
    rename();
  });
  return bubble;
}

/// A name turned into a field: Enter or leaving it saves a new name, Esc or an emptied one puts the
/// old one back (`done` is called either way, after `save` when it saves).
export function renameField(name: string, className: string, save: (text: string) => void, done: () => void): HTMLInputElement {
  const field = h("input", { class: `filter ${className}`, maxlength: "80", "aria-label": "New name" }) as HTMLInputElement;
  field.value = name;
  let finished = false;
  const finish = (keep: boolean) => {
    if (finished) return;
    finished = true;
    const text = field.value.trim();
    if (keep && text && text !== name) save(text);
    done();
  };
  field.addEventListener("keydown", (e) => {
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") {
      e.stopPropagation();
      finish(false);
    }
  });
  field.addEventListener("blur", () => finish(true));
  field.addEventListener("click", (e) => e.stopPropagation());
  return field;
}

/// What each list last drew, so a refresh that changes nothing leaves it be (and the keyboard where
/// it is): Ledger asks again every few seconds.
const drawn = new WeakMap<HTMLElement, string>();

/// The controls on a card the keyboard can be on, by what they are.
const CONTROLS = ["input[type=checkbox]", ".task-title", ".task-start", ".task-stop"];

/// Where the keyboard is in the list: which task, which of its controls, and the open tasks after
/// it in order, to go to one of if this one has gone.
function focusIn(root: HTMLElement): { id: string; control: string; after: string[] } | null {
  const el = document.activeElement;
  const card = el instanceof HTMLElement && root.contains(el) ? el.closest<HTMLElement>("[data-task]") : null;
  if (!card) return null;
  const control = CONTROLS.find((c) => el!.matches(c)) ?? ".task-title";
  const open = [...root.querySelectorAll<HTMLElement>(".ledger-group [data-task]")].map((c) => c.dataset.task!);
  return { id: card.dataset.task!, control, after: open.slice(open.indexOf(card.dataset.task!) + 1) };
}

const cardOf = (root: HTMLElement, id: string) => [...root.querySelectorAll<HTMLElement>("[data-task]")].find((c) => c.dataset.task === id);

/// Puts the keyboard back where it was: the same control on the same task, if it's still to be
/// seen; else the next open task's tick, as after ticking one off.
function refocus(root: HTMLElement, was: { id: string; control: string; after: string[] }) {
  const card = cardOf(root, was.id);
  const seen = card && !card.closest("details:not([open])");
  if (seen) return (card.querySelector<HTMLElement>(was.control) ?? card.querySelector<HTMLElement>(".task-title"))?.focus();
  for (const id of was.after) {
    const next = cardOf(root, id);
    if (next && !next.closest("details:not([open])")) return next.querySelector<HTMLElement>("input[type=checkbox]")?.focus();
  }
  root.querySelector<HTMLElement>(".task-add")?.focus();
}

/// Draws the list; `query` is what Find a task holds, `tasks` already narrowed to it.
export function renderTaskList(root: HTMLElement, tasks: Task[], session: RunningSession, act: LedgerActions, selected: string | null = null, query = ""): void {
  // A task being renamed in the list keeps its field: the next redraw brings the changes.
  if (document.activeElement?.classList.contains("task-rename") && root.contains(document.activeElement)) return;
  // The minutes a running session has left are part of what's drawn, so it's redrawn as they change.
  const left = session?.endsMs ? minutesLeft(session.endsMs, Date.now()) : null;
  const shape = JSON.stringify([tasks, session, left, selected, act.focusMinutes?.() ?? 25, query]);
  if (drawn.get(root) === shape && root.childElementCount) return;
  drawn.set(root, shape);
  const was = focusIn(root);
  const { add, preview, help, spoken } = addField(root, act);
  // The same field, put back: what's typed in it, and the caret, stay through a redraw.
  const typing = document.activeElement === add;
  const doneOpen = root.querySelector<HTMLDetailsElement>("details.task-done")?.open ?? false;
  const parts: Node[] = [add, preview, spoken];
  if (!tasks.length && query) parts.push(h("p", { class: "muted ledger-empty", role: "status" }, `No tasks match “${query}”. Esc clears the search.`));
  else if (!tasks.length) {
    const art = h("div", { class: "ledger-empty-art", "aria-hidden": "true" });
    art.innerHTML = logo("ledger", 64);
    parts.push(art, h("p", { class: "muted ledger-empty" }, "Nothing to do yet. What are you working on?"));
  }
  for (const [group, name] of GROUPS) {
    const these = tasks.filter((t) => t.group === group);
    if (these.length) parts.push(h("section", { class: `ledger-group ${group}` }, h("h2", {}, name), ...these.map((t) => row(t, session, act, selected))));
  }
  const done = tasks.filter((t) => t.group === "done");
  // Everything ticked: said, rather than a list with only its Done fold.
  if (!query && done.length && done.length === tasks.length) parts.push(h("p", { class: "muted ledger-empty ledger-all-done" }, "All done. Nothing left open."));
  if (done.length) {
    const folded = h("details", { class: "task-done" }, h("summary", {}, `Done (${done.length})`), ...done.map((t) => row(t, session, act, selected))) as HTMLDetailsElement;
    folded.open = doneOpen;
    parts.push(folded);
  }
  const caret = typing ? [add.selectionStart, add.selectionEnd] : null;
  // The shorthand and keys, folded, after the list: help for when it is wanted, not in the way.
  parts.push(help, h("p", { id: "task-keys", class: "sr-only" }, "Up and down arrows move between tasks. Space ticks, Enter opens, F2 renames, Delete deletes, right arrow reaches Start or Stop."));
  root.replaceChildren(...parts);
  // The list is one Tab stop: the task the keyboard was last on, or the first.
  const titles = [...root.querySelectorAll<HTMLElement>("[data-task] .task-title")];
  const current = (was && titles.find((b) => b.closest<HTMLElement>("[data-task]")!.dataset.task === was.id && !b.closest("details:not([open])"))) || titles.find((b) => !b.closest("details:not([open])"));
  if (current) current.tabIndex = 0;
  if (caret) {
    add.focus();
    add.setSelectionRange(caret[0], caret[1]);
  } else if (was) refocus(root, was);
  listKeys(root, act);
}

const keyed = new WeakSet<HTMLElement>();

/// The task list's keys, on the task the keyboard is on: up and down move, Space ticks, Enter
/// opens (the title is a button), F2 renames, Delete deletes, right reaches Start and left comes
/// back. Wired once per list.
function listKeys(root: HTMLElement, act: LedgerActions) {
  if (keyed.has(root)) return;
  keyed.add(root);
  root.addEventListener("keydown", (e) => {
    const el = e.target as HTMLElement;
    const card = el.closest<HTMLElement>("[data-task]");
    if (!card || el.classList.contains("task-rename")) return;
    const titles = [...root.querySelectorAll<HTMLElement>("[data-task] .task-title")].filter((b) => !b.closest("details:not([open])"));
    const at = titles.indexOf(card.querySelector<HTMLElement>(".task-title")!);
    const go = (to: HTMLElement | undefined) => {
      if (!to) return;
      for (const b of titles) b.tabIndex = -1;
      to.tabIndex = 0;
      to.focus();
    };
    const id = card.dataset.task!;
    if (e.key === "ArrowDown") go(titles[at + 1]);
    else if (e.key === "ArrowUp") go(titles[at - 1]);
    else if (e.key === "Home") go(titles[0]);
    else if (e.key === "End") go(titles[titles.length - 1]);
    else if (e.key === " " && el.classList.contains("task-title")) card.querySelector<HTMLInputElement>("input[type=checkbox]")?.click();
    else if (e.key === "F2") renamers.get(card)?.();
    else if (e.key === "Delete") act.remove(id);
    // Right reaches Start, or Stop on the running task: a session started from here can always be
    // ended from here, by keyboard too.
    else if (e.key === "ArrowRight") card.querySelector<HTMLElement>(".task-start, .task-stop")?.focus();
    else if (e.key === "ArrowLeft" && el.matches(".task-start, .task-stop")) card.querySelector<HTMLElement>(".task-title")?.focus();
    else return;
    e.preventDefault();
  });
}

/// The list's "Add a task" field and the line under it saying what it reads, made once per list
/// and kept: Ledger redraws the list every few seconds, and a new field would drop what's typed.
const fields = new WeakMap<HTMLElement, { add: HTMLInputElement; preview: HTMLElement; help: HTMLElement; spoken: HTMLElement }>();

function addField(root: HTMLElement, act: LedgerActions): { add: HTMLInputElement; preview: HTMLElement; help: HTMLElement; spoken: HTMLElement } {
  const kept = fields.get(root);
  if (kept) return kept;
  const add = h("input", { class: "filter task-add", maxlength: "120", placeholder: "Add a task", "aria-label": "Add a task", autocomplete: "off" }) as HTMLInputElement;
  const preview = h("p", { class: "task-preview muted" }, HINT);
  // What it reads is said aloud once typing pauses, not at every key; trouble is said at once.
  const spoken = h("p", { class: "sr-only task-spoken", "aria-live": "polite" });
  let asked = 0;
  let pause = 0;
  const show = (text: string, now = false) => {
    preview.textContent = text;
    window.clearTimeout(pause);
    if (now) spoken.textContent = text;
    else pause = window.setTimeout(() => (spoken.textContent = text), 1200);
  };
  add.addEventListener("input", async () => {
    const text = add.value;
    const mine = ++asked;
    const shown = text.trim() ? await act.preview(text).catch(() => "") : HINT;
    // Only the latest answer, should an earlier one arrive after it.
    if (mine === asked) show(shown);
  });
  // One add at a time, so a second Enter before the first is kept can't add it twice.
  let adding = false;
  add.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter" || adding) return;
    const text = add.value.trim();
    if (!text) return;
    adding = true;
    asked++;
    try {
      await act.add(text);
      // Kept: the field empties, unless something new was typed meanwhile.
      if (add.value.trim() === text) add.value = "";
      show(HINT);
    } catch {
      // Not kept: what was typed stays, to try again.
      show("Couldn't add this. Press Enter to try again.", true);
    } finally {
      adding = false;
    }
  });
  // What the field understands, and the list's keys, folded away until wanted.
  const help = h(
    "details",
    { class: "task-help" },
    h("summary", {}, "Shorthand and keys"),
    h(
      "dl",
      {},
      h("dt", {}, "!h  !m  !l"),
      h("dd", {}, "High, Medium or Low priority (or !high, !med, !low)"),
      h("dt", {}, "today  tomorrow  fri"),
      h("dd", {}, "Due that day; a weekday is the next one"),
      h("dt", {}, "7 nov  11/7"),
      h("dd", {}, "Due on a date"),
      h("dt", {}, "↑ ↓  Space  Enter"),
      h("dd", {}, "Move between tasks, tick one, open it"),
      h("dt", {}, "F2  Delete  →"),
      h("dd", {}, "Rename, delete (Ctrl+Z brings it back), reach Start or Stop"),
      h("dt", {}, "Ctrl+N  /  Ctrl+1 2 3"),
      h("dd", {}, "New task, find, switch views"),
    ),
  );
  fields.set(root, { add, preview, help, spoken });
  return { add, preview, help, spoken };
}

/// "15:00" for a moment, in this computer's zone, as a time field takes it.
function clock(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
}

/// "Fri 8 Nov", with the year when it isn't this one.
function dayName(iso: string): string {
  const d = parseDate(iso);
  const year = d.getFullYear() === new Date().getFullYear() ? undefined : "numeric";
  return d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short", year });
}

/// A button that shows a day and opens Kairo's calendar under it, in place of a native date field.
function dayButton(label: string, day: string | null, empty: string, pick: (iso: string) => void): HTMLElement {
  const anchor = h("span", { class: "day-pick" });
  const button = h(
    "button",
    { type: "button", class: "filter day-field", "aria-haspopup": "dialog", "aria-label": `${label}: ${day ? dayName(day) : empty}` },
    day ? dayName(day) : empty,
  ) as HTMLButtonElement;
  button.classList.toggle("empty", !day);
  anchor.append(button);
  const cal = calendar({ anchor, trigger: button, label, selected: () => day, min: () => null, max: () => null, onPick: pick });
  button.addEventListener("click", () => cal.toggle());
  return anchor;
}

/// The panel's Note row: the attached note (open, detach), or a way to attach one: a search over
/// the notes, with a new one named after the task first.
function noteRow(t: Task, act: LedgerActions, notes: NoteName[]): HTMLElement {
  const row = h("div", { class: "detail-field detail-note" }, h("span", {}, "Note"));
  const attached = t.note ? notes.find((n) => n.id === t.note) : undefined;
  if (attached) {
    const open = h("button", { class: "link-btn open-note link-out" }, attached.title, glyph("out"));
    open.addEventListener("click", () => act.openNote(attached.id));
    const detach = h("button", { class: "link-btn detach-note", "aria-label": "Detach the note", title: "Detach the note" }, glyph("close"));
    detach.addEventListener("click", () => act.attach(t.id, null));
    row.append(open, detach);
    return row;
  }
  const start = h("button", { class: "link-btn attach-note" }, "+ Attach a note");
  start.addEventListener("click", () => {
    const find = h("input", { class: "filter", type: "search", placeholder: "Find a note", "aria-label": "Find a note" }) as HTMLInputElement;
    const options = h("div", { class: "note-pick-options" });
    const draw = () => {
      const q = find.value.trim().toLowerCase();
      const fresh = h("button", { class: "link-btn" }, `New note: ${t.title}`);
      fresh.addEventListener("click", () => act.newNoteFor(t.id, t.title));
      const found = notes
        .filter((n) => !q || n.title.toLowerCase().includes(q))
        .slice(0, 8)
        .map((n) => {
          const b = h("button", { class: "link-btn" }, n.title);
          b.addEventListener("click", () => act.attach(t.id, n.id));
          return b;
        });
      options.replaceChildren(fresh, ...found);
    };
    find.addEventListener("input", draw);
    draw();
    start.replaceWith(h("div", { class: "note-pick" }, find, options));
    find.focus();
  });
  row.append(start);
  return row;
}

/// Draws the detail panel for `t`: title, due day, priority, exact reminder, note, focus time,
/// Delete. `notes` names the notes it can be attached to.
export function renderDetail(root: HTMLElement, t: Task, act: LedgerActions, notes: NoteName[] = [], session: RunningSession = null): void {
  const title = h("input", { class: "filter detail-title", maxlength: "80", "aria-label": "Title" }) as HTMLInputElement;
  title.value = t.title;
  const saveTitle = () => {
    const text = title.value.trim();
    if (!text) title.value = t.title;
    else if (text !== t.title) act.rename(t.id, text);
  };
  title.addEventListener("keydown", (e) => {
    if (e.key === "Enter") saveTitle();
  });
  title.addEventListener("blur", saveTitle);

  // What a task is for, done from its panel too: tick it, or focus on it (or stop focusing).
  const done = t.group === "done";
  const tick = h("button", { class: "btn detail-done" }, glyph("check"), done ? "Reopen" : "Mark done");
  tick.addEventListener("click", () => act.done(t.id, !done));
  const actions: Node[] = [tick];
  if (!done && session && session.task === t.id) {
    const left = session.endsMs ? ` · ${minutesLeft(session.endsMs, Date.now())}m left` : "";
    const stop = h("button", { class: "btn detail-stop" }, "Stop focusing");
    stop.addEventListener("click", () => act.stop?.());
    actions.push(stop, h("span", { class: "muted detail-now" }, `Focusing now${left}`));
  } else if (!done && !session) {
    const minutes = act.focusMinutes?.() ?? 25;
    const start = h("button", { class: "btn detail-start", title: "Blocks distracting apps while it runs" }, glyph("play"), `Focus ${minutes} min`);
    start.addEventListener("click", () => act.start(t.id));
    actions.push(start, h("span", { class: "muted detail-now" }, "Blocks distracting apps"));
  }

  const due = dayButton("Due day", t.dueDay, "No due day", (iso) => act.plan(t.id, iso, t.priority, t.remindMs));
  // Clearing is offered only when there's a day to clear.
  const dueParts: Node[] = [due];
  if (t.dueDay) {
    const clearDue = h("button", { class: "link-btn clear-due", "aria-label": "Clear due day", title: "Clear due day" }, glyph("close"));
    clearDue.addEventListener("click", () => act.plan(t.id, null, t.priority, t.remindMs));
    dueParts.push(clearDue);
  }

  const priorities = h("div", { class: "range", role: "group", "aria-label": "Priority" });
  for (const p of [2, 1, 0]) {
    const b = h("button", { "data-priority": String(p), "aria-pressed": String(t.priority === p) }, PRIORITY[p]);
    b.addEventListener("click", () => act.plan(t.id, t.dueDay, p, t.remindMs));
    priorities.append(b);
  }

  // A reminder, once there is one, is a day from the calendar and a time beside it, each moving
  // it. Until then a single "+ Add a reminder" says so, rather than a time that looks set.
  const remindAt = (day: string, time: string) => {
    const at = new Date(`${day}T${time}`).getTime();
    if (!Number.isNaN(at)) act.plan(t.id, t.dueDay, t.priority, at);
  };
  let remind: Node[];
  if (t.remindMs === null) {
    const add = h("button", { class: "link-btn add-remind" }, "+ Add a reminder");
    // At 9 on the due day, or the next 9 o'clock: shown at once, to move from there.
    add.addEventListener("click", () => {
      const nine = new Date(`${t.dueDay ?? todayIso()}T09:00`);
      if (!t.dueDay && nine.getTime() <= Date.now()) nine.setDate(nine.getDate() + 1);
      act.plan(t.id, t.dueDay, t.priority, nine.getTime());
    });
    remind = [add];
  } else {
    const remindDay = isoDate(new Date(t.remindMs));
    const time = timeSelect(clock(t.remindMs), "Reminder time");
    time.classList.add("remind-time");
    time.addEventListener("change", () => {
      if (time.value) remindAt(remindDay, time.value);
    });
    const remindPick = dayButton("Reminder day", remindDay, "No reminder", (iso) => remindAt(iso, time.value || "09:00"));
    const clearRemind = h("button", { class: "link-btn clear-remind", "aria-label": "Clear reminder", title: "Clear reminder" }, glyph("close"));
    clearRemind.addEventListener("click", () => act.plan(t.id, t.dueDay, t.priority, null));
    remind = [h("div", { class: "remind-when" }, remindPick, time), clearRemind];
  }

  // Delete in the warning ink, as a note's is.
  const remove = h("button", { class: "link-btn danger-link detail-delete" }, "Delete");
  remove.addEventListener("click", () => act.remove(t.id));

  const close = h("button", { class: "detail-close", "aria-label": "Close", title: "Close (Esc)" }, glyph("close"));
  close.addEventListener("click", () => act.select(t.id));
  // Redrawn with the new plan, the keyboard stays on the control it was on: the same priority,
  // the same day button.
  const controls = () => [...root.querySelectorAll<HTMLElement>("button, input, select")];
  const at = controls().indexOf(document.activeElement as HTMLElement);
  root.replaceChildren(
    h("div", { class: "detail-head" }, title, close),
    h("div", { class: "detail-actions" }, ...actions),
    h("div", { class: "detail-field" }, h("span", {}, "Due"), ...dueParts),
    h("div", { class: "detail-field" }, h("span", {}, "Priority"), priorities),
    h("div", { class: "detail-field" }, h("span", {}, "Remind me"), ...remind),
    noteRow(t, act, notes),
    h("p", { class: "muted" }, t.focusedMs > 0 ? `${fmtDur(t.focusedMs)} focused` : "No focus time yet"),
    remove,
  );
  if (at >= 0) controls()[Math.min(at, controls().length - 1)]?.focus();
}
