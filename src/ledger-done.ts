import type { Task } from "./api";
import { fmtClock, fmtDur, h } from "./ui";

/// Weeks the log shows at first, and how many more each "Show older" adds.
const WEEKS = 4;
const DAY_MS = 86_400_000;

/// What the Done view asks the page to do.
export type DoneActions = {
  /** Puts a done task back among the open ones. */
  reopen(id: string): void;
  /** Deletes it, with the page's Undo. */
  remove(id: string): void;
  openTask(id: string): void;
};

/// The local midnight at or before `ms`.
const dayStart = (ms: number): number => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
};

/// The Monday at or before `ms`, local midnight.
export function weekStart(ms: number): number {
  const d = new Date(dayStart(ms));
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - ((d.getDay() + 6) % 7)).getTime();
}

export type Day = { start: number; tasks: Task[] };

/// The done tasks, newest first, under their local day. `since` leaves out what was done before it.
export function byDay(tasks: Task[], since = 0): Day[] {
  const done = tasks.filter((t) => t.doneMs !== null && t.doneMs >= since).sort((a, b) => (b.doneMs ?? 0) - (a.doneMs ?? 0));
  const days: Day[] = [];
  for (const t of done) {
    const start = dayStart(t.doneMs!);
    const last = days[days.length - 1];
    if (last && last.start === start) last.tasks.push(t);
    else days.push({ start, tasks: [t] });
  }
  return days;
}

export type Totals = { done: number; focusedMs: number };

/// How many were done from `from` up to `to`, and the focus time on them.
export function totals(tasks: Task[], from: number, to: number): Totals {
  let done = 0;
  let focusedMs = 0;
  for (const t of tasks) {
    if (t.doneMs === null || t.doneMs < from || t.doneMs >= to) continue;
    done++;
    focusedMs += t.focusedMs;
  }
  return { done, focusedMs };
}

/// "Today", "Yesterday", "Mon 9 Nov", with the year once it isn't this one.
export function dayName(start: number, now: number): string {
  const today = dayStart(now);
  if (start === today) return "Today";
  if (start === dayStart(today - DAY_MS / 2)) return "Yesterday";
  const d = new Date(start);
  const year = d.getFullYear() === new Date(now).getFullYear() ? {} : { year: "numeric" as const };
  return d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short", ...year });
}

/// "12 done · 6h 20m focused"; the focus part only when there was some.
export function totalsLine(t: Totals): string {
  return `${t.done} done${t.focusedMs >= 60_000 ? ` · ${fmtDur(t.focusedMs)} focused` : ""}`;
}

export type DoneView = {
  /** Draws the log for these tasks, as the list changed or the view opened. */
  render(tasks: Task[]): void;
};

/// The Done view: the finished tasks by day, with this week against last.
export function mountDone(root: HTMLElement, act: DoneActions, now: () => number = Date.now): DoneView {
  let tasks: Task[] = [];
  let weeks = WEEKS;
  let query = "";

  const search = h("input", { class: "filter done-search", type: "search", placeholder: "Find", "aria-label": "Find a done task", autocomplete: "off" }) as HTMLInputElement;
  const week = h("p", { class: "done-week", "aria-live": "polite" });
  const list = h("div", { class: "done-list" });
  root.replaceChildren(h("div", { class: "done-head" }, week, search), list);

  const draw = () => {
    const t = now();
    const thisWeek = weekStart(t);
    const lastWeek = weekStart(thisWeek - DAY_MS / 2);
    const mine = totals(tasks, thisWeek, Infinity);
    const before = totals(tasks, lastWeek, thisWeek);
    week.replaceChildren(h("strong", {}, "This week "), totalsLine(mine), h("span", { class: "muted" }, ` · last week ${totalsLine(before)}`));

    const q = query.trim().toLowerCase();
    // Weeks back from this one: this week and the `weeks - 1` before it.
    let since = thisWeek;
    for (let i = 1; i < weeks; i++) since = weekStart(since - DAY_MS / 2);
    const all = tasks.filter((x) => x.doneMs !== null && (!q || x.title.toLowerCase().includes(q)));
    const shown = byDay(all, since);
    const hidden = all.length - shown.reduce((n, d) => n + d.tasks.length, 0);

    if (!all.length) {
      list.replaceChildren(h("p", { class: "muted done-empty" }, q ? "No done task matches." : "Nothing done yet. Tick a task off and it lands here."));
      return;
    }
    const nodes: Node[] = [];
    for (const day of shown) {
      nodes.push(h("h2", { class: "done-day" }, dayName(day.start, t)));
      for (const task of day.tasks) nodes.push(row(task));
    }
    if (hidden > 0) {
      const more = h("button", { class: "btn done-older", type: "button" }, `Show older (${hidden})`);
      more.addEventListener("click", () => {
        weeks += WEEKS;
        draw();
      });
      nodes.push(more);
    }
    list.replaceChildren(...nodes);
  };

  const row = (t: Task): HTMLElement => {
    const title = h("button", { class: "done-title", type: "button" }, t.title);
    title.addEventListener("click", () => act.openTask(t.id));
    const reopen = h("button", { class: "btn done-reopen", type: "button", "aria-label": `Reopen “${t.title}”` }, "Reopen");
    reopen.addEventListener("click", () => act.reopen(t.id));
    const del = h("button", { class: "btn done-delete", type: "button", "aria-label": `Delete “${t.title}”` }, "Delete");
    del.addEventListener("click", () => act.remove(t.id));
    const chips = [t.focusedMs >= 60_000 ? fmtDur(t.focusedMs) : "", fmtClock(t.doneMs!)].filter(Boolean);
    return h("div", { class: "done-row", "data-task": t.id }, title, h("span", { class: "muted done-meta" }, chips.join(" · ")), reopen, del);
  };

  search.addEventListener("input", () => {
    query = search.value;
    draw();
  });

  return {
    render: (all) => {
      tasks = all;
      draw();
    },
  };
}
