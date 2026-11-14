import { describe, expect, it } from "vitest";
import { busySpans, freeGaps, freeLine, placedSpan, suggest } from "./ledger-free";

const at = (h: number, m = 0) => new Date(2026, 10, 12, h, m).getTime();
const meeting = (from: number, to: number) => ({ startMs: from, endMs: to, title: "m" });
const hours = (gaps: { startMs: number; endMs: number }[]) => gaps.map((g) => [new Date(g.startMs).getHours() + new Date(g.startMs).getMinutes() / 60, new Date(g.endMs).getHours() + new Date(g.endMs).getMinutes() / 60]);

describe("free gaps", () => {
  it("is the whole working day when nothing is on", () => {
    expect(hours(freeGaps([], at(7)))).toEqual([[9, 18]]);
  });

  it("starts from the next five minutes once the day is under way", () => {
    expect(hours(freeGaps([], at(10, 32)))).toEqual([[10 + 35 / 60, 18]]);
  });

  it("leaves out meetings and gaps too short to plan into", () => {
    const busy = [meeting(at(10), at(11)), meeting(at(11, 10), at(12)), meeting(at(14), at(15, 30))];
    expect(hours(freeGaps(busy, at(8)))).toEqual([
      [9, 10],
      [12, 14],
      [15.5, 18],
    ]);
  });

  it("handles meetings that overlap, start before the day, or run past it", () => {
    const busy = [meeting(at(8), at(9, 30)), meeting(at(9), at(10)), meeting(at(17), at(19))];
    expect(hours(freeGaps(busy, at(8)))).toEqual([[10, 17]]);
  });

  it("has nothing once the day is over", () => {
    expect(freeGaps([], at(18))).toEqual([]);
    expect(freeGaps([], at(22))).toEqual([]);
  });

  it("skips what has already ended", () => {
    expect(hours(freeGaps([meeting(at(9), at(10))], at(13)))).toEqual([[13, 18]]);
  });
});

describe("what takes the time", () => {
  const task = (over: object) => ({ doneMs: null, remindMs: null, estimateMin: null, ...over });

  it("counts a task placed in today's hours, with its estimate", () => {
    expect(placedSpan(task({ remindMs: at(14), estimateMin: 90 }), at(8))).toEqual({ startMs: at(14), endMs: at(15, 30) });
  });

  it("ignores reminders that are past, outside the day, done, or have no estimate", () => {
    expect(placedSpan(task({ remindMs: at(7), estimateMin: 30 }), at(8))).toBeNull();
    expect(placedSpan(task({ remindMs: at(19), estimateMin: 30 }), at(8))).toBeNull();
    expect(placedSpan(task({ remindMs: at(14), estimateMin: 30, doneMs: 1 }), at(8))).toBeNull();
    expect(placedSpan(task({ remindMs: at(14) }), at(8))).toBeNull();
  });

  it("ignores all-day events", () => {
    const allDay = meeting(new Date(2026, 10, 12).getTime(), new Date(2026, 10, 13).getTime());
    expect(busySpans([allDay, meeting(at(10), at(11))], [], at(8))).toEqual([{ startMs: at(10), endMs: at(11) }]);
  });
});

describe("where tasks would go", () => {
  const gaps = [
    { startMs: at(9), endMs: at(10) },
    { startMs: at(13), endMs: at(16) },
  ];

  it("puts each in the first gap it fits, taking the time from it", () => {
    const places = suggest(
      [
        { id: "a", estimateMin: 90 },
        { id: "b", estimateMin: 30 },
        { id: "c", estimateMin: 60 },
        { id: "d", estimateMin: 45 },
      ],
      gaps,
    );
    // a doesn't fit before ten, so goes at one; b fits the morning; c follows a at half past two;
    // d (45 minutes) fits neither what is left of the morning nor of the afternoon.
    expect(Object.fromEntries(places)).toEqual({ a: at(13), b: at(9), c: at(14, 30) });
  });

  it("gives no place to a task with no estimate, and leaves the gaps as they were", () => {
    const before = JSON.stringify(gaps);
    expect(suggest([{ id: "x", estimateMin: null }], gaps).size).toBe(0);
    expect(JSON.stringify(gaps)).toBe(before);
  });

  it("starts the next task on a five-minute mark", () => {
    const places = suggest(
      [
        { id: "a", estimateMin: 22 },
        { id: "b", estimateMin: 10 },
      ],
      [{ startMs: at(9), endMs: at(12) }],
    );
    expect(places.get("b")).toBe(at(9, 25));
  });
});

describe("the free line", () => {
  it("says how much is free and where the next gap is", () => {
    const line = freeLine([
      { startMs: at(10), endMs: at(11, 30) },
      { startMs: at(14), endMs: at(16) },
    ]);
    expect(line).toMatch(/^3h 30m free today, next /);
    expect(line).toContain("–");
  });

  it("says nothing when there is no time", () => {
    expect(freeLine([])).toBe("");
  });
});
