import { beforeEach, describe, expect, it, vi } from "vitest";
import { mountUndo } from "./undo";

let line: HTMLElement;
let fallback: HTMLButtonElement;

beforeEach(() => {
  document.body.innerHTML = `<p id="line" hidden></p><button id="fallback">After</button><textarea id="typing"></textarea>`;
  line = document.getElementById("line")!;
  fallback = document.getElementById("fallback") as HTMLButtonElement;
});

const offer = (u: ReturnType<typeof mountUndo>, what = "Essay") => {
  const p = { what, finish: vi.fn(), undo: vi.fn(), focusAfter: () => fallback };
  u.offer(p);
  return p;
};

describe("a delete that can be taken back", () => {
  it("says what went, with Undo in the keyboard, and stays until the next one", () => {
    const u = mountUndo(line);
    const first = offer(u);
    expect(line.hidden).toBe(false);
    expect(line.textContent).toBe("Deleted “Essay” · Undo");
    expect(document.activeElement).toBe(line.querySelector("button"));
    // A second delete makes the first final.
    const second = offer(u, "Milk");
    expect(first.finish).toHaveBeenCalledTimes(1);
    expect(second.finish).not.toHaveBeenCalled();
    expect(line.textContent).toBe("Deleted “Milk” · Undo");
  });

  it("takes it back with Undo or Ctrl+Z", () => {
    const u = mountUndo(line);
    const p = offer(u);
    line.querySelector("button")!.click();
    expect(p.undo).toHaveBeenCalledTimes(1);
    expect(line.hidden).toBe(true);
    const q = offer(u, "Milk");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    expect(q.undo).toHaveBeenCalledTimes(1);
    expect(q.finish).not.toHaveBeenCalled();
  });

  it("leaves Ctrl+Z to a field being typed in", () => {
    const u = mountUndo(line);
    const p = offer(u);
    document.getElementById("typing")!.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    expect(p.undo).not.toHaveBeenCalled();
  });

  it("made final, moves the keyboard somewhere sensible rather than losing it", () => {
    const u = mountUndo(line);
    const p = offer(u);
    u.finish();
    expect(p.finish).toHaveBeenCalledTimes(1);
    expect(line.hidden).toBe(true);
    expect(document.activeElement).toBe(fallback);
    // Nothing waiting: finishing again does nothing.
    u.finish();
    expect(p.finish).toHaveBeenCalledTimes(1);
  });
});
