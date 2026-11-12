import { beforeEach, describe, expect, it, vi } from "vitest";
import { dashboardBody as markup } from "./test-dashboard";

const api = vi.hoisted(() => ({
  ringSettings: vi.fn(),
  setRingShortcut: vi.fn(),
  setRingItems: vi.fn(),
  setRingFocusMinutes: vi.fn(),
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
  clipboardHistory: vi.fn(),
  setClipboardHistory: vi.fn(),
  setPaletteShortcut: vi.fn(),
  recordingShortcut: vi.fn(async () => {}),
  autostart: vi.fn(),
  setAutostart: vi.fn(),
  setPaletteSuggestions: vi.fn(),
  checkForUpdates: vi.fn(async () => {}),
}));
const events = vi.hoisted(() => new Map<string, ((e: { payload: unknown }) => void)[]>());
vi.mock("./api", () => ({ api }));
const vt = vi.hoisted(() => ({ hasKey: vi.fn(async () => false), setKey: vi.fn(async () => {}), testKey: vi.fn(async () => {}) }));
vi.mock("./virustotal-api", () => ({ vt }));
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
  api.clipboardHistory.mockResolvedValue(true);
  api.setClipboardHistory.mockResolvedValue(undefined);
  api.setPaletteShortcut.mockResolvedValue(undefined);
  const ring = { items: ["chrono", "ledger", "palette"], all: [["chrono", "Chrono"], ["ledger", "Ledger"], ["palette", "Palette"], ["week", "This week"]], focusMinutes: 25 };
  api.ringSettings.mockResolvedValue({ shortcut: "Alt+Q", ...ring });
  api.setRingShortcut.mockResolvedValue({ shortcut: "Alt+R", ...ring });
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
    expect($<HTMLInputElement>("#clipboard-history").checked).toBe(true);
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

describe("the VirusTotal key", () => {
  it("is saved as it's pasted, never shown again, and can be tested", async () => {
    const key = $<HTMLInputElement>("#vt-key");
    await vi.waitFor(() => expect(key.placeholder).toBe("Paste your key"));
    key.value = "  abc123  ";
    key.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(vt.setKey).toHaveBeenCalledWith("  abc123  "));
    await vi.waitFor(() => expect(key.value).toBe(""));
    expect(key.placeholder).toBe("Saved. Paste a new one to change it");
    $<HTMLButtonElement>("#vt-test").click();
    await vi.waitFor(() => expect($("#vt-note").textContent).toBe("Works. The free key allows 4 lookups a minute and 500 a day."));
    expect($("#vt-note .glyph-check")).not.toBeNull();
    vt.testKey.mockRejectedValueOnce("VirusTotal didn't accept the key. Check it in Kairo's settings.");
    $<HTMLButtonElement>("#vt-test").click();
    await vi.waitFor(() => expect($("#vt-note").textContent).toBe("VirusTotal didn't accept the key. Check it in Kairo's settings."));
  });
});

describe("what's on the app ring", () => {
  it("lists the items and saves a change through Kairo", async () => {
    await vi.waitFor(() => expect(document.querySelectorAll("#ring-items .ring-choice").length).toBe(11));
    api.setRingItems.mockResolvedValue({ shortcut: "Alt+Q", items: ["chrono", "ledger", "palette", "week"], all: [], focusMinutes: 25 });
    $<HTMLInputElement>('#ring-items [data-item="week"] input[type=checkbox]').click();
    await vi.waitFor(() => expect(api.setRingItems).toHaveBeenCalledWith(["chrono", "ledger", "palette", "week"]));
  });
});

describe("the app ring's shortcut", () => {
  const ringKeys = () => [...document.querySelectorAll("#ring-keys kbd")].map((k) => k.textContent);

  it("shows the saved one", async () => {
    await vi.waitFor(() => expect(ringKeys()).toEqual(["Alt", "Q"]));
  });

  it("records a new one apart from the palette's", async () => {
    $("#ring-record").click();
    expect($("#ring-record").textContent).toBe("Cancel");
    expect($("#palette-record").textContent).toBe("Change");
    press("r", "KeyR", { altKey: true });
    await vi.waitFor(() => expect(api.setRingShortcut).toHaveBeenCalledWith("Alt+R"));
    await vi.waitFor(() => expect($("#ring-note").textContent).toBe("Saved. Alt+R opens the app ring from anywhere."));
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
  });

  it("stops recording the palette's when the ring's starts", () => {
    $("#palette-record").click();
    $("#ring-record").click();
    expect($("#palette-record").textContent).toBe("Change");
    expect($("#ring-record").textContent).toBe("Cancel");
  });
});

describe("switches", () => {
  it("say Saved beside their words once saved", async () => {
    api.setPaletteSuggestions.mockResolvedValue(undefined);
    const box = $<HTMLInputElement>("#palette-suggestions");
    box.click();
    await vi.waitFor(() => expect(box.closest("label")!.querySelector(".saved-mark")!.textContent).toBe("Saved"));
    expect(api.setPaletteSuggestions).toHaveBeenCalledWith(false);
  });

  it("flip back and say so when they can't be saved", async () => {
    api.setAutostart.mockRejectedValue("not allowed");
    const box = $<HTMLInputElement>("#autostart-enabled");
    box.click();
    await vi.waitFor(() => expect(box.checked).toBe(false));
    expect(box.closest("label")!.querySelector(".saved-mark")!.textContent).toBe("Couldn't save");
    expect($("#autostart-note").textContent).toBe("Couldn't change it: not allowed");
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

  it("lets the window know while recording, so Windows' Alt+Space menu stays away, and when it stops", async () => {
    expect(api.recordingShortcut).toHaveBeenLastCalledWith(true);
    press(" ", "Space", { altKey: true });
    await vi.waitFor(() => expect(api.setPaletteShortcut).toHaveBeenCalledWith("Alt+Space"));
    await vi.waitFor(() => expect(api.recordingShortcut).toHaveBeenLastCalledWith(false));
  });

  it("asks for a modifier when a key comes alone", () => {
    press("k", "KeyK");
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
    expect($("#palette-note").textContent).toContain("Hold Ctrl, Alt, Shift or the Windows key");
  });

  it("says when a key can't be used, and waits while only modifiers are held", () => {
    press("Control", "ControlLeft", { ctrlKey: true });
    expect($("#palette-note").textContent).not.toContain("can't be used");
    press("Tab", "Tab", { ctrlKey: true });
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
    expect($("#palette-note").textContent).toBe("Tab can't be used in a shortcut. Try a letter, a number, F1 to F12 or Space, with a modifier.");
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

  it("stops combining once asked, the safe answer first", async () => {
    api.stopSync.mockResolvedValue(OFF);
    $("#sync-stop").click();
    // It asks, saying what happens; nothing has stopped yet.
    expect(shown("#sync-confirm")).toBe(true);
    expect($("#sync-confirm-text").textContent).toContain("the other computers drop this one's too");
    expect(document.activeElement).toBe($("#sync-confirm-no"));
    expect(api.stopSync).not.toHaveBeenCalled();
    $("#sync-confirm-no").click();
    expect(shown("#sync-confirm")).toBe(false);
    expect(api.stopSync).not.toHaveBeenCalled();
    $("#sync-stop").click();
    $("#sync-confirm-yes").click();
    await vi.waitFor(() => expect($("#sync-status").textContent).toContain("Stopped."));
    expect($("#sync-stop").hidden).toBe(true);
  });

  it("says when stopping fails", async () => {
    api.stopSync.mockRejectedValue("the folder can't be reached");
    $("#sync-stop").click();
    $("#sync-confirm-yes").click();
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

describe("errors in plain words", () => {
  it("shows Kairo's own messages as they are, and says what failed before a raw one", () => {
    expect(settings.plain("That code has expired. Make a new one.", "link")).toBe("That code has expired. Make a new one.");
    expect(settings.plain("Access is denied. (os error 5)", "use that folder")).toBe("Couldn't use that folder. Try again in a moment. (Access is denied. (os error 5))");
    expect(settings.plain("network unreachable", "reach the relay")).toBe("Couldn't reach the relay. Try again in a moment. (network unreachable)");
  });
});

describe("checking for updates", () => {
  it("shows the answer beside the button, not only as a notification", async () => {
    $("#update-check").click();
    await vi.waitFor(() => expect($("#update-status").textContent).toBe("Checking…"));
    for (const fn of events.get("kairo:update") ?? []) fn({ payload: "Kairo is up to date. You have the latest version, 1.31.13." });
    expect($("#update-status").textContent).toBe("Kairo is up to date. You have the latest version, 1.31.13.");
  });
});

describe("finding a setting", () => {
  const find = (q: string) => {
    const field = $<HTMLInputElement>("#settings-find");
    field.value = q;
    field.dispatchEvent(new Event("input"));
  };
  const visibleSections = () => [...document.querySelectorAll<HTMLElement>(".settings-group:not([hidden]) .block:not([hidden])")].map((b) => b.querySelector("h3")?.textContent ?? b.getAttribute("aria-label"));

  it("keeps the sections that mention it, and their groups", () => {
    find("theme");
    expect(visibleSections()).toEqual(["Appearance"]);
    expect($("#group-shortcuts").hidden).toBe(true);
    expect($("#settings-none").hidden).toBe(true);
    find("backup");
    expect(visibleSections()).toEqual(["Your data"]);
  });

  it("says when nothing matches, and Esc brings everything back", () => {
    find("zzz");
    expect($("#settings-none").hidden).toBe(false);
    const field = $<HTMLInputElement>("#settings-find");
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(field.value).toBe("");
    expect($("#settings-none").hidden).toBe(true);
    expect(visibleSections().length).toBe(9);
  });

  it("goes to the first match's control on Enter", () => {
    Element.prototype.scrollIntoView = vi.fn();
    find("theme");
    $<HTMLInputElement>("#settings-find").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe($("#appearance"));
  });

  it("matches each app's row on its own", () => {
    $("#app-settings").innerHTML = `<li><strong>Chrono</strong> Goals</li><li><strong>Ledger</strong> Reminders</li>`;
    find("reminder");
    expect([...document.querySelectorAll<HTMLElement>("#app-settings li")].map((li) => li.hidden)).toEqual([true, false]);
  });

  it("finds a setting kept in an app's own window, and names it under the app", () => {
    $("#app-settings").innerHTML = `<li><div><strong>Chrono</strong><p>Goals</p><ul class="app-finds" hidden><li data-finds="erase tracked time delete history" hidden><button>Erase tracked time</button></li><li data-finds="strict mode end early" hidden><button>Strict mode</button></li></ul></div></li><li><div><strong>Ledger</strong><p>Reminders</p></div></li>`;
    const hidden = (s: string) => [...document.querySelectorAll<HTMLElement>(s)].map((e) => e.hidden);
    find("delete");
    expect(hidden("#app-settings > li")).toEqual([false, true]);
    expect(hidden("#app-settings [data-finds]")).toEqual([false, true]);
    expect($("#app-settings .app-finds").hidden).toBe(false);
    expect($("#settings-none").hidden).toBe(true);
    // Its own words still find the row, with none of its settings named.
    find("goals");
    expect(hidden("#app-settings > li")).toEqual([false, true]);
    expect($("#app-settings .app-finds").hidden).toBe(true);
    find("");
    expect(hidden("#app-settings [data-finds]")).toEqual([true, true]);
  });
});

describe("a section that can't be read", () => {
  it("says so and holds its controls, while the others fill in as normal", async () => {
    api.paletteSettings.mockRejectedValue("database is locked");
    await settings.loadKairoSettings();
    const palette = $("#palette-settings");
    expect(palette.querySelector(".load-failed")!.textContent).toBe("Couldn't read this. Try again in a moment.");
    expect($<HTMLInputElement>("#palette-suggestions").disabled).toBe(true);
    // The ring's section, read after it, is filled regardless.
    expect([...document.querySelectorAll("#ring-keys kbd")].map((k) => k.textContent)).toEqual(["Alt", "Q"]);
    // Read again once it can be: the line goes and the controls come back.
    api.paletteSettings.mockResolvedValue({ shortcut: "Alt+Space", suggestions: true, overFullscreen: false, searchFiles: true });
    await settings.loadKairoSettings();
    expect(palette.querySelector(".load-failed")).toBeNull();
    expect($<HTMLInputElement>("#palette-suggestions").disabled).toBe(false);
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

  it("says what the relay said when it answered with a problem", async () => {
    const full = "Kairo's relay said: it's full: these computers' history together is over the 50 MB it keeps. Keep less history in Settings → Privacy.";
    api.sync.mockResolvedValue({ ...RELAY, syncedMs: Date.now() - 2 * 60 * 60 * 1000, error: full });
    await settings.loadKairoSettings();
    expect($("#sync-where").textContent).toContain(`Last synced 2 h ago. ${full}`);
    expect($("#sync-where").textContent).not.toContain("Can't reach");
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

  it("puts away the code shown when another is to be typed", () => {
    api.pairCancel.mockClear();
    $("#sync-enter").click();
    expect(api.pairCancel).toHaveBeenCalled();
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
