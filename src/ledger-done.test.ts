import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { byDay, dayName, mountDone, totals, totalsLine, weekStart } from "./ledger-done";

const at = (y: number, m: number, d: number, hh = 12, mm = 0) => new Date(y, m - 1, d, hh, mm).getTime();
// Thursday 12 November 2026.
const NOW = at(2026, 11, 12, 15);

const task = (id: string, title: string, doneMs: number | null, focusedMs = 0): Task => ({
  id,
  title,
  createdMs: 0,
  doneMs,
  position: 0,
  focusedMs,
  dueDay: null,
  priority: 1,
  remindMs: null,
  reminded: "",
  note: null,
  steps: [],
  repeat: null,
  tags: [],
  project: null,
  group: doneMs === null ? "none" : "done",
});

describe("weeks and days", () => {
  it("a week starts on Monday at midnight", () => {
    expect(weekStart(NOW)).toBe(at(2026, 11, 9, 0));
    expect(weekStart(at(2026, 11, 9, 0))).toBe(at(2026, 11, 9, 0));
    expect(weekStart(at(2026, 11, 15, 23, 59))).toBe(at(2026, 11, 9, 0));
    expect(weekStart(at(2026, 11, 16, 0, 1))).toBe(at(2026, 11, 16, 0));
  });

  it("names a day for today, yesterday, then its date, with the year once it's another", () => {
    expect(dayName(at(2026, 11, 12, 0), NOW)).toBe("Today");
    expect(dayName(at(2026, 11, 11, 0), NOW)).toBe("Yesterday");
    expect(dayName(at(2026, 11, 9, 0), NOW)).toMatch(/9/);
    expect(dayName(at(2026, 11, 9, 0), NOW)).not.toMatch(/2026/);
    expect(dayName(at(2025, 12, 30, 0), NOW)).toMatch(/2025/);
  });

  it("lists done tasks newest first under their day, leaving out open ones and old ones", () => {
    const tasks = [
      task("a", "A", at(2026, 11, 12, 9)),
      task("b", "B", at(2026, 11, 12, 14)),
      task("c", "C", at(2026, 11, 10, 8)),
      task("open", "Open", null),
      task("old", "Old", at(2026, 9, 1)),
    ];
    const days = byDay(tasks, at(2026, 10, 1));
    expect(days.map((d) => d.tasks.map((t) => t.id))).toEqual([["b", "a"], ["c"]]);
    expect(days[0].start).toBe(at(2026, 11, 12, 0));
  });
});

describe("totals", () => {
  const tasks = [
    task("a", "A", at(2026, 11, 10), 30 * 60_000),
    task("b", "B", at(2026, 11, 12), 50 * 60_000),
    task("c", "C", at(2026, 11, 4), 25 * 60_000),
    task("open", "Open", null, 99 * 60_000),
  ];

  it("adds up the done tasks in a range and the focus time on them", () => {
    expect(totals(tasks, weekStart(NOW), Infinity)).toEqual({ done: 2, focusedMs: 80 * 60_000 });
    expect(totals(tasks, at(2026, 11, 2, 0), at(2026, 11, 9, 0))).toEqual({ done: 1, focusedMs: 25 * 60_000 });
  });

  it("says the focus time only when there was some", () => {
    expect(totalsLine({ done: 2, focusedMs: 80 * 60_000 })).toBe("2 done · 1h 20m focused");
    expect(totalsLine({ done: 1, focusedMs: 0 })).toBe("1 done");
    expect(totalsLine({ done: 0, focusedMs: 0 })).toBe("0 done");
  });
});

describe("the Done view", () => {
  let root: HTMLElement;
  let act: { reopen: ReturnType<typeof vi.fn>; remove: ReturnType<typeof vi.fn>; openTask: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    document.body.innerHTML = `<div id="root"></div>`;
    root = document.getElementById("root")!;
    act = { reopen: vi.fn(), remove: vi.fn(), openTask: vi.fn() };
  });

  it("says so when nothing is done", () => {
    mountDone(root, act, () => NOW).render([task("open", "Open", null)]);
    expect(root.textContent).toContain("Nothing done yet");
  });

  it("shows this week against last, the days and the rows", () => {
    const view = mountDone(root, act, () => NOW);
    view.render([task("a", "Write essay", at(2026, 11, 12, 9), 90 * 60_000), task("b", "Pay rent", at(2026, 11, 5, 10))]);
    expect(root.querySelector(".done-week")?.textContent).toContain("This week 1 done · 1h 30m focused");
    expect(root.querySelector(".done-week")?.textContent).toContain("last week 1 done");
    const days = [...root.querySelectorAll(".done-day")].map((d) => d.textContent);
    expect(days[0]).toBe("Today");
    expect(days).toHaveLength(2);
    expect(root.querySelectorAll(".done-row")).toHaveLength(2);
  });

  it("reopens, deletes and opens a task from its row", () => {
    mountDone(root, act, () => NOW).render([task("a", "Write essay", at(2026, 11, 12, 9))]);
    root.querySelector<HTMLButtonElement>(".done-reopen")!.click();
    root.querySelector<HTMLButtonElement>(".done-delete")!.click();
    root.querySelector<HTMLButtonElement>(".done-title")!.click();
    expect(act.reopen).toHaveBeenCalledWith("a");
    expect(act.remove).toHaveBeenCalledWith("a");
    expect(act.openTask).toHaveBeenCalledWith("a");
  });

  it("shows four weeks, and four more on Show older", () => {
    const tasks = [task("new", "New", at(2026, 11, 12)), task("mid", "Mid", at(2026, 10, 5)), task("old", "Old", at(2026, 9, 1))];
    mountDone(root, act, () => NOW).render(tasks);
    expect([...root.querySelectorAll(".done-row")].map((r) => (r as HTMLElement).dataset.task)).toEqual(["new"]);
    root.querySelector<HTMLButtonElement>(".done-older")!.click();
    expect([...root.querySelectorAll(".done-row")].map((r) => (r as HTMLElement).dataset.task)).toEqual(["new", "mid"]);
    root.querySelector<HTMLButtonElement>(".done-older")!.click();
    expect(root.querySelectorAll(".done-row")).toHaveLength(3);
    expect(root.querySelector(".done-older")).toBeNull();
  });

  it("finds by title", () => {
    const view = mountDone(root, act, () => NOW);
    view.render([task("a", "Write essay", at(2026, 11, 12)), task("b", "Pay rent", at(2026, 11, 12))]);
    const search = root.querySelector<HTMLInputElement>(".done-search")!;
    search.value = "rent";
    search.dispatchEvent(new Event("input"));
    expect(root.querySelectorAll(".done-row")).toHaveLength(1);
    search.value = "zzz";
    search.dispatchEvent(new Event("input"));
    expect(root.textContent).toContain("No done task matches");
  });
});
