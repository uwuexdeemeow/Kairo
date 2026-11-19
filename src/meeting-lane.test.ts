import { describe, expect, it } from "vitest";
import type { CalendarEvent } from "./api";
import { MAX_BANDS, meetingBands, meetingLine } from "./meeting-lane";

const H = 3_600_000;
const day0 = 1_000 * 24 * H;
const ev = (fromH: number, toH: number, title = "Review"): CalendarEvent => ({ startMs: day0 + fromH * H, endMs: day0 + toH * H, title });

describe("meeting bands", () => {
  it("places each meeting on the shown hours as a share of them", () => {
    const { bands, more } = meetingBands([ev(10, 11), ev(14, 15.5, "")], day0 + 9 * H, 9 * H, day0, day0 + 24 * H);
    expect(more).toBe(0);
    expect(bands.map((b) => [b.left, b.width, b.title])).toEqual([
      [(1 / 9) * 100, (1 / 9) * 100, "Review"],
      [(5 / 9) * 100, (1.5 / 9) * 100, "Meeting"],
    ]);
  });

  it("clips a meeting that spans midnight to the day, and leaves out one outside the shown hours", () => {
    const { bands } = meetingBands([ev(-1, 1, "Late"), ev(23, 25, "Later"), ev(3, 4, "Early")], day0 + 0 * H, 24 * H, day0, day0 + 24 * H);
    expect(bands.map((b) => [b.title, b.start - day0, b.end - day0])).toEqual([
      ["Late", 0, H],
      ["Early", 3 * H, 4 * H],
      ["Later", 23 * H, 24 * H],
    ]);
    const narrow = meetingBands([ev(3, 4)], day0 + 8 * H, 4 * H, day0, day0 + 24 * H);
    expect(narrow.bands).toEqual([]);
  });

  it("draws at most 40 and says how many more there were", () => {
    const many = Array.from({ length: 46 }, (_, i) => ev(0 + i * 0.5, 0.25 + i * 0.5));
    const { bands, more } = meetingBands(many, day0, 24 * H, day0, day0 + 24 * H);
    expect(bands).toHaveLength(MAX_BANDS);
    expect(more).toBe(6);
  });
});

describe("the day's meeting line", () => {
  it("says the calendar's time, and nothing for none", () => {
    expect(meetingLine(0)).toBeNull();
    expect(meetingLine(2.5 * H)).toBe("2h 30m");
  });
});
