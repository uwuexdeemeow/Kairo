import { beforeEach, describe, expect, it, vi } from "vitest";
import { mountRing, type RingActions } from "./ring-view";

let root: HTMLElement;
let act: { [K in keyof RingActions]: ReturnType<typeof vi.fn> };
let clock: number;
const lines = ["2h 05m today", "3 due today", "Ctrl + Space", "Kairo", "In Ledger", "In Ledger", "25 minutes"];

beforeEach(() => {
  document.body.innerHTML = `<main id="root"></main>`;
  root = document.getElementById("root")!;
  act = { pick: vi.fn(), close: vi.fn() };
  clock = 0;
});

const open = (lit = 1) => mountRing(root, { lit, lines, focusRunning: false }, act, () => clock);
const key = (type: "keydown" | "keyup", k: string, extra: KeyboardEventInit = {}) => document.dispatchEvent(new KeyboardEvent(type, { key: k, ...extra }));
const litName = () => root.querySelector(".ring-seg.lit")!.getAttribute("data-segment");

describe("the web ring", () => {
  it("draws the lit segment with its name and line, and its neighbours", () => {
    open(1);
    expect(root.querySelectorAll(".ring-seg").length).toBe(5);
    expect(litName()).toBe("ledger");
    expect(root.querySelector(".ring-name")!.textContent).toBe("Ledger");
    expect(root.querySelector(".ring-line")!.textContent).toBe("3 due today");
    expect(root.querySelector('.ring-seg[data-segment="chrono"]')).not.toBeNull();
    expect(root.querySelector('.ring-seg[data-segment="palette"]')).not.toBeNull();
  });

  it("turns with the arrows and the wheel, and opens with Enter", () => {
    open(1);
    key("keydown", "ArrowRight");
    expect(litName()).toBe("palette");
    key("keydown", "ArrowLeft");
    key("keydown", "ArrowLeft");
    expect(litName()).toBe("chrono");
    root.querySelector("svg")!.dispatchEvent(new WheelEvent("wheel", { deltaY: 40, bubbles: true }));
    expect(litName()).toBe("ledger");
    key("keydown", "Enter");
    expect(act.pick).toHaveBeenCalledWith(1);
  });

  it("opens a segment by its number, and closes on Esc", () => {
    open(0);
    key("keydown", "6");
    expect(act.pick).toHaveBeenCalledWith(5);
    key("keydown", "Escape");
    expect(act.close).toHaveBeenCalled();
  });

  it("turns back with Shift and the key, and turns on when told to", () => {
    const view = open(1);
    key("keydown", "Q", { altKey: true, shiftKey: true });
    expect(litName()).toBe("chrono");
    view.turn(2);
    expect(litName()).toBe("palette");
  });

  it("opens the lit one when the modifier is let go after a hold, not after a tap", () => {
    open(1);
    clock = 150;
    key("keyup", "Alt");
    expect(act.pick).not.toHaveBeenCalled();
    key("keydown", "ArrowRight");
    clock = 900;
    key("keyup", "Alt");
    // After a tap the ring stays for keys; a later Alt release isn't a pick either.
    expect(act.pick).not.toHaveBeenCalled();
  });

  it("picks on release after holding", () => {
    open(2);
    clock = 400;
    key("keyup", "Alt");
    expect(act.pick).toHaveBeenCalledWith(2);
  });

  it("says when a session is running", () => {
    mountRing(root, { lit: 6, lines, focusRunning: true }, act, () => clock);
    expect(root.querySelector(".ring-seg.lit")!.classList.contains("dim")).toBe(true);
  });
});
