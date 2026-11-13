import { describe, expect, it } from "vitest";
import { costText, estimateName, loadLine, paceLine, paceRatio, plannedMinutes } from "./ledger-cost";

const MIN = 60_000;
const finished = (estimateMin: number | null, focusMin: number) => ({ doneMs: 1, estimateMin, focusedMs: focusMin * MIN });

describe("estimates and time spent", () => {
  it("names an estimate in minutes, whole hours or both", () => {
    expect([15, 45, 60, 90, 120, 480].map(estimateName)).toEqual(["15m", "45m", "1h", "1h 30m", "2h", "8h"]);
  });

  it("says a task's cost: the estimate, the time spent, or the time spent of the estimate", () => {
    expect(costText({ estimateMin: 120, focusedMs: 0 })).toBe("~2h");
    expect(costText({ estimateMin: null, focusedMs: 35 * MIN })).toBe("35m");
    expect(costText({ estimateMin: 120, focusedMs: 95 * MIN })).toBe("1h 35m of ~2h");
    expect(costText({ estimateMin: null, focusedMs: 0 })).toBe("");
  });
});

describe("a day's load", () => {
  const task = (group: "today" | "overdue" | "upcoming" | "none", estimateMin: number | null, doneMs: number | null = null) => ({ group, estimateMin, doneMs });

  it("adds the estimates of what is due today or overdue, and still to do", () => {
    const tasks = [task("today", 60), task("overdue", 90), task("upcoming", 120), task("today", null), task("today", 30, 5)];
    expect(plannedMinutes(tasks)).toBe(150);
    expect(plannedMinutes([task("today", null)])).toBe(0);
  });

  it("says nothing with no estimates, the load alone before a usual day is known, and a warning past it", () => {
    expect(loadLine(0, 3 * 60 * MIN)).toBe("");
    expect(loadLine(240, null)).toBe("4h planned today");
    expect(loadLine(120, 3 * 60 * MIN)).toBe("2h planned today");
    expect(loadLine(180, 3 * 60 * MIN)).toBe("3h planned today");
    expect(loadLine(240, 3 * 60 * MIN)).toBe("4h planned today, more than the 3h you usually focus");
  });
});

describe("how long tasks take against their estimates", () => {
  it("says nothing before five finished tasks have an estimate and real focus time", () => {
    const four = [finished(60, 120), finished(60, 120), finished(60, 120), finished(60, 120)];
    expect(paceRatio(four)).toBeNull();
    // Not counted: no estimate, almost no focus, or not finished.
    expect(paceRatio([...four, finished(null, 120), finished(60, 5), { doneMs: null, estimateMin: 60, focusedMs: 120 * MIN }])).toBeNull();
  });

  it("is the middle ratio of the finished ones, and speaks only when it is far from as estimated", () => {
    const twice = Array.from({ length: 5 }, () => finished(60, 120));
    expect(paceRatio(twice)).toBe(2);
    expect(paceLine(2)).toBe("You usually take about twice what you estimate.");
    expect(paceLine(1.5)).toBe("You usually take about half as long again as you estimate.");
    expect(paceLine(1.1)).toBe("");
    expect(paceLine(0.9)).toBe("");
    expect(paceLine(0.5)).toBe("You usually take about half what you estimate.");
    expect(paceLine(null)).toBe("");
    // One long task among quick ones doesn't make a habit: the middle one decides.
    expect(paceRatio([finished(60, 60), finished(60, 60), finished(60, 60), finished(60, 60), finished(60, 600)])).toBe(1);
    // An even count takes the middle two.
    expect(paceRatio([finished(60, 60), finished(60, 60), finished(60, 60), finished(60, 120), finished(60, 120), finished(60, 120)])).toBe(1.5);
  });
});
