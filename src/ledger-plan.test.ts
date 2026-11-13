import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { dayAfter, isEvening, leadLine, mountPlan, planTitle, remindLater, remindTomorrow, toLookAt, type PlanActions } from "./ledger-plan";

const MIN = 60_000;
const at = (h: number, m = 0) => new Date(2026, 10, 12, h, m).getTime();

const task = (id: string, title: string, over: Partial<Task> = {}): Task => ({
  id,
  title,
  createdMs: 0,
  doneMs: null,
  position: 0,
  focusedMs: 0,
  dueDay: null,
  priority: 1,
  remindMs: null,
  reminded: "",
  note: null,
  steps: [],
  repeat: null,
  tags: [],
  project: null,
  remindWhen: null,
  estimateMin: null,
  group: "none",
  ...over,
});

describe("the hour and the days", () => {
  it("plans the day until five, and wraps it up from then", () => {
    expect([isEvening(at(9)), isEvening(at(16, 59)), isEvening(at(17)), isEvening(at(23))]).toEqual([false, false, true, true]);
    expect([planTitle(at(9)), planTitle(at(17))]).toEqual(["Plan the day", "Wrap up the day"]);
  });

  it("finds the next day, over a month and a year", () => {
    expect(dayAfter("2026-11-12")).toBe("2026-11-13");
    expect(dayAfter("2026-11-30")).toBe("2026-12-01");
    expect(dayAfter("2026-12-31")).toBe("2027-01-01");
  });

  it("moves a reminder a day on at the same time, and lets a past one go when put off", () => {
    expect(remindTomorrow(null)).toBeNull();
    expect(remindTomorrow(at(15, 30))).toBe(new Date(2026, 10, 13, 15, 30).getTime());
    expect(remindLater(null, at(10))).toBeNull();
    expect(remindLater(at(9), at(10))).toBeNull();
    expect(remindLater(at(15), at(10))).toBe(at(15));
  });
});

describe("what the day holds", () => {
  const tasks = [
    task("a", "Overdue one", { group: "overdue", estimateMin: 60 }),
    task("b", "Today one", { group: "today", estimateMin: 90 }),
    task("c", "Later one", { group: "upcoming", estimateMin: 30 }),
    task("d", "Done one", { group: "done", doneMs: at(9), focusedMs: 40 * MIN }),
    task("e", "No day"),
  ];

  it("looks at the open tasks due today or overdue", () => {
    expect(toLookAt(tasks).map((t) => t.id)).toEqual(["a", "b"]);
  });

  it("opens the morning with how many, and how long they take against a usual day", () => {
    expect(leadLine(tasks, at(9), null)).toBe("2 to do · 2h 30m planned today");
    expect(leadLine(tasks, at(9), 3 * 60 * MIN)).toBe("2 to do · 2h 30m planned today");
    expect(leadLine(tasks, at(9), 2 * 60 * MIN)).toBe("2 to do · 2h 30m planned today, more than the 2h you usually focus");
    expect(leadLine([task("e", "x")], at(9), null)).toBe("Nothing due today.");
    expect(leadLine([task("a", "No estimate", { group: "today" })], at(9), null)).toBe("1 to do");
  });

  it("opens the evening with what was done and what is left", () => {
    expect(leadLine(tasks, at(18), null)).toBe("1 done · 40m focused · 2 left");
    expect(leadLine([tasks[3]], at(18), null)).toBe("1 done · 40m focused · everything due is done");
    expect(leadLine([task("a", "x", { group: "today" })], at(18), null)).toBe("Nothing done today · 1 left");
    expect(leadLine([], at(18), null)).toBe("Nothing done today");
  });
});

describe("the Plan view", () => {
  let root: HTMLElement;
  let act: { [K in keyof PlanActions]: ReturnType<typeof vi.fn> };

  const rows = () => [...root.querySelectorAll<HTMLElement>(".plan-list .plan-row")];
  const button = (id: string, cls: string) => root.querySelector<HTMLButtonElement>(`[data-task="${id}"] ${cls}`)!;

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(at(10));
    document.body.innerHTML = `<div id="root"></div>`;
    root = document.getElementById("root")!;
    act = { move: vi.fn(), done: vi.fn(), openTask: vi.fn() };
  });

  afterEach(() => vi.useRealTimers());

  const tasks = [
    task("a", "Overdue one", { group: "overdue", dueDay: "2026-11-10", estimateMin: 60, focusedMs: 20 * MIN, remindMs: at(15) }),
    task("b", "Today one", { group: "today", dueDay: "2026-11-12", priority: 2 }),
    task("c", "Next week", { group: "upcoming", dueDay: "2026-11-19" }),
  ];

  it("lists only what is due today or overdue, with its cost and how late it is", () => {
    mountPlan(root, act, () => null).render(tasks);
    expect(root.querySelector(".plan-head")!.textContent).toBe("To do today");
    expect(rows().map((r) => r.dataset.task)).toEqual(["a", "b"]);
    const first = rows()[0];
    expect(first.querySelector(".plan-late")!.textContent).toBe("Overdue");
    expect(first.querySelector(".plan-cost")!.textContent).toBe("20m of ~1h");
    expect(rows()[1].querySelector(".plan-late")).toBeNull();
    expect(root.querySelector(".plan-lead")!.textContent).toBe("2 to do · 1h planned today");
  });

  it("moves a task to tomorrow with its reminder, or puts it off with no day", () => {
    mountPlan(root, act, () => null).render(tasks);
    button("a", ".plan-tomorrow").click();
    expect(act.move).toHaveBeenLastCalledWith("a", "2026-11-13", new Date(2026, 10, 13, 15, 0).getTime());
    button("b", ".plan-later").click();
    expect(act.move).toHaveBeenLastCalledWith("b", null, null);
    // A reminder still to come stays when it's put off.
    button("a", ".plan-later").click();
    expect(act.move).toHaveBeenLastCalledWith("a", null, at(15));
  });

  it("ticks a task off, and opens one", () => {
    mountPlan(root, act, () => null).render(tasks);
    button("b", ".plan-tick").click();
    expect(act.done).toHaveBeenCalledWith("b", true);
    button("b", ".plan-title").click();
    expect(act.openTask).toHaveBeenCalledWith("b");
  });

  it("moves them all to tomorrow on a second click, and only offers it for more than one", () => {
    const view = mountPlan(root, act, () => null);
    view.render(tasks);
    const all = root.querySelector<HTMLButtonElement>(".plan-all")!;
    all.click();
    expect(act.move).not.toHaveBeenCalled();
    expect(all.textContent).toBe("Click again to move them all");
    all.click();
    expect(act.move.mock.calls.map((c) => [c[0], c[1]])).toEqual([
      ["a", "2026-11-13"],
      ["b", "2026-11-13"],
    ]);
    view.render([tasks[1]]);
    expect(root.querySelector(".plan-all")).toBeNull();
  });

  it("says what tomorrow already holds", () => {
    mountPlan(root, act, () => null).render([...tasks, task("t1", "Tomorrow one", { group: "upcoming", dueDay: "2026-11-13", estimateMin: 90 }), task("t2", "Tomorrow two", { group: "upcoming", dueDay: "2026-11-13" })]);
    expect(root.querySelector(".plan-tomorrow-line")!.textContent).toBe("Tomorrow already has 2 tasks · 1h 30m planned.");
    mountPlan(root, act, () => null).render(tasks);
    expect(root.querySelector(".plan-tomorrow-line")).toBeNull();
  });

  it("wraps the day up in the evening: what is left over, and what was done today", () => {
    vi.setSystemTime(at(18));
    const done = task("d", "Finished", { group: "done", doneMs: at(11), focusedMs: 50 * MIN });
    const old = task("o", "Done long ago", { group: "done", doneMs: new Date(2026, 10, 1).getTime() });
    mountPlan(root, act, () => null).render([...tasks, done, old]);
    expect(root.querySelector(".plan-lead")!.textContent).toBe("1 done · 50m focused · 2 left");
    expect([...root.querySelectorAll(".plan-head")].map((h) => h.textContent)).toEqual(["Left over", "Done today"]);
    const finished = [...root.querySelectorAll(".plan-done-list .plan-row")];
    expect(finished.map((r) => r.querySelector(".plan-name")!.textContent)).toEqual(["Finished"]);
    expect(finished[0].querySelector(".plan-cost")!.textContent).toBe("50m");
    expect(root.querySelector(".plan-focused")!.textContent).toBe("50m focused on them.");
  });

  it("says when nothing is due, and keeps the keyboard in the list as rows go", () => {
    const view = mountPlan(root, act, () => null);
    view.render([task("x", "Nothing due", { group: "upcoming" })]);
    expect(root.querySelector(".plan-empty")!.textContent).toBe("Nothing due today or overdue.");
    expect(root.querySelector(".plan-list")).toBeNull();

    view.render(tasks.slice(0, 2));
    button("a", ".plan-tomorrow").click();
    // The first is gone: the keyboard goes to the one that took its place.
    view.render([tasks[1]]);
    expect(document.activeElement).toBe(button("b", ".plan-title"));
  });
});
