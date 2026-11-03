import type { Task, TaskGroup } from "./api";
import { logo } from "./logos";
import { fmtDur, h } from "./ui";

/// How long the ink stroke takes before a ticked task moves to Done.
const INK_MS = 280;
const stillPlease = () => !!window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;

/// What Ledger's task list and detail panel ask the page to do.
export type LedgerActions = {
  /** Quick-add text, shortcuts and all. */
  add(text: string): void;
  /** What the field's text reads as, for under it. */
  preview(text: string): Promise<string>;
  done(id: string, done: boolean): void;
  start(id: string): void;
  select(id: string): void;
  rename(id: string, title: string): void;
  plan(id: string, due: string | null, priority: number, remind: number | null): void;
  remove(id: string): void;
  openNote(id: string): void;
  /** Attaches a note to a task, or (null) detaches it. */
  attach(task: string, note: string | null): void;
  /** A new note headed with the task's title, attached to it. */
  newNoteFor(task: string, title: string): void;
};

/// A note as the task panel names it.
export type NoteName = { id: string; title: string };

/// The running focus session's task, by id (null for a session on no task), or null with none.
export type RunningSession = { task: string | null } | null;

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

/// "Fri 6 Nov" for "2026-11-06".
export function dayText(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const weekday = date.toLocaleDateString("en-GB", { weekday: "short" });
  const month = date.toLocaleDateString("en-GB", { month: "short" });
  return `${weekday} ${d} ${month}`;
}

function row(t: Task, session: RunningSession, act: LedgerActions): HTMLElement {
  const done = t.doneMs !== null;
  const tick = h("input", { type: "checkbox", "aria-label": done ? "Reopen" : "Mark done" }) as HTMLInputElement;
  tick.checked = done;
  const el = h("div", { class: `task-row${done ? " done" : ""}${t.group === "overdue" ? " ribbon" : ""}`, "data-task": t.id }, tick);
  tick.addEventListener("click", () => {
    // Ticking off draws an ink stroke through it first; reopening is at once.
    if (done || stillPlease()) return act.done(t.id, !done);
    el.classList.add("inking");
    window.setTimeout(() => act.done(t.id, true), INK_MS);
  });
  if (t.priority >= 1) {
    const name = PRIORITY[Math.min(t.priority, 2)];
    el.append(h("span", { class: `prio-dot prio-${name.toLowerCase()}`, title: `${name} priority`, "aria-label": `${name} priority` }));
  }
  const title = h("button", { class: "task-title", title: "Details" }, t.title);
  title.addEventListener("click", () => act.select(t.id));
  el.append(title);
  // Today's group says the day already.
  if (t.dueDay && t.group !== "today") el.append(h("span", { class: "task-day muted" }, dayText(t.dueDay)));
  if (t.remindMs !== null) el.append(h("span", { class: "task-bell", title: "Has a reminder", "aria-label": "Has a reminder" }, "🔔"));
  if (t.note) el.append(h("span", { class: "task-note", title: "Has a note", "aria-label": "Has a note" }, "📝"));
  el.append(h("span", { class: "task-time muted" }, t.focusedMs > 0 ? fmtDur(t.focusedMs) : ""));
  if (!done) {
    // While a session runs, Start would end it, so only the running task's row says anything.
    if (session && t.id === session.task) el.append(h("span", { class: "task-now" }, "Focusing now"));
    else if (!session) {
      const start = h("button", { class: "btn task-start", title: "Start a focus session on it" }, "Start focus");
      start.addEventListener("click", () => act.start(t.id));
      el.append(start);
    }
  }
  return el;
}

/// Draws the Tasks view: the field to add one (with what it reads under it), the open tasks by
/// group, and the done ones folded away under "Done (n)".
export function renderTaskList(root: HTMLElement, tasks: Task[], session: RunningSession, act: LedgerActions): void {
  const { add, preview } = addField(root, act);
  // The same field, put back: what's typed in it, and the caret, stay through a redraw.
  const typing = document.activeElement === add;
  const doneOpen = root.querySelector<HTMLDetailsElement>("details.task-done")?.open ?? false;
  const parts: Node[] = [add, preview];
  if (!tasks.length) {
    const art = h("div", { class: "ledger-empty-art", "aria-hidden": "true" });
    art.innerHTML = logo("ledger", 64);
    parts.push(art, h("p", { class: "muted ledger-empty" }, "Add what you're working on. Give it a day with “fri” or a priority with “!h”."));
  }
  for (const [group, name] of GROUPS) {
    const these = tasks.filter((t) => t.group === group);
    if (these.length) parts.push(h("section", { class: `ledger-group ${group}` }, h("h3", {}, name), ...these.map((t) => row(t, session, act))));
  }
  const done = tasks.filter((t) => t.group === "done");
  if (done.length) {
    const folded = h("details", { class: "task-done" }, h("summary", {}, `Done (${done.length})`), ...done.map((t) => row(t, session, act))) as HTMLDetailsElement;
    folded.open = doneOpen;
    parts.push(folded);
  }
  const caret = typing ? [add.selectionStart, add.selectionEnd] : null;
  root.replaceChildren(...parts);
  if (caret) {
    add.focus();
    add.setSelectionRange(caret[0], caret[1]);
  }
}

/// The list's "Add a task" field and the line under it saying what it reads, made once per list
/// and kept: Ledger redraws the list every few seconds, and a new field would drop what's typed.
const fields = new WeakMap<HTMLElement, { add: HTMLInputElement; preview: HTMLElement }>();

function addField(root: HTMLElement, act: LedgerActions): { add: HTMLInputElement; preview: HTMLElement } {
  const kept = fields.get(root);
  if (kept) return kept;
  const add = h("input", { class: "filter task-add", maxlength: "120", placeholder: "Add a task", "aria-label": "Add a task", autocomplete: "off" }) as HTMLInputElement;
  const preview = h("p", { class: "task-preview muted", "aria-live": "polite" });
  let asked = 0;
  add.addEventListener("input", async () => {
    const text = add.value;
    const mine = ++asked;
    const shown = text.trim() ? await act.preview(text).catch(() => "") : "";
    // Only the latest answer, should an earlier one arrive after it.
    if (mine === asked) preview.textContent = shown;
  });
  add.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    const text = add.value.trim();
    // Emptied at once, so a second Enter before the redraw can't add it twice.
    add.value = "";
    preview.textContent = "";
    asked++;
    if (text) act.add(text);
  });
  fields.set(root, { add, preview });
  return { add, preview };
}

/// "2026-11-08T15:00" for a time, in this computer's zone, as a datetime-local field takes it.
function localInput(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/// The panel's Note row: the attached note (open, detach), or a way to attach one: a search over
/// the notes, with a new one named after the task first.
function noteRow(t: Task, act: LedgerActions, notes: NoteName[]): HTMLElement {
  const row = h("div", { class: "detail-field detail-note" }, h("span", {}, "Note"));
  const attached = t.note ? notes.find((n) => n.id === t.note) : undefined;
  if (attached) {
    const open = h("button", { class: "link-btn open-note" }, `${attached.title} ↗`);
    open.addEventListener("click", () => act.openNote(attached.id));
    const detach = h("button", { class: "link-btn detach-note", "aria-label": "Detach the note", title: "Detach the note" }, "×");
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
export function renderDetail(root: HTMLElement, t: Task, act: LedgerActions, notes: NoteName[] = []): void {
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

  const due = h("input", { type: "date", class: "filter", "aria-label": "Due day" }) as HTMLInputElement;
  due.value = t.dueDay ?? "";
  due.addEventListener("change", () => act.plan(t.id, due.value || null, t.priority, t.remindMs));
  const clearDue = h("button", { class: "link-btn clear-due", "aria-label": "No due day", title: "No due day" }, "×");
  clearDue.addEventListener("click", () => act.plan(t.id, null, t.priority, t.remindMs));

  const priorities = h("div", { class: "range", role: "group", "aria-label": "Priority" });
  for (const p of [2, 1, 0]) {
    const b = h("button", { "data-priority": String(p), "aria-pressed": String(t.priority === p) }, PRIORITY[p]);
    b.addEventListener("click", () => act.plan(t.id, t.dueDay, p, t.remindMs));
    priorities.append(b);
  }

  const remind = h("input", { type: "datetime-local", class: "filter", "aria-label": "Reminder" }) as HTMLInputElement;
  remind.value = t.remindMs !== null ? localInput(t.remindMs) : "";
  remind.addEventListener("change", () => {
    const at = remind.value ? new Date(remind.value).getTime() : null;
    act.plan(t.id, t.dueDay, t.priority, at !== null && !Number.isNaN(at) ? at : null);
  });
  const clearRemind = h("button", { class: "link-btn clear-remind", "aria-label": "No reminder", title: "No reminder" }, "×");
  clearRemind.addEventListener("click", () => act.plan(t.id, t.dueDay, t.priority, null));

  const remove = h("button", { class: "btn detail-delete" }, "Delete");
  remove.addEventListener("click", () => act.remove(t.id));

  root.replaceChildren(
    title,
    h("label", { class: "detail-field" }, h("span", {}, "Due"), due, clearDue),
    h("div", { class: "detail-field" }, h("span", {}, "Priority"), priorities),
    h("label", { class: "detail-field" }, h("span", {}, "Remind me"), remind, clearRemind),
    noteRow(t, act, notes),
    h("p", { class: "muted" }, t.focusedMs > 0 ? `${fmtDur(t.focusedMs)} focused` : "No focus time yet"),
    remove,
  );
}
