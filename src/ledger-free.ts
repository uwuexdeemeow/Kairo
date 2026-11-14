import type { CalendarEvent, Task } from "./api";
import { estimateName } from "./ledger-cost";

// The free time left in today, worked out from the calendar and from tasks already placed in it,
// and where tasks with an estimate would fit. Placing one only sets its reminder, so nothing here
// is stored: it is all drawn again from the calendar and the tasks as they stand.

const MIN = 60_000;
/// The working day gaps are looked for in, in minutes into the day.
export type Hours = { fromMin: number; toMin: number };
export const USUAL_DAY: Hours = { fromMin: 9 * 60, toMin: 18 * 60 };

/// Hours from Ledger's settings, "09:00" and "18:00"; the usual day for anything that isn't one.
export function dayHours(from: string, to: string): Hours {
  const minutes = (t: string): number | null => {
    const m = /^(\d{1,2}):(\d{2})$/.exec(t);
    return m && +m[1] < 24 && +m[2] < 60 ? +m[1] * 60 + +m[2] : null;
  };
  const [a, b] = [minutes(from), minutes(to)];
  return a !== null && b !== null && b > a ? { fromMin: a, toMin: b } : USUAL_DAY;
}
/// A gap shorter than this isn't worth planning into, in minutes.
export const MIN_GAP = 15;
/// Starts are put on a multiple of this many minutes, so "2:35 pm" is never offered.
const ROUND = 5;
/// An event this long is an all-day one (or a stretch away), which says nothing about the hours.
const ALL_DAY_MS = 20 * 60 * MIN;

export type Span = { startMs: number; endMs: number };

/// `ms` moved up to the next multiple of `ROUND` minutes.
const roundUp = (ms: number): number => Math.ceil(ms / (ROUND * MIN)) * ROUND * MIN;

/// The day's working hours, for the day `ms` falls in.
function workday(ms: number, hours: Hours): Span {
  const d = new Date(ms);
  const at = (min: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate(), Math.floor(min / 60), min % 60).getTime();
  return { startMs: at(hours.fromMin), endMs: at(hours.toMin) };
}

/// A task already placed in today's hours: it has an estimate and a reminder still to come inside
/// the working day. Its time is taken, whatever else is free around it.
export function placedSpan(t: Pick<Task, "doneMs" | "remindMs" | "estimateMin">, now: number, hours: Hours = USUAL_DAY): Span | null {
  if (t.doneMs !== null || t.remindMs === null || t.estimateMin === null || t.remindMs <= now) return null;
  const day = workday(now, hours);
  if (t.remindMs < day.startMs || t.remindMs >= day.endMs) return null;
  return { startMs: t.remindMs, endMs: t.remindMs + t.estimateMin * MIN };
}

/// What takes today's time: meetings (not all-day ones), and tasks already placed.
export function busySpans(events: CalendarEvent[], tasks: Task[], now: number, hours: Hours = USUAL_DAY): Span[] {
  const placed = tasks.map((t) => placedSpan(t, now, hours)).filter((s): s is Span => s !== null);
  return [...events.filter((e) => e.endMs - e.startMs < ALL_DAY_MS).map((e) => ({ startMs: e.startMs, endMs: e.endMs })), ...placed];
}

/// The stretches of today's working hours that are free from now on, at least `MIN_GAP` long.
export function freeGaps(busy: Span[], now: number, hours: Hours = USUAL_DAY): Span[] {
  const day = workday(now, hours);
  let from = Math.max(day.startMs, roundUp(now));
  const gaps: Span[] = [];
  const keep = (startMs: number, endMs: number) => {
    if (endMs - startMs >= MIN_GAP * MIN) gaps.push({ startMs, endMs });
  };
  for (const b of [...busy].sort((a, c) => a.startMs - c.startMs)) {
    if (b.endMs <= from) continue;
    if (b.startMs >= day.endMs) break;
    keep(from, Math.min(b.startMs, day.endMs));
    from = Math.max(from, roundUp(b.endMs));
  }
  if (from < day.endMs) keep(from, day.endMs);
  return gaps;
}

/// Where each task would go: the first gap it fits in, in the order given, each placement taking
/// its time from the gap. A task too long for every gap has no place.
export function suggest(tasks: Pick<Task, "id" | "estimateMin">[], gaps: Span[]): Map<string, number> {
  const left = gaps.map((g) => ({ ...g }));
  const places = new Map<string, number>();
  for (const t of tasks) {
    if (t.estimateMin === null) continue;
    const need = t.estimateMin * MIN;
    const gap = left.find((g) => g.endMs - g.startMs >= need);
    if (!gap) continue;
    places.set(t.id, gap.startMs);
    gap.startMs = roundUp(gap.startMs + need);
  }
  return places;
}

/// "2:00 pm", in the person's own clock.
export const clockName = (ms: number): string => new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

/// The line under the day's heading: "3h 10m free today, next 2:00 – 3:30 pm"; empty when there is
/// none left to speak of.
export function freeLine(gaps: Span[]): string {
  if (!gaps.length) return "";
  const total = Math.round(gaps.reduce((sum, g) => sum + (g.endMs - g.startMs), 0) / MIN);
  return `${estimateName(total)} free today, next ${clockName(gaps[0].startMs)} – ${clockName(gaps[0].endMs)}`;
}
