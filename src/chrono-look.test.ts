import { describe, expect, it } from "vitest";
import { dayParts, watchSvg } from "./chrono-look";
import { RING } from "./logos";

describe("Chrono's day dial", () => {
  it("shares the day's active time three ways", () => {
    expect(dayParts({ activeMs: 100, productiveMs: 50, distractingMs: 20 })).toEqual({ productive: 50, neutral: 30, distracting: 20 });
    expect(dayParts({ activeMs: 0, productiveMs: 0, distractingMs: 0 })).toEqual({ productive: 0, neutral: 0, distracting: 0 });
  });
});

describe("the focus watch", () => {
  const dash = (svg: string) => Number(/class="watch-left"[^>]*stroke-dasharray="([\d.]+) /.exec(svg)![1]);

  it("unwinds as the session runs", () => {
    expect(dash(watchSvg(25 * 60_000, 25 * 60_000, false))).toBeCloseTo(RING, 2);
    expect(dash(watchSvg(12.5 * 60_000, 25 * 60_000, false))).toBeCloseTo(RING / 2, 2);
    expect(dash(watchSvg(0, 25 * 60_000, false))).toBeCloseTo(0, 2);
  });

  it("keeps within the ring whatever it's given", () => {
    expect(dash(watchSvg(40 * 60_000, 25 * 60_000, false))).toBeCloseTo(RING, 2);
    expect(dash(watchSvg(-5, 25 * 60_000, false))).toBeCloseTo(0, 2);
    expect(dash(watchSvg(5, 0, false))).toBeCloseTo(RING, 2);
  });

  it("says when it's paused", () => {
    expect(watchSvg(1, 2, true)).toContain('class="watch paused"');
    expect(watchSvg(1, 2, false)).toContain('class="watch"');
  });
});
