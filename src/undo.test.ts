import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SHOWN_FOR_MS, mountUndo } from "./undo";

let line: HTMLElement;
let fallback: HTMLButtonElement;

beforeEach(() => {
  document.body.innerHTML = `<p id="line" hidden></p><button id="fallback">After</button><textarea id="typing"></textarea>`;
  line = document.getElementById("line")!;
  fallback = document.getElementById("fallback") as HTMLButtonElement;
});

afterEach(() => vi.useRealTimers());

const offer = (u: ReturnType<typeof mountUndo>, what = "Essay") => {
  const p = { what, finish: vi.fn(), undo: vi.fn(), focusAfter: () => fallback };
  u.offer(p);
  return p;
};

const ctrlZ = (target: EventTarget = document) => target.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));

describe("deletes that can be taken back", () => {
  it("says what went, leaving the keyboard where it was", () => {
    const u = mountUndo(line);
    fallback.focus();
    offer(u);
    expect(line.hidden).toBe(false);
    expect(line.textContent).toBe("Deleted “Essay” · Undo");
    expect(document.activeElement).toBe(fallback);
  });

  it("keeps several, and takes them back latest first", () => {
    const u = mountUndo(line);
    const first = offer(u);
    const second = offer(u, "Milk");
    expect(first.finish).not.toHaveBeenCalled();
    expect(line.textContent).toBe("Deleted “Milk” and 1 more · Undo");
    ctrlZ();
    expect(second.undo).toHaveBeenCalledTimes(1);
    expect(first.undo).not.toHaveBeenCalled();
    expect(line.textContent).toBe("Deleted “Essay” · Undo");
    line.querySelector("button")!.click();
    expect(first.undo).toHaveBeenCalledTimes(1);
    expect(line.hidden).toBe(true);
  });

  it("fades after a few seconds, Ctrl+Z still working", () => {
    vi.useFakeTimers();
    const u = mountUndo(line);
    const p = offer(u);
    vi.advanceTimersByTime(SHOWN_FOR_MS);
    expect(line.hidden).toBe(true);
    expect(p.finish).not.toHaveBeenCalled();
    ctrlZ();
    expect(p.undo).toHaveBeenCalledTimes(1);
  });

  it("doesn't fade from under the keyboard", () => {
    vi.useFakeTimers();
    const u = mountUndo(line);
    offer(u);
    line.querySelector<HTMLButtonElement>("button")!.focus();
    vi.advanceTimersByTime(SHOWN_FOR_MS * 2);
    expect(line.hidden).toBe(false);
  });

  it("leaves Ctrl+Z to a field being typed in", () => {
    const u = mountUndo(line);
    const p = offer(u);
    ctrlZ(document.getElementById("typing")!);
    expect(p.undo).not.toHaveBeenCalled();
  });

  it("made final, finishes them all and moves the keyboard off Undo", () => {
    const u = mountUndo(line);
    const a = offer(u);
    const b = offer(u, "Milk");
    line.querySelector<HTMLButtonElement>("button")!.focus();
    u.finish();
    expect(a.finish).toHaveBeenCalledTimes(1);
    expect(b.finish).toHaveBeenCalledTimes(1);
    expect(line.hidden).toBe(true);
    expect(document.activeElement).toBe(fallback);
    // Nothing waiting: finishing again does nothing.
    u.finish();
    expect(a.finish).toHaveBeenCalledTimes(1);
  });
});
