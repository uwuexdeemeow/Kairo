// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { countOf, flash, foldState } from "./ui";

describe("flash", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("says a confirmation, then clears it", () => {
    const el = document.createElement("span");
    flash(el, "Saved");
    expect(el.textContent).toBe("Saved");
    vi.advanceTimersByTime(4000);
    expect(el.textContent).toBe("");
  });

  it("leaves a newer message alone", () => {
    const el = document.createElement("span");
    flash(el, "Saved");
    el.textContent = "Couldn't save";
    vi.advanceTimersByTime(4000);
    expect(el.textContent).toBe("Couldn't save");
  });

  it("keeps a second confirmation for its own full time", () => {
    const el = document.createElement("span");
    flash(el, "Saved");
    vi.advanceTimersByTime(3000);
    flash(el, "Saved");
    vi.advanceTimersByTime(3000);
    expect(el.textContent).toBe("Saved");
    vi.advanceTimersByTime(1000);
    expect(el.textContent).toBe("");
  });
});

describe("a fold's state", () => {
  it("is said in its header, and counts read plainly", () => {
    document.body.innerHTML = '<span id="state-schedule"></span>';
    foldState("schedule", countOf(2, "schedule"));
    expect(document.getElementById("state-schedule")!.textContent).toBe("2 schedules");
    expect(countOf(0, "list")).toBe("None");
    expect(countOf(1, "mode")).toBe("1 mode");
    // A fold the page doesn't have is no error.
    expect(() => foldState("nothing", "x")).not.toThrow();
  });
});
