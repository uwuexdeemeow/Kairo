import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  layouts: vi.fn(),
  setLayoutFocusLabel: vi.fn(),
  removeLayout: vi.fn(),
  saveLayout: vi.fn(),
  scripts: vi.fn(),
  openScriptsFolder: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { WindowLayout } from "./api";
import { loadLayouts, renderLayouts, renderScripts, wireLayoutSave, wireScripts } from "./desk-settings";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

const writing: WindowLayout = { name: "Writing", windows: [{ app: "code.exe", title: "a" }, { app: "chrome.exe", title: "b" }], focusLabel: "essay" };
const one: WindowLayout = { name: "Calls", windows: [{ app: "zoom.exe", title: "Zoom" }], focusLabel: null };

beforeEach(() => {
  document.body.innerHTML = `<div id="layouts-list"></div><input id="layout-new-name" /><button id="layout-save"></button><span id="layouts-status"></span><div id="scripts-list"></div><button id="scripts-open"></button><button id="scripts-refresh"></button><span id="scripts-status"></span>`;
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

  it("saves the open windows under the name typed, then clears the box", async () => {
    wireLayoutSave();
    api.saveLayout.mockResolvedValue([writing]);
    $<HTMLInputElement>("#layout-new-name").value = "Writing";
    $("#layout-save").click();
    await settle();
    expect(api.saveLayout).toHaveBeenCalledWith("Writing");
    expect($(".layout-row")).not.toBeNull();
    expect($<HTMLInputElement>("#layout-new-name").value).toBe("");
    expect($("#layouts-status").textContent).toBe("Saved.");
  });

  it("says why a layout wasn't saved and keeps the name", async () => {
    wireLayoutSave();
    api.saveLayout.mockRejectedValue("There are no windows to save.");
    $<HTMLInputElement>("#layout-new-name").value = "Empty";
    $("#layout-save").click();
    await settle();
    expect($("#layouts-status").textContent).toBe("There are no windows to save.");
    expect($<HTMLInputElement>("#layout-new-name").value).toBe("Empty");
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

  it("lists what it found and the files it left out", () => {
    renderScripts({
      scripts: [{ title: "Deploy site", file: "deploy.sh", description: null, argument: "Branch", output: "copy" }],
      skipped: ["notes.txt"],
    });
    expect($("#scripts-list").textContent).toContain("Deploy site · deploy.sh, asks for branch, copies the result");
    expect($("#scripts-list").textContent).toContain("Not listed, with no @kairo.title line: notes.txt.");
  });

  it("says when there are none, and looks again on request", async () => {
    renderScripts({ scripts: [], skipped: [] });
    expect($("#scripts-list").textContent).toBe("No scripts yet.");
    wireScripts();
    api.scripts.mockResolvedValue({ scripts: [{ title: "Tidy", file: "tidy.js", description: null, argument: null, output: "silent" }], skipped: [] });
    $("#scripts-refresh").click();
    await settle();
    expect($("#scripts-list").textContent).toContain("Tidy · tidy.js, says nothing");
  });
});
