import type { Task, TaskGroup } from "./api";
import { fmtDur, h } from "./ui";

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
};

/// The running focus session's task, by id (null for a session on no task), or null with none.
export type RunningSession = { task: string | null } | null;

const GROUPS: [TaskGroup, string][] = [
  ["overdue", "Overdue"],
  ["today", "Today"],
  ["upcoming", "Upcoming"],
  ["none", "No date"],
];

const PRIORITY = ["Low", "Medium", "High"];

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
  tick.addEventListener("click", () => act.done(t.id, !done));
  const el = h("div", { class: `task-row${done ? " done" : ""}`, "data-task": t.id }, tick);
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
  // Typing several tasks in a row keeps the field, though each add draws the list anew.
  const typing = document.activeElement?.classList.contains("task-add") && root.contains(document.activeElement);
  const doneOpen = root.querySelector<HTMLDetailsElement>("details.task-done")?.open ?? false;
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
  const parts: Node[] = [add, preview];
  if (!tasks.length) parts.push(h("p", { class: "muted ledger-empty" }, "Add what you're working on. Give it a day with “fri” or a priority with “!h”."));
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
  root.replaceChildren(...parts);
  if (typing) add.focus();
}

/// "2026-11-08T15:00" for a time, in this computer's zone, as a datetime-local field takes it.
function localInput(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/// Draws the detail panel for `t`: title, due day, priority, exact reminder, focus time, Delete.
export function renderDetail(root: HTMLElement, t: Task, act: LedgerActions): void {
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
    h("p", { class: "muted" }, t.focusedMs > 0 ? `${fmtDur(t.focusedMs)} focused` : "No focus time yet"),
    remove,
  );
}
