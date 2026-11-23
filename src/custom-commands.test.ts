import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  customCommands: vi.fn(),
  saveCustomCommand: vi.fn(),
  removeCustomCommand: vi.fn(),
  pickCommandPath: vi.fn(),
  setCustomKeyword: vi.fn(),
  setCustomKeys: vi.fn(),
  keywordsStatus: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { CustomCommand } from "./api";
import { keywordNote, loadCustomCommands, renderCustomCommands, setUpCustomForm, useKeysRecorder, type KeysRow } from "./custom-commands";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

const tax: CustomCommand = { name: "tax", kind: "open", target: "C:\\Users\\me\\Tax" };
const gh: CustomCommand = { name: "gh", kind: "link", target: "https://github.com" };

beforeEach(() => {
  document.body.innerHTML = `<div id="custom-list"></div><input id="custom-new-name" /><select id="custom-new-kind"></select><textarea id="custom-new-target"></textarea><p id="custom-new-pick" hidden><button id="custom-pick-file"></button><button id="custom-pick-folder"></button></p><button id="custom-add"></button><span id="custom-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
  api.keywordsStatus.mockResolvedValue("idle");
});

describe("Your commands in Kairo's settings", () => {
  it("shows each command's name, what it does and where", () => {
    renderCustomCommands([tax, gh]);
    expect([...document.querySelectorAll<HTMLInputElement>(".custom-name")].map((i) => i.value)).toEqual(["tax", "gh"]);
    expect([...document.querySelectorAll<HTMLSelectElement>(".custom-kind")].map((s) => s.value)).toEqual(["open", "link"]);
    expect([...document.querySelectorAll<HTMLTextAreaElement>(".custom-target")].map((t) => t.value)).toEqual(["C:\\Users\\me\\Tax", "https://github.com"]);
  });

  it("reads them when settings open", async () => {
    api.customCommands.mockResolvedValue([gh]);
    await loadCustomCommands();
    expect(document.querySelectorAll(".custom-command").length).toBe(1);
  });

  it("saves a change under the name it had, and says why when it is refused", async () => {
    api.saveCustomCommand.mockResolvedValueOnce([{ ...tax, target: "D:\\Tax" }]);
    renderCustomCommands([tax]);
    const target = $<HTMLTextAreaElement>(".custom-target");
    target.value = "D:\\Tax";
    target.dispatchEvent(new Event("change"));
    expect(api.saveCustomCommand).toHaveBeenLastCalledWith("tax", "tax", "open", "D:\\Tax");
    await vi.waitFor(() => expect($<HTMLTextAreaElement>(".custom-target").value).toBe("D:\\Tax"));

    api.saveCustomCommand.mockRejectedValueOnce("There's already a command called gh.");
    const name = $<HTMLInputElement>(".custom-name");
    name.value = "gh";
    name.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect($("#custom-status").textContent).toContain("There's already a command called gh."));
  });

  it("changes what it does", async () => {
    api.saveCustomCommand.mockResolvedValueOnce([{ ...tax, kind: "text" }]);
    renderCustomCommands([tax]);
    const kind = $<HTMLSelectElement>(".custom-kind");
    kind.value = "text";
    kind.dispatchEvent(new Event("change"));
    expect(api.saveCustomCommand).toHaveBeenLastCalledWith("tax", "tax", "text", "C:\\Users\\me\\Tax");
    await settle();
  });

  it("asks for what the kind chosen needs, then adds and clears the form", async () => {
    setUpCustomForm();
    const kind = $<HTMLSelectElement>("#custom-new-kind");
    const target = $<HTMLTextAreaElement>("#custom-new-target");
    expect(target.placeholder).toContain("github.com");
    kind.value = "text";
    kind.dispatchEvent(new Event("change"));
    expect(target.placeholder).toBe("The text to paste. {date}, {time} and {clipboard} are filled in");

    api.saveCustomCommand.mockResolvedValueOnce([{ name: "sig", kind: "text", target: "Best," }]);
    $<HTMLInputElement>("#custom-new-name").value = "sig";
    target.value = "Best,";
    $<HTMLButtonElement>("#custom-add").click();
    expect(api.saveCustomCommand).toHaveBeenCalledWith(null, "sig", "text", "Best,");
    await vi.waitFor(() => expect(document.querySelectorAll(".custom-command").length).toBe(1));
    expect($<HTMLInputElement>("#custom-new-name").value).toBe("");
    expect(target.value).toBe("");
    expect(document.activeElement).toBe($("#custom-new-name"));
  });

  it("offers the system's file and folder dialogs only for a path, and fills the box from them", async () => {
    setUpCustomForm();
    const picks = $("#custom-new-pick");
    // A link is typed; a path is picked.
    expect(picks.hidden).toBe(true);
    const kind = $<HTMLSelectElement>("#custom-new-kind");
    kind.value = "open";
    kind.dispatchEvent(new Event("change"));
    expect(picks.hidden).toBe(false);
    api.pickCommandPath.mockResolvedValueOnce("C:\Users\me\Tax");
    $<HTMLButtonElement>("#custom-pick-folder").click();
    expect(api.pickCommandPath).toHaveBeenLastCalledWith(true);
    await vi.waitFor(() => expect($<HTMLTextAreaElement>("#custom-new-target").value).toBe("C:\Users\me\Tax"));
    // Cancelled: what was there stays.
    api.pickCommandPath.mockResolvedValueOnce(null);
    $<HTMLButtonElement>("#custom-pick-file").click();
    expect(api.pickCommandPath).toHaveBeenLastCalledWith(false);
    await settle();
    expect($<HTMLTextAreaElement>("#custom-new-target").value).toBe("C:\Users\me\Tax");
    kind.value = "text";
    kind.dispatchEvent(new Event("change"));
    expect(picks.hidden).toBe(true);
  });

  it("puts a picked path on a command that opens one, and saves it", async () => {
    renderCustomCommands([tax, gh]);
    // Only the command that opens a path has the dialogs.
    expect(document.querySelectorAll(".custom-pick").length).toBe(1);
    api.pickCommandPath.mockResolvedValueOnce("D:\Tax");
    api.saveCustomCommand.mockResolvedValueOnce([{ ...tax, target: "D:\Tax" }, gh]);
    document.querySelector<HTMLButtonElement>(".custom-pick .link-btn")!.click();
    await vi.waitFor(() => expect(api.saveCustomCommand).toHaveBeenCalledWith("tax", "tax", "open", "D:\Tax"));
  });

  it("says why the dialog couldn't open", async () => {
    setUpCustomForm();
    $<HTMLSelectElement>("#custom-new-kind").value = "open";
    $<HTMLSelectElement>("#custom-new-kind").dispatchEvent(new Event("change"));
    api.pickCommandPath.mockRejectedValueOnce("Try again in a moment.");
    $<HTMLButtonElement>("#custom-pick-file").click();
    await vi.waitFor(() => expect($("#custom-status").textContent).toBe("Try again in a moment."));
  });

  it("keeps what was typed and says why when adding is refused", async () => {
    setUpCustomForm();
    api.saveCustomCommand.mockRejectedValueOnce("Give the command a name.");
    $<HTMLTextAreaElement>("#custom-new-target").value = "x";
    $<HTMLButtonElement>("#custom-add").click();
    await vi.waitFor(() => expect($("#custom-status").textContent).toBe("Give the command a name."));
    expect($<HTMLTextAreaElement>("#custom-new-target").value).toBe("x");
  });

  it("takes a command away on the second press", async () => {
    api.removeCustomCommand.mockResolvedValueOnce([]);
    renderCustomCommands([tax]);
    const remove = $<HTMLButtonElement>(".custom-command .link-btn");
    remove.click();
    expect(api.removeCustomCommand).not.toHaveBeenCalled();
    remove.click();
    expect(api.removeCustomCommand).toHaveBeenCalledWith("tax");
    await vi.waitFor(() => expect(document.querySelectorAll(".custom-command").length).toBe(0));
  });

  it("shows a command's keys, records new ones with Settings' recorder, and clears them", async () => {
    const started: KeysRow[] = [];
    useKeysRecorder((row) => started.push(row), (keys) => keys.split("+").map((k) => Object.assign(document.createElement("kbd"), { textContent: k })));
    renderCustomCommands([{ ...gh, keys: "Ctrl+Alt+G" }, tax]);
    const rows = [...document.querySelectorAll(".custom-command")];
    expect([...rows[0].querySelectorAll(".shortcut-keys kbd")].map((k) => k.textContent)).toEqual(["Ctrl", "Alt", "G"]);
    expect(rows[1].querySelector(".shortcut-keys")!.textContent).toBe("No keys");
    expect(rows[1].querySelector<HTMLButtonElement>(".custom-clear-keys")!.hidden).toBe(true);
    rows[1].querySelector<HTMLButtonElement>(".custom-record")!.click();
    expect(started[0].does).toBe("runs tax");
    // Saving goes to the command, and a refusal is thrown back for the recorder to say.
    api.setCustomKeys.mockRejectedValueOnce("Ctrl+Space opens the command palette.");
    await expect(started[0].save("Ctrl+Space")).rejects.toBe("Ctrl+Space opens the command palette.");
    expect(api.setCustomKeys).toHaveBeenCalledWith("tax", "Ctrl+Space");
    api.setCustomKeys.mockResolvedValueOnce([gh, tax]);
    rows[0].querySelector<HTMLButtonElement>(".custom-clear-keys")!.click();
    await settle();
    expect(api.setCustomKeys).toHaveBeenLastCalledWith("gh", "");
  });

  it("gives text, and only text, a keyword box that saves as it changes", async () => {
    const sig: CustomCommand = { name: "sig", kind: "text", target: "Best", keyword: ";sig" };
    renderCustomCommands([sig, gh]);
    const boxes = document.querySelectorAll<HTMLInputElement>(".custom-keyword");
    expect([...boxes].map((b) => b.value)).toEqual([";sig"]);
    api.setCustomKeyword.mockResolvedValueOnce([{ ...sig, keyword: ";s" }, gh]);
    boxes[0].value = ";s";
    boxes[0].dispatchEvent(new Event("change"));
    await settle();
    expect(api.setCustomKeyword).toHaveBeenCalledWith("sig", ";s");
    expect($<HTMLInputElement>(".custom-keyword").value).toBe(";s");
  });

  it("says where keywords can't work, and turns the box off there", async () => {
    api.keywordsStatus.mockResolvedValue("unavailable");
    api.customCommands.mockResolvedValue([{ name: "sig", kind: "text", target: "Best" }]);
    await loadCustomCommands();
    expect($<HTMLInputElement>(".custom-keyword").disabled).toBe(true);
    expect($(".custom-keyword-note").textContent).toContain("Wayland");
    expect(keywordNote("waiting", true)).toContain("Input Monitoring");
  });
});
