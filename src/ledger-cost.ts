import type { Task } from "./api";
import { fmtDur } from "./ui";

// What a task costs: its estimate, the focus time spent on it, and how a day's estimates stand
// against the focus a day usually holds. Every figure that is derived from history says nothing
// until there is enough behind it.

/// The estimates the panel offers, in minutes.
export const ESTIMATES = [15, 30, 45, 60, 90, 120, 180, 240, 360, 480];

/// "45m", "2h", "1h 30m" for minutes.
export function estimateName(min: number): string {
  if (min < 60) return `${min}m`;
  const [h, m] = [Math.floor(min / 60), min % 60];
  return m ? `${h}h ${m}m` : `${h}h`;
}

/// What a task's row and panel say of its cost: "~2h" before any focus, "1h 35m of ~2h" while it's
/// being spent, plain "1h 35m" with no estimate; nothing for a task with neither.
export function costText(t: Pick<Task, "estimateMin" | "focusedMs">): string {
  const spent = t.focusedMs > 0 ? fmtDur(t.focusedMs) : "";
  if (t.estimateMin === null) return spent;
  const estimate = `~${estimateName(t.estimateMin)}`;
  return spent ? `${spent} of ${estimate}` : estimate;
}

/// The minutes of estimates on the tasks still to do today or overdue.
export function plannedMinutes(tasks: Pick<Task, "group" | "doneMs" | "estimateMin">[]): number {
  return tasks.filter((t) => t.doneMs === null && (t.group === "today" || t.group === "overdue")).reduce((sum, t) => sum + (t.estimateMin ?? 0), 0);
}

/// The day's load in words: "4h planned today", and once a usual day's focus is known, whether it
/// is more than that ("4h planned today, more than the 3h you usually focus"). Nothing without any
/// estimate on the day's tasks.
export function loadLine(plannedMin: number, usualMs: number | null): string {
  if (plannedMin <= 0) return "";
  const planned = `${estimateName(plannedMin)} planned today`;
  const usualMin = usualMs === null ? null : Math.round(usualMs / 60_000);
  return usualMin !== null && plannedMin > usualMin ? `${planned}, more than the ${estimateName(usualMin)} you usually focus` : planned;
}

/// Finished tasks it takes to say how someone's time compares with their estimates, and the least
/// focus on one for it to count.
const PACE_MIN_TASKS = 5;
const PACE_MIN_FOCUS_MS = 10 * 60_000;

/// The middle of how long finished tasks took against their estimates (1 is as estimated, 2 twice
/// as long); null until there are five with both an estimate and real focus time.
export function paceRatio(tasks: Pick<Task, "doneMs" | "estimateMin" | "focusedMs">[]): number | null {
  const ratios = tasks
    .filter((t) => t.doneMs !== null && t.estimateMin !== null && t.focusedMs >= PACE_MIN_FOCUS_MS)
    .map((t) => t.focusedMs / (t.estimateMin! * 60_000))
    .sort((a, b) => a - b);
  if (ratios.length < PACE_MIN_TASKS) return null;
  const mid = Math.floor(ratios.length / 2);
  return ratios.length % 2 ? ratios[mid] : (ratios[mid - 1] + ratios[mid]) / 2;
}

/// What the pace says, only when it is far enough from 1 to be worth saying.
export function paceLine(ratio: number | null): string {
  if (ratio === null) return "";
  if (ratio >= 1.8) return "You usually take about twice what you estimate.";
  if (ratio >= 1.3) return "You usually take about half as long again as you estimate.";
  if (ratio <= 0.6) return "You usually take about half what you estimate.";
  return "";
}
