import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ paletteSettings: vi.fn(), openPalette: vi.fn(), openLedger: vi.fn() }));
vi.mock("./api", () => ({ api }));

import { setUpWelcome, showWelcomeKeys } from "./welcome";

const $ = (sel: string) => document.querySelector<HTMLElement>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  document.body.innerHTML = `<span id="welcome-keys"></span><button id="welcome-ledger"></button><button id="welcome-palette"></button><span id="welcome-palette-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
});

describe("The welcome's palette lines", () => {
  it("shows the palette's keys as they are set", async () => {
    api.paletteSettings.mockResolvedValue({ shortcut: "Ctrl+Alt+K" });
    await showWelcomeKeys();
    expect([...document.querySelectorAll("#welcome-keys kbd")].map((k) => k.textContent)).toEqual(["Ctrl", "Alt", "K"]);
  });

  it("shows the usual keys when the settings can't be read", async () => {
    api.paletteSettings.mockRejectedValue("nope");
    await showWelcomeKeys();
    expect($("#welcome-keys").textContent).toMatch(/Space/);
  });

  it("opens the palette and Ledger, and says when one won't open", async () => {
    setUpWelcome();
    api.openPalette.mockResolvedValue(undefined);
    $("#welcome-palette").click();
    await settle();
    expect(api.openPalette).toHaveBeenCalled();
    expect($("#welcome-palette-status").textContent).toBe("");
    api.openLedger.mockRejectedValue("no");
    $("#welcome-ledger").click();
    await settle();
    expect($("#welcome-palette-status").textContent).toBe("Couldn't open Ledger.");
  });
});
