import { beforeEach, describe, expect, it, vi } from "vitest";
import { dashboardBody as markup } from "./test-dashboard";

const api = vi.hoisted(() => ({
  status: vi.fn(),
  dayTotals: vi.fn(),
  day: vi.fn(),
  paletteSettings: vi.fn(),
  takeDashboardTab: vi.fn(),
  openChrono: vi.fn(),
  openPalette: vi.fn(),
  openLedger: vi.fn(),
  openRing: vi.fn(),
  resumeTracking: vi.fn(async () => {}),
  setTaskDone: vi.fn(async () => []),
  ringSettings: vi.fn(),
  tasks: vi.fn(),
  appearance: vi.fn(),
  setAppearance: vi.fn(),
  setPaletteShortcut: vi.fn(),
  version: vi.fn(),
  sync: vi.fn(),
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
// The dropdowns have tests of their own, and hook the page's setters once for good.
vi.mock("./dropdown", () => ({ useDropdowns: () => {}, shownControl: (el: HTMLElement) => el }));

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const emit = (name: string) => events.get(name)?.forEach((fn) => fn({ payload: null }));
const line = (id: string) => $(`[data-line="${id}"]`).textContent;
const onTab = () => ($("#tab-settings").hidden ? "home" : "settings");

/// Opens the dashboard as its window does, asked for `tab` (or nothing).
async function open(tab: string | null = null) {
  api.takeDashboardTab.mockResolvedValueOnce(tab);
  await import("./dashboard");
  await vi.waitFor(() => expect(api.takeDashboardTab).toHaveBeenCalled());
}

beforeEach(() => {
  vi.resetModules();
  events.clear();
  for (const fn of Object.values(api)) fn.mockReset();
  api.status.mockResolvedValue({ tracking: true });
  api.dayTotals.mockResolvedValue([{ date: "2026-10-29", activeMs: 125 * 60_000, productiveMs: 50 * 60_000 }]);
  api.paletteSettings.mockResolvedValue({ shortcut: "Alt+Space", suggestions: true, overFullscreen: false });
  api.takeDashboardTab.mockResolvedValue(null);
  api.appearance.mockResolvedValue("dark");
  api.version.mockResolvedValue("1.19.3");
  api.sync.mockResolvedValue({ folder: null, name: "Tower", computers: [], shown: "all" });
  api.autostart.mockResolvedValue({ enabled: false, available: true });
  api.tasks.mockResolvedValue([{ id: "a", group: "today" }, { id: "b", group: "overdue" }]);
  api.ringSettings.mockResolvedValue({ shortcut: "Alt+Q", items: ["chrono", "ledger", "notes", "week"], all: [], focusMinutes: 25 });
  document.body.innerHTML = markup;
});

describe("Home", () => {
  it("shows a tile for each app, with today's line", async () => {
    await open();
    // Each name is the tile's one button, read as "Open Chrono".
    expect([...document.querySelectorAll(".app-tile h2")].map((t) => t.textContent)).toEqual(["Open Chrono", "Open Ledger", "Open Command palette", "Open App ring"]);
    expect(document.querySelector(".app-tile[aria-label]")).toBeNull();
    await vi.waitFor(() => expect(line("chrono")).toBe("2h 05m on screen · 40% productive"));
    expect(line("palette")).toMatch(/^Try “.+”$/);
    expect(line("ledger")).toBe("1 due today · 1 overdue");
    // Lines refresh every 30 seconds, so they aren't read out each time.
    expect(document.querySelector("[aria-live]#app-tiles, #app-tiles [aria-live]")).toBeNull();
  });

  it("starts the keyboard on the first tile when nothing is due", async () => {
    api.tasks.mockResolvedValue([]);
    await open();
    await vi.waitFor(() => expect($("#today-list").textContent).toContain("Nothing due today"));
    expect(document.activeElement).toBe($('[data-app="chrono"] .app-open'));
  });

  it("leaves a line that hasn't changed as it is", async () => {
    await open();
    await vi.waitFor(() => expect(line("ledger")).toBe("1 due today · 1 overdue"));
    const shown = $('[data-line="ledger"]').firstChild;
    $<HTMLButtonElement>('[data-tab="settings"]').click();
    $<HTMLButtonElement>('[data-tab="home"]').click();
    await vi.waitFor(() => expect(api.tasks).toHaveBeenCalledTimes(2));
    await Promise.resolve();
    expect($('[data-line="ledger"]').firstChild).toBe(shown);
  });

  it("reads only today's totals for Chrono's line, not the whole day", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 0, 5, 9, 30));
    try {
      await open();
      await vi.waitFor(() => expect(api.dayTotals).toHaveBeenCalledWith("2026-01-05", "2026-01-05"));
      expect(api.day).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("says when nothing is tracked yet, or tracking is paused", async () => {
    api.dayTotals.mockResolvedValue([]);
    await open();
    await vi.waitFor(() => expect(line("chrono")).toBe("Nothing tracked yet today"));

    vi.resetModules();
    document.body.innerHTML = markup;
    api.status.mockResolvedValue({ tracking: false });
    await open();
    await vi.waitFor(() => expect(line("chrono")).toBe("Paused · Resume"));
    // Paused offers its way out, there on the tile.
    api.status.mockResolvedValue({ tracking: true });
    $<HTMLButtonElement>('[data-line="chrono"] .tile-resume').click();
    expect(api.resumeTracking).toHaveBeenCalled();
    await vi.waitFor(() => expect(line("chrono")).toBe("Nothing tracked yet today"));
  });

  it("says so when a tile's first line can't be read", async () => {
    api.dayTotals.mockRejectedValue("database busy");
    await open();
    await vi.waitFor(() => expect(line("chrono")).toBe("Couldn't read Chrono just now"));
  });

  it("keeps a line it can't refresh", async () => {
    await open();
    await vi.waitFor(() => expect(line("chrono")).toContain("on screen"));
    api.dayTotals.mockRejectedValue("database busy");
    $<HTMLButtonElement>('[data-tab="settings"]').click();
    $<HTMLButtonElement>('[data-tab="home"]').click();
    await vi.waitFor(() => expect(api.dayTotals).toHaveBeenCalledTimes(2));
    expect(line("chrono")).toBe("2h 05m on screen · 40% productive");
  });

  it("draws each app as its object", async () => {
    await open();
    // Chrono's dial: today's time in its middle, its ring shared by productive and the rest.
    await vi.waitFor(() => expect($('[data-app="chrono"] .dial-centre').textContent).toBe("2h 05"));
    expect($('[data-app="chrono"] .dial-arc.productive')).not.toBeNull();
    expect($('[data-app="ledger"] .app-object .logo-ledger')).not.toBeNull();
    expect([...document.querySelectorAll('[data-app="palette"] .keycap kbd')].map((k) => k.textContent)).toEqual(["Alt", "Space"]);
  });

  it("opens an app from its tile", async () => {
    await open();
    $<HTMLButtonElement>('[data-app="ledger"] .app-open').click();
    expect(api.openLedger).toHaveBeenCalled();
    $<HTMLButtonElement>('[data-app="chrono"] .app-open').click();
    expect(api.openChrono).toHaveBeenCalledWith();
    // The drawing is part of the tile's one button, not a second one.
    expect(document.querySelector("#app-tiles .app-object")!.getAttribute("aria-hidden")).toBe("true");
    // Tiles carry no settings of their own; those are on the Settings tab.
    expect(document.querySelector("#app-tiles .link-btn")).toBeNull();
  });

  it("suggests one thing to try in the palette, the same all day", async () => {
    const { tryToday } = await import("./dashboard");
    expect(tryToday(new Date(2026, 10, 5, 8))).toBe(tryToday(new Date(2026, 10, 5, 23)));
    expect(tryToday(new Date(2026, 10, 5))).not.toBe(tryToday(new Date(2026, 10, 6)));
  });
});

describe("Today", () => {
  const titles = () => [...document.querySelectorAll("#today-list .today-task-title")].map((t) => t.textContent);

  it("lists what's due, overdue marked, each opening in Ledger", async () => {
    api.tasks.mockResolvedValue([
      { id: "a", title: "Essay", group: "today", remindMs: null },
      { id: "b", title: "Bank", group: "overdue", remindMs: null },
      { id: "c", title: "Later", group: "upcoming", remindMs: null },
      { id: "d", title: "Old", group: "done", remindMs: null },
    ]);
    await open();
    await vi.waitFor(() => expect(titles()).toEqual(["Essay", "Bank"]));
    expect(document.querySelectorAll("#today-list .today-late").length).toBe(1);
    $<HTMLButtonElement>("#today-list .today-task").click();
    expect(api.openLedger).toHaveBeenCalledWith("task:a");
    // Ledger's tile and Today read the tasks once between them.
    expect(api.tasks).toHaveBeenCalledTimes(1);
  });

  it("ticks a task off from Home, and says where the task opens", async () => {
    api.tasks.mockResolvedValue([{ id: "a", title: "Essay", group: "today", remindMs: null }]);
    await open();
    await vi.waitFor(() => expect(titles()).toEqual(["Essay"]));
    // Opened on Home, the keyboard starts on today's first task, before the tiles.
    expect(document.activeElement).toBe($("#today-list .today-task"));
    expect($("#today-list .today-task").getAttribute("aria-label")).toBe("Open “Essay” in Ledger");
    api.tasks.mockResolvedValue([{ id: "a", title: "Essay", group: "done", remindMs: null }]);
    $<HTMLButtonElement>("#today-list .today-tick").click();
    await vi.waitFor(() => expect(api.setTaskDone).toHaveBeenCalledWith("a", true));
    await vi.waitFor(() => expect($("#today-list").textContent).toBe("Nothing due today. Add a task"));
  });

  it("lets the greeting say how the day stands", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 5, 18, 0));
    try {
      api.tasks.mockResolvedValue([{ id: "b", title: "Bank", group: "overdue", remindMs: null }]);
      await open();
      // Not "nearly done." at six with something overdue.
      await vi.waitFor(() => expect($("#dash-title em").textContent).toBe("1 overdue."));
    } finally {
      vi.useRealTimers();
    }
  });

  it("says when nothing is due, with a way to add something", async () => {
    api.tasks.mockResolvedValue([]);
    await open();
    await vi.waitFor(() => expect($("#today-list").textContent).toBe("Nothing due today. Add a task"));
    $<HTMLButtonElement>("#today-list .link-btn").click();
    expect(api.openLedger).toHaveBeenCalledWith("add");
  });

  it("names the next reminder still to come today", async () => {
    const soon = new Date();
    soon.setMinutes(soon.getMinutes() + 1);
    // Near midnight, a minute from now is tomorrow: nothing to say then.
    const today = soon.getDate() === new Date().getDate();
    api.tasks.mockResolvedValue([{ id: "a", title: "Call", group: "upcoming", remindMs: soon.getTime() }]);
    await open();
    await vi.waitFor(() => expect(api.tasks).toHaveBeenCalled());
    await vi.waitFor(() => expect($("#today-next").textContent).toBe(today ? `Next reminder at ${soon.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}: Call` : ""));
  });
});

describe("the app ring's tile", () => {
  it("shows its keys and how many are on it, and opens it", async () => {
    await open();
    await vi.waitFor(() => expect(line("ring")).toBe("AltQ · 4 items"));
    expect([...document.querySelectorAll('[data-line="ring"] kbd')].map((k) => k.textContent)).toEqual(["Alt", "Q"]);
    // Its picture: the arc, as many segments as show.
    expect(document.querySelectorAll('[data-app="ring"] .arc-seg').length).toBe(4);
    $<HTMLButtonElement>('[data-app="ring"] .app-open').click();
    expect(api.openRing).toHaveBeenCalled();
  });
});

describe("App settings", () => {
  const rows = () => [...document.querySelectorAll("#app-settings li")].map((li) => [li.querySelector("strong")!.textContent, li.querySelector("button")!.textContent]);

  it("lists the apps whose settings are on their own screen; the palette's and ring's are on this page", async () => {
    await open();
    expect(rows()).toEqual([
      ["Chrono", "Open in Chrono"],
      ["Ledger", "Open in Ledger"],
    ]);
  });

  it("opens each app on its settings", async () => {
    await open();
    $<HTMLButtonElement>('[data-settings="ledger"]').click();
    expect(api.openLedger).toHaveBeenCalledWith("settings");
    $<HTMLButtonElement>('[data-settings="chrono"]').click();
    expect(api.openChrono).toHaveBeenCalledWith("settings");
  });
});

describe("tabs", () => {
  it("opens on Home when nothing asks otherwise", async () => {
    await open();
    expect(onTab()).toBe("home");
    // The greeting for the hour, its aside in italic.
    expect($("#dash-title").textContent).toMatch(/^(Good (morning|afternoon|evening),|Still up\?) /);
    expect($("#dash-title em")).not.toBeNull();
  });

  it("opens on the tab it was asked for", async () => {
    await open("settings");
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
    expect($("#dash-title").textContent).toBe("Settings");
    expect($('[data-tab="settings"]').getAttribute("aria-selected")).toBe("true");
    await vi.waitFor(() => expect($("#settings-version").textContent).toBe("Kairo 1.19.3"));
  });

  it("switches with the arrows on the tabs, and Ctrl+1, Ctrl+2 or Ctrl+, anywhere", async () => {
    await open();
    const keydown = (target: EventTarget, key: string, ctrlKey = false) => target.dispatchEvent(new KeyboardEvent("keydown", { key, ctrlKey, bubbles: true, cancelable: true }));
    keydown($('[data-tab="home"]'), "ArrowRight");
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
    expect(document.activeElement).toBe($('[data-tab="settings"]'));
    expect($<HTMLButtonElement>('[data-tab="home"]').tabIndex).toBe(-1);
    keydown(document, "1", true);
    await vi.waitFor(() => expect(onTab()).toBe("home"));
    keydown(document, ",", true);
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
  });

  it("goes to a tab asked for later, and stays put when asked for none", async () => {
    await open();
    api.takeDashboardTab.mockResolvedValueOnce("settings");
    emit("kairo:dashboard");
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
    api.takeDashboardTab.mockResolvedValueOnce(null);
    emit("kairo:dashboard");
    await vi.waitFor(() => expect(api.takeDashboardTab).toHaveBeenCalledTimes(3));
    expect(onTab()).toBe("settings");
  });

  it("stops recording a shortcut when Settings is left", async () => {
    await open("settings");
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
    $("#palette-record").click();
    $<HTMLButtonElement>('[data-tab="home"]').click();
    const e = new KeyboardEvent("keydown", { key: "k", code: "KeyK", ctrlKey: true, cancelable: true });
    document.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(false);
    expect(api.setPaletteShortcut).not.toHaveBeenCalled();
    expect($("#palette-record").textContent).toBe("Change");
  });
});

describe("Jumping to a setting", () => {
  it("opens Settings with the asked-for control focused", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    await open("settings#palette-suggestions");
    await vi.waitFor(() => expect(document.activeElement).toBe($("#palette-suggestions")));
    expect(onTab()).toBe("settings");
    expect($("#palette-settings").classList.contains("jumped")).toBe(true);
  });
});
