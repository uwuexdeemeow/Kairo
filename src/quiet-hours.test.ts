import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  pauseSchedule: vi.fn(),
  addPauseWindow: vi.fn(),
  removePauseWindow: vi.fn(),
}));
vi.mock("./api", () => ({ api }));

import type { PauseWindow } from "./api";
import { loadQuietHours, renderQuietHours, setUpQuietForm, spanText } from "./quiet-hours";
import { daysOf, daysText } from "./week-text";

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;

const nights: PauseWindow = { id: 1, days: 0x7f, startMin: 22 * 60, minutes: 9 * 60 };
const weekend: PauseWindow = { id: 2, days: 0x60, startMin: 9 * 60, minutes: 240 };

beforeEach(() => {
  document.body.innerHTML = `<div id="quiet-list"></div><div id="quiet-days"></div><input id="quiet-start" value="22:00" /><select id="quiet-length"></select><button id="quiet-add"></button><span id="quiet-status"></span>`;
  for (const fn of Object.values(api)) fn.mockReset();
});

describe("Quiet hours in Chrono's settings", () => {
  it("says each window as its span, its days and how long", () => {
    renderQuietHours([nights, weekend]);
    const rows = [...document.querySelectorAll(".schedule-row")].map((r) => r.textContent);
    // A night that passes midnight ends on the next day's clock.
    expect(rows[0]).toContain("22:00–07:00");
    expect(rows[0]).toContain("Every day");
    expect(rows[1]).toContain("09:00–13:00");
    expect(rows[1]).toContain("Weekends");
    expect(spanText(nights)).toBe("22:00–07:00");
    expect(daysText(0x1f)).toBe("Weekdays");
    expect(daysText(0b0000101)).toBe("Mon, Wed");
  });

  it("reads the windows when settings open", async () => {
    api.pauseSchedule.mockResolvedValue([weekend]);
    await loadQuietHours();
    expect(document.querySelectorAll(".schedule-row").length).toBe(1);
  });

  it("adds a window from the form: every day to start, eight hours, saved as minutes", async () => {
    setUpQuietForm();
    expect([...$("#quiet-days").children].every((b) => b.getAttribute("aria-pressed") === "true")).toBe(true);
    expect($<HTMLSelectElement>("#quiet-length").value).toBe("480");
    // Weekends only, from 23:30.
    const days = [...$("#quiet-days").children] as HTMLButtonElement[];
    for (const b of days.slice(0, 5)) b.click();
    expect(daysOf($("#quiet-days"))).toBe(0x60);
    $<HTMLInputElement>("#quiet-start").value = "23:30";
    api.addPauseWindow.mockResolvedValueOnce([{ id: 3, days: 0x60, startMin: 23 * 60 + 30, minutes: 480 }]);
    $<HTMLButtonElement>("#quiet-add").click();
    expect(api.addPauseWindow).toHaveBeenCalledWith(0x60, 23 * 60 + 30, 480);
    await vi.waitFor(() => expect(document.querySelectorAll(".schedule-row").length).toBe(1));
  });

  it("says why a window can't be added", async () => {
    setUpQuietForm();
    api.addPauseWindow.mockRejectedValueOnce("Pick at least one day.");
    $<HTMLButtonElement>("#quiet-add").click();
    await vi.waitFor(() => expect($("#quiet-status").textContent).toBe("Pick at least one day."));
  });

  it("takes a window away on the second press", async () => {
    api.removePauseWindow.mockResolvedValueOnce([]);
    renderQuietHours([nights]);
    const remove = $<HTMLButtonElement>(".schedule-row .link-btn");
    expect(remove.getAttribute("aria-label")).toBe("Remove Every day 22:00–07:00");
    remove.click();
    expect(api.removePauseWindow).not.toHaveBeenCalled();
    remove.click();
    expect(api.removePauseWindow).toHaveBeenCalledWith(1);
    await vi.waitFor(() => expect(document.querySelectorAll(".schedule-row").length).toBe(0));
  });
});
