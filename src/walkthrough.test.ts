import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ walkthroughDue: vi.fn(), finishWalkthrough: vi.fn(), paletteSettings: vi.fn(), openPalette: vi.fn() }));
vi.mock("./api", () => ({ api }));

import { setUpWalkthrough } from "./walkthrough";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));
const title = () => $("#walkthrough-title").textContent;

beforeEach(() => {
  document.body.innerHTML = `<div id="app"><button id="walkthrough-again"></button></div>
    <div id="walkthrough" hidden data-app="kairo"><div id="walkthrough-dots"></div><div id="walkthrough-step"></div>
    <button id="walkthrough-skip"></button><button id="walkthrough-settings" hidden></button><button id="walkthrough-back" hidden></button><button id="walkthrough-next"></button></div>
    <div id="tooltip"></div>`;
  for (const fn of Object.values(api)) fn.mockReset();
  api.finishWalkthrough.mockResolvedValue(undefined);
  api.paletteSettings.mockResolvedValue({ shortcut: "Ctrl+Space" });
});

// Each test leaves its walkthrough open; Esc closes them so none keeps listening for keys.
afterEach(async () => {
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  await settle();
});

async function open(due = true) {
  api.walkthroughDue.mockResolvedValue(due);
  const openSettings = vi.fn();
  setUpWalkthrough(openSettings);
  await settle();
  return openSettings;
}

describe("The first-run walkthrough", () => {
  it("shows on a first run, on the first of four screens, and not otherwise", async () => {
    await open(false);
    expect($("#walkthrough").hidden).toBe(true);
    await open(true);
    expect($("#walkthrough").hidden).toBe(false);
    expect(title()).toBe("Welcome to Kairo");
    expect($("#walkthrough-dots").getAttribute("aria-label")).toBe("Step 1 of 4");
    expect($("#walkthrough-back").hidden).toBe(true);
    expect($("#walkthrough-next").textContent).toBe("Next");
    // The page behind can't be reached while it's open.
    expect($("#app").inert).toBe(true);
    expect(!!$("#tooltip").inert).toBe(false);
  });

  it("walks through the screens and ends on Start, which finishes it", async () => {
    await open();
    const seen = [title()];
    for (let i = 0; i < 3; i++) {
      $("#walkthrough-next").click();
      await settle();
      seen.push(title());
    }
    expect(seen).toEqual(["Welcome to Kairo", "The command palette", "Chrono keeps your time", "Ledger and focus"]);
    expect($("#walkthrough-next").textContent).toBe("Start");
    expect($("#walkthrough-skip").hidden).toBe(true);
    expect($("#walkthrough-settings").hidden).toBe(false);
    $("#walkthrough-next").click();
    await settle();
    expect(api.finishWalkthrough).toHaveBeenCalledTimes(1);
    expect($("#walkthrough").hidden).toBe(true);
    expect($("#app").inert).toBe(false);
  });

  it("shows the palette's keys as set, and opens it from Try it", async () => {
    api.paletteSettings.mockResolvedValue({ shortcut: "Ctrl+Alt+K" });
    await open();
    $("#walkthrough-next").click();
    await settle();
    expect([...document.querySelectorAll("#walkthrough-step kbd")].map((k) => k.textContent).slice(0, 3)).toEqual(["Ctrl", "Alt", "K"]);
    api.openPalette.mockResolvedValue(undefined);
    $("#walkthrough-try").click();
    await settle();
    expect(api.openPalette).toHaveBeenCalled();
    api.openPalette.mockRejectedValue("no");
    $("#walkthrough-try").click();
    await settle();
    expect($("#walkthrough-note").textContent).toBe("Couldn't open the palette.");
  });

  it("goes back, and the arrow keys move between screens", async () => {
    await open();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    await settle();
    expect(title()).toBe("The command palette");
    expect($("#walkthrough-back").hidden).toBe(false);
    $("#walkthrough-back").click();
    await settle();
    expect(title()).toBe("Welcome to Kairo");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    await settle();
    expect(title()).toBe("Welcome to Kairo");
  });

  it("skips with the button or Esc, and counts as seen", async () => {
    await open();
    $("#walkthrough-skip").click();
    await settle();
    expect($("#walkthrough").hidden).toBe(true);
    expect(api.finishWalkthrough).toHaveBeenCalledTimes(1);
    $("#walkthrough-again").click();
    await settle();
    expect($("#walkthrough").hidden).toBe(false);
    expect(title()).toBe("Welcome to Kairo");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await settle();
    expect($("#walkthrough").hidden).toBe(true);
    expect(api.finishWalkthrough).toHaveBeenCalledTimes(2);
  });

  it("the last screen's link opens settings after it finishes, and a failed save still closes it", async () => {
    api.finishWalkthrough.mockRejectedValue("no");
    const openSettings = await open();
    for (let i = 0; i < 3; i++) {
      $("#walkthrough-next").click();
      await settle();
    }
    $("#walkthrough-settings").click();
    await settle();
    expect($("#walkthrough").hidden).toBe(true);
    expect(openSettings).toHaveBeenCalled();
  });
});
