import { beforeEach, describe, expect, it, vi } from "vitest";
import { mountRing, type RingActions } from "./ring-view";
import { CENTRE, INNER, OUTER, STEP } from "./ring-model";

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

const items = ["chrono", "ledger", "palette", "dashboard", "task", "note", "focus"] as const;
const names = ["Chrono", "Ledger", "Palette", "Dashboard", "New task", "New note", "Focus 25"];
const open = (lit = 1) => mountRing(root, { items: [...items], lit, names, lines, focusRunning: false }, act, () => clock);
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

  it("stays turned after a drag, then opens with a click or Enter", () => {
    open(0);
    root.getBoundingClientRect = () => ({ left: 0, top: 0, right: 300, bottom: 210, width: 300, height: 210, x: 0, y: 0, toJSON() {} }) as DOMRect;
    // On the ring at a given angle (degrees clockwise from the top), in window pixels.
    const at = (deg: number) => {
      const r = (OUTER + INNER) / 2;
      const a = (deg * Math.PI) / 180;
      return { clientX: CENTRE.x + r * Math.sin(a), clientY: CENTRE.y - r * Math.cos(a) };
    };
    const pointer = (type: string, deg: number) => root.dispatchEvent(new MouseEvent(type, { ...at(deg), bubbles: true }));
    pointer("pointerdown", STEP);
    pointer("pointermove", 0);
    pointer("pointerup", 0);
    expect(litName()).toBe("ledger");
    expect(act.pick).not.toHaveBeenCalled();
    // A held modifier let go after the drag doesn't open it either.
    clock = 900;
    key("keyup", "Alt");
    expect(act.pick).not.toHaveBeenCalled();
    // A click on the lit one opens it.
    pointer("pointerdown", 0);
    pointer("pointerup", 0);
    expect(act.pick).toHaveBeenCalledWith(1);
  });

  it("opens the lit one with Enter after a drag", () => {
    open(0);
    root.getBoundingClientRect = () => ({ left: 0, top: 0, right: 300, bottom: 210, width: 300, height: 210, x: 0, y: 0, toJSON() {} }) as DOMRect;
    const y = (deg: number) => CENTRE.y - ((OUTER + INNER) / 2) * Math.cos((deg * Math.PI) / 180);
    const x = (deg: number) => CENTRE.x + ((OUTER + INNER) / 2) * Math.sin((deg * Math.PI) / 180);
    root.dispatchEvent(new MouseEvent("pointerdown", { clientX: x(STEP * 2), clientY: y(STEP * 2), bubbles: true }));
    root.dispatchEvent(new MouseEvent("pointermove", { clientX: x(0), clientY: y(0), bubbles: true }));
    root.dispatchEvent(new MouseEvent("pointerup", { clientX: x(0), clientY: y(0), bubbles: true }));
    expect(litName()).toBe("palette");
    expect(act.pick).not.toHaveBeenCalled();
    key("keydown", "Enter");
    expect(act.pick).toHaveBeenCalledWith(2);
  });

  it("lets go of a drag that's cut off, so moving the mouse after doesn't turn it", () => {
    open(0);
    root.getBoundingClientRect = () => ({ left: 0, top: 0, right: 300, bottom: 210, width: 300, height: 210, x: 0, y: 0, toJSON() {} }) as DOMRect;
    const at = (deg: number) => {
      const a = (deg * Math.PI) / 180;
      const r = (OUTER + INNER) / 2;
      return { clientX: CENTRE.x + r * Math.sin(a), clientY: CENTRE.y - r * Math.cos(a), bubbles: true };
    };
    root.dispatchEvent(new MouseEvent("pointerdown", at(STEP)));
    root.dispatchEvent(new MouseEvent("pointermove", at(0)));
    expect(litName()).toBe("ledger");
    root.dispatchEvent(new MouseEvent("pointercancel", at(0)));
    root.dispatchEvent(new MouseEvent("pointermove", at(-STEP)));
    expect(litName()).toBe("ledger");
    expect(root.classList.contains("dragging")).toBe(false);
    expect(act.pick).not.toHaveBeenCalled();
  });

  it("says when a session is running", () => {
    mountRing(root, { items: [...items], lit: 6, names, lines, focusRunning: true }, act, () => clock);
    expect(root.querySelector(".ring-seg.lit")!.classList.contains("dim")).toBe(true);
  });

  it("draws an arranged ring, each item once, and picks by place on it", () => {
    mountRing(root, { items: ["notes", "week", "pause"], lit: 0, names: ["All notes", "This week", "Pause tracking"], lines: ["Pop-out", "Chrono", "Tracking"], focusRunning: false }, act, () => clock);
    expect([...root.querySelectorAll(".ring-seg")].map((s) => s.getAttribute("data-segment")).sort()).toEqual(["notes", "pause", "week"]);
    expect(root.querySelector(".ring-name")!.textContent).toBe("All notes");
    // Past the items on it, a number does nothing.
    key("keydown", "5");
    expect(act.pick).not.toHaveBeenCalled();
    key("keydown", "3");
    expect(act.pick).toHaveBeenCalledWith(2);
  });

  it("draws Pause as Resume while tracking is paused", () => {
    mountRing(root, { items: ["pause", "chrono", "ledger"], lit: 0, names: ["Resume tracking", "Chrono", "Ledger"], lines: ["Paused", "", ""], focusRunning: false, paused: true }, act, () => clock);
    expect(root.querySelector(".ring-name")!.textContent).toBe("Resume tracking");
    expect(root.querySelector(".ring-lit svg")!.innerHTML).toContain("M32 22l46 28-46 28z");
  });
});
