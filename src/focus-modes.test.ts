import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  saveFocusMode: vi.fn(),
  removeFocusMode: vi.fn(),
  installedApps: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { FocusMode } from "./api";
import { forgetInstalledApps, renderModes, setUpModeForm } from "./focus-modes";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

const word = { name: "Word", path: "shell:word" };
const chrome = { name: "Chrome", path: "shell:chrome" };
const notion = { name: "Notion", path: "shell:notion" };
const study: FocusMode = { name: "Study", minutes: 50, list: "Social", apps: [word] };

beforeEach(() => {
  document.body.innerHTML = `<div id="modes"></div><input id="mode-name" /><button id="mode-add"></button><span id="mode-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
  api.installedApps.mockResolvedValue([word, chrome, notion]);
  forgetInstalledApps();
});

describe("Modes on the Focus page", () => {
  it("shows each mode's name, length, block list and apps", () => {
    renderModes([study, { name: "Write", minutes: 25, list: null, apps: [] }], ["Social", "Games"]);
    const rows = [...document.querySelectorAll(".mode")];
    expect(rows.length).toBe(2);
    expect(rows[0].querySelector<HTMLInputElement>(".mode-name")!.value).toBe("Study");
    expect(rows[0].querySelector<HTMLSelectElement>(".mode-length")!.value).toBe("50");
    const list = rows[0].querySelector<HTMLSelectElement>(".mode-list")!;
    expect(list.value).toBe("Social");
    expect([...list.options].map((o) => o.textContent)).toEqual(["What a session blocks", "Your “Social” list", "Your “Games” list"]);
    expect([...rows[0].querySelectorAll(".mode-app")].map((a) => a.firstChild!.textContent)).toEqual(["Word"]);
    expect(rows[0].querySelector(".mode-app-add")!.textContent).toBe("Add another app");
    // None yet: the way in says so, and no list is "what a session blocks".
    expect(rows[1].querySelector<HTMLSelectElement>(".mode-list")!.value).toBe("");
    expect(rows[1].querySelector(".mode-app-add")!.textContent).toBe("Add an app");
  });

  it("keeps a length that isn't one of the usual ones", () => {
    renderModes([{ ...study, minutes: 37 }], []);
    const length = $<HTMLSelectElement>(".mode-length");
    expect(length.value).toBe("37");
    expect([...length.options].map((o) => o.textContent)).toContain("37m");
  });

  it("saves a change to its length, list or name, under the name it had", async () => {
    api.saveFocusMode.mockResolvedValue([study]);
    renderModes([study], ["Social", "Games"]);
    const length = $<HTMLSelectElement>(".mode-length");
    length.value = "90";
    length.dispatchEvent(new Event("change"));
    expect(api.saveFocusMode).toHaveBeenLastCalledWith("Study", "Study", 90, "Social", [word]);
    const list = $<HTMLSelectElement>(".mode-list");
    list.value = "";
    list.dispatchEvent(new Event("change"));
    expect(api.saveFocusMode).toHaveBeenLastCalledWith("Study", "Study", 50, null, [word]);
    const name = $<HTMLInputElement>(".mode-name");
    name.value = "Deep work";
    name.dispatchEvent(new Event("change"));
    expect(api.saveFocusMode).toHaveBeenLastCalledWith("Study", "Deep work", 50, "Social", [word]);
    await settle();
  });

  it("takes an app away, and says why when a change is refused", async () => {
    api.saveFocusMode.mockResolvedValueOnce([{ ...study, apps: [] }]);
    renderModes([study], ["Social"]);
    $<HTMLButtonElement>(".mode-app-remove").click();
    expect(api.saveFocusMode).toHaveBeenLastCalledWith("Study", "Study", 50, "Social", []);
    await vi.waitFor(() => expect(document.querySelectorAll(".mode-app").length).toBe(0));

    api.saveFocusMode.mockRejectedValueOnce("There's already a mode called Write.");
    const name = $<HTMLInputElement>(".mode-name");
    name.value = "Write";
    name.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect($("#mode-status").textContent).toContain("There's already a mode called Write."));
  });

  it("adds an app from the installed ones, read once and not offering what it already opens", async () => {
    api.saveFocusMode.mockResolvedValue([{ ...study, apps: [word, chrome] }]);
    renderModes([study], ["Social"]);
    $<HTMLButtonElement>(".mode-app-add").click();
    await vi.waitFor(() => expect(document.querySelector(".mode-app-pick")).not.toBeNull());
    const pick = $<HTMLSelectElement>(".mode-app-pick");
    expect([...pick.options].map((o) => o.textContent)).toEqual(["Choose an app…", "Chrome", "Notion"]);
    expect(document.activeElement).toBe(pick);
    pick.value = chrome.path;
    pick.dispatchEvent(new Event("change"));
    expect(api.saveFocusMode).toHaveBeenLastCalledWith("Study", "Study", 50, "Social", [word, chrome]);
    await vi.waitFor(() => expect(document.querySelectorAll(".mode-app").length).toBe(2));
    // Another one doesn't read the apps again.
    $<HTMLButtonElement>(".mode-app-add").click();
    await vi.waitFor(() => expect(document.querySelector(".mode-app-pick")).not.toBeNull());
    expect(api.installedApps).toHaveBeenCalledTimes(1);
  });

  it("removes a mode on the second click only", async () => {
    api.removeFocusMode.mockResolvedValue([]);
    renderModes([study], []);
    const remove = $<HTMLButtonElement>(".mode .link-btn:not(.mode-app-remove):not(.mode-app-add)");
    remove.click();
    expect(api.removeFocusMode).not.toHaveBeenCalled();
    expect(remove.textContent).toBe("Click again to remove");
    remove.click();
    expect(api.removeFocusMode).toHaveBeenCalledWith("Study");
    await vi.waitFor(() => expect(document.querySelectorAll(".mode").length).toBe(0));
  });

  it("adds a mode of 25 minutes by its name, and says why when it can't", async () => {
    setUpModeForm(() => ["Social"]);
    api.saveFocusMode.mockResolvedValueOnce([{ name: "Study", minutes: 25, list: null, apps: [] }]);
    $<HTMLInputElement>("#mode-name").value = "Study";
    $<HTMLButtonElement>("#mode-add").click();
    expect(api.saveFocusMode).toHaveBeenLastCalledWith(null, "Study", 25, null, []);
    await vi.waitFor(() => expect(document.querySelectorAll(".mode").length).toBe(1));
    expect($<HTMLInputElement>("#mode-name").value).toBe("");

    api.saveFocusMode.mockRejectedValueOnce("Give the mode a name.");
    $<HTMLButtonElement>("#mode-add").click();
    await vi.waitFor(() => expect($("#mode-status").textContent).toBe("Give the mode a name."));
  });
});
