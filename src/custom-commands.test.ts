import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  customCommands: vi.fn(),
  saveCustomCommand: vi.fn(),
  removeCustomCommand: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { CustomCommand } from "./api";
import { loadCustomCommands, renderCustomCommands, setUpCustomForm } from "./custom-commands";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const settle = () => new Promise((r) => setTimeout(r, 0));

const tax: CustomCommand = { name: "tax", kind: "open", target: "C:\\Users\\me\\Tax" };
const gh: CustomCommand = { name: "gh", kind: "link", target: "https://github.com" };

beforeEach(() => {
  document.body.innerHTML = `<div id="custom-list"></div><input id="custom-new-name" /><select id="custom-new-kind"></select><textarea id="custom-new-target"></textarea><button id="custom-add"></button><span id="custom-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
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
    expect(target.placeholder).toBe("The text to copy");

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
});
