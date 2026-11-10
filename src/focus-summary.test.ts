import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FocusTarget, FocusWrapUp } from "./api";
import { renderFocusTarget, renderWrapUp } from "./focus-summary";

const MIN = 60_000;
let box: HTMLElement;
const text = (sel: string) => [...box.querySelectorAll(sel)].map((el) => el.textContent);

const wrapUp = (extra: Partial<FocusWrapUp> = {}): FocusWrapUp => ({
  start: 0,
  end: 25 * MIN,
  plannedMs: 25 * MIN,
  focusedMs: 23 * MIN,
  completed: false,
  label: null,
  nudges: 2,
  pulled: [
    { label: "youtube", ms: 3 * MIN },
    { label: "Discord", ms: MIN },
  ],
  versus: ["12m longer than your usual 38m", "Fewer nudges than usual"],
  ...extra,
});

beforeEach(() => {
  box = document.createElement("section");
  box.hidden = true;
  document.body.replaceChildren(box);
});

describe("the wrap-up", () => {
  it("says how long, how many nudges, what pulled you away and how it compares", () => {
    renderWrapUp(box, wrapUp(), async () => {});
    expect(box.hidden).toBe(false);
    expect(text("h2")).toEqual(["How your last session went"]);
    expect(text(".wrapup-facts span")).toEqual(["Focused 23m of 25m", "Ended early", "2 nudges"]);
    expect(text(".wrapup-pulled li")).toEqual(["youtube3m", "Discord1m"]);
    expect(text(".wrapup-versus li")).toEqual(["12m longer than your usual 38m", "Fewer nudges than usual"]);
  });

  it("names the session, and says so when nothing got in the way", () => {
    renderWrapUp(box, wrapUp({ label: "essay", completed: true, nudges: 0, pulled: [], versus: [] }), async () => {});
    expect(text("h2")).toEqual(["How “essay” went"]);
    expect(text(".wrapup-facts span")).toEqual(["Focused 23m of 25m", "Finished", "No nudges"]);
    expect(box.querySelector(".wrapup-pulled")).toBeNull();
    expect(box.querySelector(".wrapup-versus")).toBeNull();
    expect(box.textContent).toContain("Nothing distracting was in front while it ran.");
    renderWrapUp(box, wrapUp({ nudges: 1 }), async () => {});
    expect(text(".wrapup-facts span")[2]).toBe("1 nudge");
  });

  it("goes when dismissed, and when there's none", () => {
    const dismiss = vi.fn(async () => {});
    renderWrapUp(box, wrapUp(), dismiss);
    box.querySelector<HTMLButtonElement>("button")!.click();
    expect(dismiss).toHaveBeenCalledTimes(1);
    expect(box.hidden).toBe(true);
    renderWrapUp(box, wrapUp(), dismiss);
    expect(box.hidden).toBe(false);
    renderWrapUp(box, null, dismiss);
    expect(box.hidden).toBe(true);
    expect(box.children.length).toBe(0);
  });
});

describe("the daily focus target", () => {
  const target = (extra: Partial<FocusTarget> = {}): FocusTarget => ({ targetMs: 120 * MIN, usedMs: 70 * MIN, met: false, streak: 4, ...extra });

  it("shows today's focus against it, and its streak", () => {
    renderFocusTarget(box, target());
    expect(box.hidden).toBe(false);
    expect(text(".usage-value")).toEqual(["1h 10m of 2h 00m today"]);
    expect(text(".section-head .muted")).toEqual(["4-day streak"]);
    expect(box.querySelector<HTMLElement>(".line")!.style.width).toBe(`${(70 / 120) * 100}%`);
  });

  it("stops the bar at full, says it's reached, and leaves out a streak of none", () => {
    renderFocusTarget(box, target({ usedMs: 150 * MIN, met: true, streak: 0 }));
    expect(box.querySelector<HTMLElement>(".line")!.style.width).toBe("100%");
    expect(text(".usage-label")).toEqual(["Reached"]);
    expect(text(".section-head .muted")).toEqual([""]);
    renderFocusTarget(box, target({ usedMs: 0 }));
    expect(text(".usage-value")).toEqual(["0m of 2h 00m today"]);
  });

  it("is hidden with no target set", () => {
    renderFocusTarget(box, target());
    renderFocusTarget(box, null);
    expect(box.hidden).toBe(true);
    expect(box.children.length).toBe(0);
  });
});
