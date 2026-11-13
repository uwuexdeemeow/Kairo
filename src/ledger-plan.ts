import type { Task } from "./api";
import { isoDate, parseDate, todayIso } from "./calendar";
import { confirmTwice } from "./confirm";
import { costText, estimateName, loadLine, plannedMinutes } from "./ledger-cost";
import { totals, totalsLine } from "./ledger-done";
import { glyph } from "./glyphs";
import { fmtDur, h } from "./ui";

// Ledger's Plan view: the day looked at as a whole. In the morning what is due today and overdue,
// each kept, moved to tomorrow or put off until later, against what a day usually holds; in the
// evening what was done, and what is left to carry over.

const DAY_MS = 86_400_000;
/// From this hour on, the view wraps the day up instead of planning it.
const EVENING_FROM = 17;

/// What the Plan view asks the page to do.
export type PlanActions = {
  /** Gives a task another due day (null for none) and its reminder with it, leaving the rest of its plan. */
  move(id: string, due: string | null, remind: number | null): void;
  done(id: string, done: boolean): void;
  openTask(id: string): void;
};

export type PlanView = {
  /** Draws the day for these tasks, as the list changed or the view opened. */
  render(tasks: Task[]): void;
};

/// Whether the day is being wrapped up (from five o'clock) rather than planned.
export function isEvening(ms: number): boolean {
  return new Date(ms).getHours() >= EVENING_FROM;
}

/// The view's title: "Plan the day" or "Wrap up the day".
export function planTitle(ms: number): string {
  return isEvening(ms) ? "Wrap up the day" : "Plan the day";
}

/// The day after `iso`, as an ISO date.
export function dayAfter(iso: string): string {
  const d = parseDate(iso);
  return isoDate(new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1));
}

/// A reminder moved a day on, at the same time of day; None stays none.
export function remindTomorrow(remindMs: number | null): number | null {
  if (remindMs === null) return null;
  const d = new Date(remindMs);
  d.setDate(d.getDate() + 1);
  return d.getTime();
}

/// A reminder when its task is put off with no day: one already past goes (it could never fire),
/// one still to come stays.
export function remindLater(remindMs: number | null, now: number): number | null {
  return remindMs !== null && remindMs > now ? remindMs : null;
}

/// The local midnight at or before `ms`.
const dayStart = (ms: number): number => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
};

/// The tasks to look at: open, and due today or overdue, in Ledger's order.
export function toLookAt(tasks: Task[]): Task[] {
  return tasks.filter((t) => t.doneMs === null && (t.group === "overdue" || t.group === "today"));
}

/// The line that opens the view: how the day stands, in words.
export function leadLine(tasks: Task[], now: number, usualMs: number | null): string {
  const left = toLookAt(tasks);
  if (isEvening(now)) {
    const start = dayStart(now);
    const done = totals(tasks, start, start + DAY_MS);
    const said = done.done ? totalsLine(done) : "Nothing done today";
    return [said, left.length ? `${left.length} left` : done.done ? "everything due is done" : ""].filter(Boolean).join(" · ");
  }
  if (!left.length) return "Nothing due today.";
  return [`${left.length} to do`, loadLine(plannedMinutes(left), usualMs)].filter(Boolean).join(" · ");
}

/// The Plan view.
export function mountPlan(root: HTMLElement, act: PlanActions, usual: () => number | null = () => null, now: () => number = Date.now): PlanView {
  let tasks: Task[] = [];
  /// The row the keyboard goes to after the next draw: where an action left it.
  let refocus: number | null = null;

  const move = (t: Task, at: number, due: string | null, remind: number | null) => {
    refocus = at;
    act.move(t.id, due, remind);
  };

  const row = (t: Task, at: number): HTMLElement => {
    const tick = h("button", { class: "plan-tick", "aria-label": `Mark “${t.title}” done`, title: "Mark done" }, glyph("check")) as HTMLButtonElement;
    tick.addEventListener("click", () => {
      refocus = at;
      act.done(t.id, true);
    });
    const open = h("button", { class: "plan-title", title: "Open it" }, h("span", { class: "plan-name" }, t.title)) as HTMLButtonElement;
    open.addEventListener("click", () => act.openTask(t.id));
    if (t.group === "overdue") open.append(h("span", { class: "plan-late" }, "Overdue"));
    const cost = costText(t);
    if (cost) open.append(h("span", { class: "plan-cost", title: "Time spent on it, and its estimate" }, cost));
    const tomorrow = h("button", { class: "link-btn plan-act plan-tomorrow", "aria-label": `Move “${t.title}” to tomorrow`, title: "Due tomorrow" }, "Tomorrow") as HTMLButtonElement;
    tomorrow.addEventListener("click", () => move(t, at, dayAfter(todayIso()), remindTomorrow(t.remindMs)));
    const later = h("button", { class: "link-btn plan-act plan-later", "aria-label": `Put “${t.title}” off, with no day`, title: "No due day" }, "Later") as HTMLButtonElement;
    later.addEventListener("click", () => move(t, at, null, remindLater(t.remindMs, now())));
    return h("li", { class: "plan-row", "data-task": t.id }, tick, open, tomorrow, later);
  };

  const draw = () => {
    const t0 = now();
    const evening = isEvening(t0);
    const left = toLookAt(tasks);
    const lead = h("p", { class: "plan-lead" }, leadLine(tasks, t0, usual()));
    const parts: Node[] = [lead];

    parts.push(h("h2", { class: "plan-head" }, evening ? "Left over" : "To do today"));
    if (left.length) {
      parts.push(h("ul", { class: "plan-list" }, ...left.map(row)));
      if (left.length > 1) {
        const all = h("button", { class: "link-btn plan-all" }, "Move all to tomorrow") as HTMLButtonElement;
        confirmTwice(all, () => "Click again to move them all", async () => {
          const due = dayAfter(todayIso());
          for (const t of left) act.move(t.id, due, remindTomorrow(t.remindMs));
        });
        parts.push(all);
      }
    } else {
      parts.push(h("p", { class: "muted plan-empty" }, "Nothing due today or overdue."));
    }

    // What tomorrow already holds, so moving things there is no surprise.
    const tomorrow = dayAfter(todayIso());
    const next = tasks.filter((t) => t.doneMs === null && t.dueDay === tomorrow);
    if (next.length) {
      const planned = plannedMinutes(next.map((t) => ({ ...t, group: "today" as const })));
      parts.push(h("p", { class: "muted plan-tomorrow-line" }, `Tomorrow already has ${next.length} ${next.length === 1 ? "task" : "tasks"}${planned ? ` · ${estimateName(planned)} planned` : ""}.`));
    }

    // What was done today, however the day is framed.
    const start = dayStart(t0);
    const doneToday = tasks.filter((t) => t.doneMs !== null && t.doneMs >= start).sort((a, b) => (b.doneMs ?? 0) - (a.doneMs ?? 0));
    if (doneToday.length) {
      const focused = doneToday.reduce((sum, t) => sum + t.focusedMs, 0);
      parts.push(
        h("h2", { class: "plan-head" }, "Done today"),
        h(
          "ul",
          { class: "plan-done-list" },
          ...doneToday.map((t) => {
            const open = h("button", { class: "plan-title", title: "Open it" }, h("span", { class: "plan-name" }, t.title)) as HTMLButtonElement;
            open.addEventListener("click", () => act.openTask(t.id));
            if (t.focusedMs >= 60_000) open.append(h("span", { class: "plan-cost" }, fmtDur(t.focusedMs)));
            return h("li", { class: "plan-row plan-row-done" }, h("span", { class: "plan-check", "aria-hidden": "true" }, glyph("check")), open);
          }),
        ),
      );
      if (focused >= 60_000) parts.push(h("p", { class: "muted plan-focused" }, `${fmtDur(focused)} focused on them.`));
    }

    root.replaceChildren(...parts);
    if (refocus !== null) {
      const rows = root.querySelectorAll<HTMLElement>(".plan-list .plan-row");
      // Where the keyboard was, or the last one now: ready for the next.
      const target = rows[Math.min(refocus, rows.length - 1)];
      refocus = null;
      target?.querySelector<HTMLElement>(".plan-title")?.focus();
    }
  };

  return {
    render(next) {
      tasks = next;
      draw();
    },
  };
}
