// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flash } from "./ui";

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
});
