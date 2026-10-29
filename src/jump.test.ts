import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { jumpTo } from "./jump";
import { useDropdowns } from "./dropdown";

beforeEach(() => {
  vi.useFakeTimers();
  Element.prototype.scrollIntoView = vi.fn();
  document.body.innerHTML = `
    <section class="block" id="evening"><label><input type="checkbox" id="evening-enabled" /> Send</label></section>
    <section class="block" id="breaks"><select id="break-every"><option value="0">Never</option><option value="30">30</option></select></section>`;
});
afterEach(() => vi.useRealTimers());

describe("jumpTo", () => {
  it("focuses the control and marks its section until the time is up", () => {
    jumpTo("evening-enabled");
    expect(document.activeElement).toBe(document.getElementById("evening-enabled"));
    expect(document.getElementById("evening")!.classList.contains("jumped")).toBe(true);
    vi.advanceTimersByTime(1499);
    expect(document.getElementById("evening")!.classList.contains("jumped")).toBe(true);
    vi.advanceTimersByTime(1);
    expect(document.getElementById("evening")!.classList.contains("jumped")).toBe(false);
  });

  it("focuses a select's themed button, since the select is hidden", () => {
    useDropdowns();
    const select = document.getElementById("break-every")!;
    jumpTo("break-every");
    expect(document.activeElement).not.toBe(select);
    expect(document.activeElement?.tagName).toBe("BUTTON");
    expect(document.getElementById("breaks")!.contains(document.activeElement)).toBe(true);
  });

  it("does nothing for an id the page doesn't have", () => {
    expect(() => jumpTo("nope")).not.toThrow();
    expect(document.querySelector(".jumped")).toBeNull();
  });
});
