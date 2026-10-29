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
vi.mock("./dropdown", () => ({ useDropdowns: () => {} }));

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
  document.body.innerHTML = markup;
});

describe("Home", () => {
  it("shows a tile for each app, with today's line", async () => {
    await open();
    expect([...document.querySelectorAll(".app-tile h2")].map((t) => t.textContent)).toEqual(["Chrono", "Command palette"]);
    await vi.waitFor(() => expect(line("chrono")).toBe("2h 05m on screen · 40% productive"));
    expect(line("palette")).toBe("AltSpace");
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
    await vi.waitFor(() => expect(line("chrono")).toBe("Paused"));
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

  it("opens an app from its tile", async () => {
    await open();
    $<HTMLButtonElement>('[data-app="chrono"] .btn').click();
    expect(api.openChrono).toHaveBeenCalledWith();
    $<HTMLButtonElement>('[data-app="chrono"] .link-btn').click();
    expect(api.openChrono).toHaveBeenCalledWith("settings");
  });
});

describe("tabs", () => {
  it("opens on Home when nothing asks otherwise", async () => {
    await open();
    expect(onTab()).toBe("home");
    expect($("#dash-title").textContent).toBe("Your apps");
  });

  it("opens on the tab it was asked for", async () => {
    await open("settings");
    await vi.waitFor(() => expect(onTab()).toBe("settings"));
    expect($("#dash-title").textContent).toBe("Settings");
    expect($('[data-tab="settings"]').getAttribute("aria-pressed")).toBe("true");
    await vi.waitFor(() => expect($("#settings-version").textContent).toBe("Kairo 1.19.3"));
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
