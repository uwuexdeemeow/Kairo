import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  layouts: vi.fn(),
  setLayoutFocusLabel: vi.fn(),
  removeLayout: vi.fn(),
  openScriptsFolder: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { WindowLayout } from "./api";
import { loadLayouts, renderLayouts, wireScripts } from "./desk-settings";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

const writing: WindowLayout = { name: "Writing", windows: [{ app: "code.exe", title: "a" }, { app: "chrome.exe", title: "b" }], focusLabel: "essay" };
const one: WindowLayout = { name: "Calls", windows: [{ app: "zoom.exe", title: "Zoom" }], focusLabel: null };

beforeEach(() => {
  document.body.innerHTML = `<div id="layouts-list"></div><span id="layouts-status"></span><button id="scripts-open"></button><span id="scripts-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
});

describe("Window layouts in Kairo's settings", () => {
  it("shows each layout's name, its windows and its focus label", async () => {
    api.layouts.mockResolvedValue([writing, one]);
    await loadLayouts();
    const rows = [...document.querySelectorAll(".layout-row")];
    expect(rows.map((r) => r.querySelector(".muted")!.textContent)).toEqual(["2 windows", "1 window"]);
    expect([...document.querySelectorAll<HTMLInputElement>(".layout-label")].map((i) => i.value)).toEqual(["essay", ""]);
  });

  it("says when there are none", () => {
    renderLayouts([]);
    expect($("#layouts-list").textContent).toBe("No layouts yet.");
  });

  it("saves a label as it changes, and a blank one as none", async () => {
    renderLayouts([writing]);
    api.setLayoutFocusLabel.mockResolvedValue([{ ...writing, focusLabel: null }]);
    const label = $<HTMLInputElement>(".layout-label");
    label.value = "  ";
    label.dispatchEvent(new Event("change"));
    await settle();
    expect(api.setLayoutFocusLabel).toHaveBeenCalledWith("Writing", null);
    expect($<HTMLInputElement>(".layout-label").value).toBe("");
  });

  it("says why a change was refused", async () => {
    renderLayouts([writing]);
    api.setLayoutFocusLabel.mockRejectedValue("That layout is gone.");
    const label = $<HTMLInputElement>(".layout-label");
    label.value = "deep";
    label.dispatchEvent(new Event("change"));
    await settle();
    expect($("#layouts-status").textContent).toBe("That layout is gone.");
  });
});

describe("Scripts in Kairo's settings", () => {
  it("opens the folder and says where it is", async () => {
    wireScripts();
    api.openScriptsFolder.mockResolvedValue("C:\\Users\\me\\AppData\\Roaming\\kairo\\scripts");
    $("#scripts-open").click();
    await settle();
    expect($("#scripts-status").textContent).toBe("C:\\Users\\me\\AppData\\Roaming\\kairo\\scripts");
  });
});
