import { afterEach, describe, expect, it } from "vitest";
import { calendar } from "./calendar";

const box = (top: number, height: number) => ({ top, bottom: top + height, height, left: 0, right: 288, width: 288, x: 0, y: top, toJSON() {} }) as DOMRect;

/// A calendar under a field at `fieldTop` in a window `windowH` tall, its month `monthH` tall.
function openAt(fieldTop: number, windowH: number, monthH = 320) {
  Object.defineProperty(window, "innerHeight", { value: windowH, configurable: true });
  document.body.innerHTML = `<span id="anchor"><button id="trigger">Fri, Nov 6</button></span>`;
  const anchor = document.getElementById("anchor")!;
  anchor.getBoundingClientRect = () => box(fieldTop, 38);
  const cal = calendar({ anchor, trigger: document.getElementById("trigger") as HTMLButtonElement, label: "Due day", selected: () => "2026-11-06", min: () => null, max: () => null, onPick: () => {} });
  const pop = anchor.querySelector<HTMLElement>(".calendar")!;
  pop.getBoundingClientRect = () => box(fieldTop + 44, monthH);
  cal.toggle();
  return pop;
}

afterEach(() => (document.body.innerHTML = ""));

describe("Kairo's calendar", () => {
  it("opens below its field when the month fits there", () => {
    expect(openAt(100, 780).classList.contains("up")).toBe(false);
  });

  it("opens above its field when the window has no room below but has it above", () => {
    expect(openAt(420, 520).classList.contains("up")).toBe(true);
  });

  it("stays below, scrolled into view, with room on neither side", () => {
    const pop = openAt(231, 520);
    expect(pop.classList.contains("up")).toBe(false);
  });
});
