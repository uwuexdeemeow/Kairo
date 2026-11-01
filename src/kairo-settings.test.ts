import { beforeEach, describe, expect, it, vi } from "vitest";
import { dashboardBody as markup } from "./test-dashboard";

const api = vi.hoisted(() => ({
  version: vi.fn(),
  sync: vi.fn(),
  stopSync: vi.fn(),
  chooseSyncFolder: vi.fn(),
  pairStart: vi.fn(),
  pairStatus: vi.fn(),
  pairCancel: vi.fn(),
  pairJoin: vi.fn(),
  startOver: vi.fn(),
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

const OFF = { mode: "off", folder: null, name: "Tower", computers: [], shown: "all", syncedMs: null, error: null };
const ON = {
  ...OFF,
  mode: "folder",
  folder: "C:/Drive/Kairo",
  computers: [{ id: "a", name: "Tower", thisOne: true, updatedMs: 0 }],
};
const RELAY = {
  ...OFF,
  mode: "relay",
  computers: [
    { id: "a", name: "Tower", thisOne: true, updatedMs: 0 },
    { id: "b", name: "Laptop", thisOne: false, updatedMs: Date.now() },
  ],
  syncedMs: Date.now(),
};
const CODE = { code: "K7QX-49MP", expiresMs: Date.now() + 10 * 60 * 1000, state: "waiting" };
const shown = (sel: string) => !$(sel).hidden;

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
  api.pairStatus.mockResolvedValue(null);
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

  it("offers a code first, and a folder after", () => {
    expect(shown("#sync-link") && shown("#sync-enter") && shown("#sync-choose")).toBe(true);
    expect($("#sync-link").textContent).toBe("Link a computer");
    expect($("#sync-choose").textContent).toBe("Use a folder instead");
    expect(shown("#sync-over")).toBe(false);
  });

  it("offers no code while combining through a folder", async () => {
    api.chooseSyncFolder.mockResolvedValue(ON);
    $("#sync-choose").click();
    await vi.waitFor(() => expect($("#sync-choose").textContent).toBe("Use another folder"));
    expect(shown("#sync-link") || shown("#sync-enter") || shown("#sync-over")).toBe(false);
  });
});

describe("linking with a code", () => {
  async function relayOn() {
    api.sync.mockResolvedValue(RELAY);
    await settings.loadKairoSettings();
  }

  it("shows what combining through the relay means, and what can be done", async () => {
    await relayOn();
    expect($("#sync-where").textContent).toBe("Combined through Kairo's relay, encrypted end to end: the relay can't read your time.");
    expect($("#sync-link").textContent).toBe("Link another computer");
    expect(shown("#sync-link") && shown("#sync-stop") && shown("#sync-over")).toBe(true);
    expect(shown("#sync-enter") || shown("#sync-choose")).toBe(false);
    expect($("#sync-computers").textContent).toContain("Laptop");
  });

  it("says when the relay couldn't be reached", async () => {
    api.sync.mockResolvedValue({ ...RELAY, syncedMs: Date.now() - 2 * 60 * 60 * 1000, error: "Can't reach Kairo's relay right now." });
    await settings.loadKairoSettings();
    expect($("#sync-where").textContent).toContain("Last synced 2 h ago. Can't reach Kairo's relay right now.");
  });

  it("says when another computer started over", async () => {
    api.sync.mockResolvedValue({ ...OFF, error: "unlinked" });
    await settings.loadKairoSettings();
    expect($("#sync-where").textContent).toBe("This computer was unlinked. Enter a new code to combine again.");
  });

  it("shows a code with the time it has left, and who linked", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    try {
      const code = { ...CODE, expiresMs: Date.now() + 10 * 60 * 1000 };
      api.pairStart.mockResolvedValue(code);
      api.pairStatus.mockResolvedValue(code);
      $("#sync-link").click();
      await vi.waitFor(() => expect($("#sync-code-text").textContent).toBe("K7QX-49MP"));
      expect(shown("#sync-code")).toBe(true);
      expect($("#sync-code-note").textContent).toBe("Type this on your other computer. It works once, for 10 minutes.");
      expect($("#sync-code-left").textContent).toBe("10:00 left");

      api.pairStatus.mockResolvedValue({ ...CODE, state: "linked", name: "Laptop" });
      api.sync.mockResolvedValue(RELAY);
      await vi.advanceTimersByTimeAsync(1000);
      await vi.waitFor(() => expect($("#sync-status").textContent).toBe("Linked with Laptop."));
      expect(shown("#sync-code")).toBe(false);
      expect($("#sync-link").textContent).toBe("Link another computer");
    } finally {
      vi.useRealTimers();
    }
  });

  it("says when a code ran out or didn't match", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    try {
      api.pairStart.mockResolvedValue(CODE);
      api.pairStatus.mockResolvedValue({ ...CODE, state: "mismatch" });
      $("#sync-link").click();
      await vi.advanceTimersByTimeAsync(1000);
      await vi.waitFor(() => expect($("#sync-status").textContent).toBe("That code didn't match. Make a new one."));
      api.pairStatus.mockResolvedValue({ ...CODE, state: "expired" });
      $("#sync-link").click();
      await vi.advanceTimersByTimeAsync(1000);
      await vi.waitFor(() => expect($("#sync-status").textContent).toBe("The code ran out. Make a new one."));
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels a code", async () => {
    api.pairStart.mockResolvedValue(CODE);
    $("#sync-link").click();
    await vi.waitFor(() => expect(shown("#sync-code")).toBe(true));
    $("#sync-code-cancel").click();
    expect(api.pairCancel).toHaveBeenCalled();
    expect(shown("#sync-code")).toBe(false);
  });

  it("links with a code typed from another computer", async () => {
    api.pairJoin.mockResolvedValue({ from: "Desk-PC", sync: RELAY });
    $("#sync-enter").click();
    expect(shown("#sync-join")).toBe(true);
    $<HTMLInputElement>("#sync-join-code").value = "k7qx 49mp";
    $("#sync-join").dispatchEvent(new Event("submit", { cancelable: true }));
    expect($("#sync-status").textContent).toBe("Linking…");
    await vi.waitFor(() => expect($("#sync-status").textContent).toBe("Linked with Desk-PC."));
    expect(api.pairJoin).toHaveBeenCalledWith("k7qx 49mp");
    expect(shown("#sync-join")).toBe(false);
    expect(shown("#sync-over")).toBe(true);
  });

  it("says why a typed code didn't link", async () => {
    api.pairJoin.mockRejectedValue("That code ran out.");
    $("#sync-enter").click();
    $<HTMLInputElement>("#sync-join-code").value = "K7QX-49MP";
    $("#sync-join").dispatchEvent(new Event("submit", { cancelable: true }));
    await vi.waitFor(() => expect($("#sync-status").textContent).toBe("That code ran out."));
    expect(shown("#sync-join")).toBe(true);
  });

  it("asks before starting over, then shows a new code", async () => {
    await relayOn();
    api.startOver.mockResolvedValue(CODE);
    $("#sync-over").click();
    expect(shown("#sync-confirm")).toBe(true);
    expect(api.startOver).not.toHaveBeenCalled();
    $("#sync-confirm-no").click();
    expect(shown("#sync-confirm")).toBe(false);
    $("#sync-over").click();
    $("#sync-confirm-yes").click();
    await vi.waitFor(() => expect($("#sync-code-text").textContent).toBe("K7QX-49MP"));
    expect(api.startOver).toHaveBeenCalledTimes(1);
  });

  it("picks up the computers list when a sync round finishes", async () => {
    api.sync.mockResolvedValue(RELAY);
    for (const fn of events.get("kairo:sync") ?? []) fn({ payload: null });
    await vi.waitFor(() => expect($("#sync-computers").textContent).toContain("Laptop"));
  });
});
