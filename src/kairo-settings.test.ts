import { beforeEach, describe, expect, it, vi } from "vitest";
import { dashboardBody as markup } from "./test-dashboard";

const api = vi.hoisted(() => ({
  version: vi.fn(),
  sync: vi.fn(),
  stopSync: vi.fn(),
  chooseSyncFolder: vi.fn(),
  appearance: vi.fn(),
  setAppearance: vi.fn(),
  paletteSettings: vi.fn(),
  setPaletteSearchFiles: vi.fn(),
  setPaletteShortcut: vi.fn(),
  autostart: vi.fn(),
}));
const events = vi.hoisted(() => new Map<string, ((e: { payload: unknown }) => void)[]>());
vi.mock("./api", () => ({ api }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, fn: (e: { payload: unknown }) => void) => {
    events.set(name, [...(events.get(name) ?? []), fn]);
    return () => {};
  },
}));

type Settings = typeof import("./kairo-settings");

const OFF = { folder: null, name: "Tower", computers: [], shown: "all" };
const ON = { folder: "C:/Drive/Kairo", name: "Tower", computers: [{ id: "a", name: "Tower", thisOne: true, updatedMs: 0 }], shown: "all" };

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const press = (key: string, code: string, init: KeyboardEventInit = {}) =>
  document.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, cancelable: true, ...init }));
const keys = () => [...$("#palette-keys").querySelectorAll("kbd")].map((k) => k.textContent);

let settings: Settings;

beforeEach(async () => {
  vi.resetModules();
  events.clear();
  for (const fn of Object.values(api)) fn.mockReset();
  api.version.mockResolvedValue("1.19.3");
  api.sync.mockResolvedValue(OFF);
  api.appearance.mockResolvedValue("dark");
  api.paletteSettings.mockResolvedValue({ shortcut: "Ctrl+Space", suggestions: true, overFullscreen: false, searchFiles: true });
  api.autostart.mockResolvedValue({ enabled: false, available: true });
  api.setPaletteShortcut.mockResolvedValue(undefined);
  document.body.innerHTML = markup;
  settings = await import("./kairo-settings");
  settings.wireKairoSettings();
  await settings.loadKairoSettings();
});

describe("loading", () => {
  it("fills the controls from what's saved", () => {
    expect($("#settings-version").textContent).toBe("Kairo 1.19.3");
    expect($<HTMLSelectElement>("#appearance").value).toBe("dark");
    expect(keys()).toEqual(["Ctrl", "Space"]);
    expect($<HTMLInputElement>("#palette-suggestions").checked).toBe(true);
    expect($<HTMLInputElement>("#palette-search-files").checked).toBe(true);
    expect($("#sync-where").textContent).toBe("Off. This computer would show up as Tower.");
    expect($("#sync-stop").hidden).toBe(true);
  });

  it("names the Windows key the way Windows does", () => {
    expect(settings.shortcutKeys("Super+K").map((k) => k.textContent)).toEqual(["Win", "K"]);
  });
});

describe("settings changed from the palette", () => {
  it("reload the controls when Kairo says they changed", async () => {
    api.paletteSettings.mockClear();
    api.paletteSettings.mockResolvedValue({ shortcut: "Ctrl+Space", suggestions: false, overFullscreen: false, searchFiles: false });
    for (const fn of events.get("kairo:settings") ?? []) fn({ payload: null });
    await vi.waitFor(() => expect($<HTMLInputElement>("#palette-suggestions").checked).toBe(false));
    expect(api.paletteSettings).toHaveBeenCalledTimes(1);
    expect($<HTMLInputElement>("#palette-search-files").checked).toBe(false);
  });

  it("saves the Search files switch", () => {
    const box = $<HTMLInputElement>("#palette-search-files");
    box.checked = false;
    box.dispatchEvent(new Event("change"));
    expect(api.setPaletteSearchFiles).toHaveBeenCalledWith(false);
  });
});

describe("the Theme dropdown", () => {
  it("follows a theme chosen in another window", () => {
    for (const fn of events.get("kairo:appearance") ?? []) fn({ payload: "light" });
    expect($<HTMLSelectElement>("#appearance").value).toBe("light");
  });

  it("saves and paints a choice", async () => {
    const select = $<HTMLSelectElement>("#appearance");
    select.value = "light";
    select.dispatchEvent(new Event("change"));
    expect(api.setAppearance).toHaveBeenCalledWith("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});

describe("recording the palette shortcut", () => {
  beforeEach(() => $("#palette-record").click());

  it("saves the next key pressed with a modifier", async () => {
    expect($("#palette-record").textContent).toBe("Cancel");
    press("k", "KeyK", { ctrlKey: true, shiftKey: true });
    await vi.waitFor(() => expect(api.setPaletteShortcut).toHaveBeenCalledWith("Ctrl+Shift+K"));
    await vi.waitFor(() => expect($("#palette-note").textContent).toBe("Saved. Ctrl+Shift+K opens the palette from anywhere."));
    expect(keys()).toEqual(["Ctrl", "Shift", "K"]);
    expect($("#palette-record").textContent).toBe("Change");
  });

  it("asks for a modifier when a key comes alone", () => {
    press("k", "KeyK");
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
    expect($("#palette-note").textContent).toContain("Hold Ctrl, Alt, Shift or the Windows key");
  });

  it("keeps the shortcut on Esc", () => {
    press("Escape", "Escape");
    expect($("#palette-note").textContent).toBe("Kept as it was.");
    expect(keys()).toEqual(["Ctrl", "Space"]);
  });

  it("stops when cancelled from outside, so later keys don't set it", () => {
    settings.cancelShortcutRecording();
    expect($("#palette-record").textContent).toBe("Change");
    const e = new KeyboardEvent("keydown", { key: "k", code: "KeyK", ctrlKey: true, cancelable: true });
    document.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(false);
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
    expect(keys()).toEqual(["Ctrl", "Space"]);
  });

  it("says why a shortcut couldn't be saved", async () => {
    api.setPaletteShortcut.mockRejectedValue("Another app already uses Ctrl+K.");
    press("k", "KeyK", { ctrlKey: true });
    await vi.waitFor(() => expect($("#palette-note").textContent).toBe("Another app already uses Ctrl+K."));
    expect(keys()).toEqual(["Ctrl", "Space"]);
  });
});

describe("combining computers", () => {
  it("shows a chosen folder and the computers in it", async () => {
    api.chooseSyncFolder.mockResolvedValue(ON);
    $("#sync-choose").click();
    await vi.waitFor(() => expect($("#sync-where").textContent).toBe("Combining through C:/Drive/Kairo."));
    expect($("#sync-stop").hidden).toBe(false);
    expect($("#sync-status").textContent).toBe("Set. Choose the same folder on your other computers.");
  });

  it("changes nothing when no folder is chosen", async () => {
    api.chooseSyncFolder.mockResolvedValue(null);
    $("#sync-choose").click();
    await vi.waitFor(() => expect(api.chooseSyncFolder).toHaveBeenCalled());
    expect($("#sync-status").textContent).toBe("");
  });

  it("stops combining", async () => {
    api.stopSync.mockResolvedValue(OFF);
    $("#sync-stop").click();
    await vi.waitFor(() => expect($("#sync-status").textContent).toContain("Stopped."));
    expect($("#sync-stop").hidden).toBe(true);
  });

  it("says when stopping fails", async () => {
    api.stopSync.mockRejectedValue("the folder can't be reached");
    $("#sync-stop").click();
    await vi.waitFor(() => expect($("#sync-status").textContent).toBe("Couldn't stop: the folder can't be reached"));
  });
});
