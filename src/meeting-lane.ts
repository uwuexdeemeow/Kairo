import type { CalendarEvent } from "./api";
import { fmtDur } from "./ui";

/// The most meetings drawn on a day's timeline; the rest are counted under it.
export const MAX_BANDS = 40;

export type Band = {
  /** The meeting's name, or "Meeting" once the title retention has blanked it. */
  title: string;
  /** Clipped to the day. */
  start: number;
  end: number;
  /** Where it sits on the hours shown, as a percentage of their width. */
  left: number;
  width: number;
};

/// The meetings of a day as bands over the hours the timeline shows (`t0` for `span` milliseconds),
/// earliest first: each clipped to the day, those wholly outside the shown hours left out, at most
/// `MAX_BANDS` kept and the number left over reported.
export function meetingBands(events: CalendarEvent[], t0: number, span: number, dayStart: number, dayEnd: number): { bands: Band[]; more: number } {
  const all: Band[] = [];
  for (const e of events) {
    const start = Math.max(e.startMs, dayStart, t0);
    const end = Math.min(e.endMs, dayEnd, t0 + span);
    if (end <= start) continue;
    all.push({ title: e.title.trim() || "Meeting", start, end, left: ((start - t0) / span) * 100, width: ((end - start) / span) * 100 });
  }
  all.sort((a, b) => a.start - b.start);
  return { bands: all.slice(0, MAX_BANDS), more: Math.max(0, all.length - MAX_BANDS) };
}

/// The day's total as it reads among the other stats: "2h 30m", nothing when there were none.
/// It is time on the calendar, not time attended.
export function meetingLine(ms: number): string | null {
  return ms > 0 ? fmtDur(ms) : null;
}
