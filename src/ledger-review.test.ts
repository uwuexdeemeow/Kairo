import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { MAX_PICKS, mountReview, nextWeekDays, nextWorkday, pickable, plannedNext, reviewedWeek, slipLine, slippedTasks } from "./ledger-review";

const task = (id: string, extra: Partial<Task> = {}): Task =>
  ({ id, title: id, createdMs: 0, doneMs: null, position: 0, focusedMs: 0, dueDay: null, priority: 1, remindMs: null, reminded: "", note: null, steps: [], repeat: null, tags: [], project: null, remindWhen: null, estimateMin: null, moved: 0, group: "none", ...extra }) as Task;

const at = (iso: string, h = 12) => new Date(`${iso}T${String(h).padStart(2, "0")}:00:00`).getTime();

describe("the week under review", () => {
  it("is the week going on, and on Monday morning the one that just ended", () => {
    // 2026-11-04 is a Wednesday, 11-08 a Sunday, 11-09 a Monday.
    expect(reviewedWeek(at("2026-11-04"))).toBe("2026-11-02");
    expect(reviewedWeek(at("2026-11-08", 20))).toBe("2026-11-02");
    expect(reviewedWeek(at("2026-11-09", 9))).toBe("2026-11-02");
    expect(reviewedWeek(at("2026-11-09", 17))).toBe("2026-11-09");
  });
});

describe("what slipped", () => {
  it("is open tasks that are overdue or were moved twice or more, the most moved first", () => {
    const tasks = [
      task("fine", { group: "upcoming", moved: 1 }),
      task("late", { group: "overdue", dueDay: "2026-11-02" }),
      task("often", { group: "upcoming", moved: 3 }),
      task("both", { group: "overdue", moved: 2 }),
      task("done", { group: "done", doneMs: 5, moved: 4 }),
    ];
    expect(slippedTasks(tasks).map((t) => t.id)).toEqual(["often", "both", "late"]);
  });

  it("says why, in the page's plain words", () => {
    expect(slipLine(task("a", { group: "overdue" }))).toBe("Overdue");
    expect(slipLine(task("a", { moved: 3 }))).toBe("Moved 3 times");
    expect(slipLine(task("a", { group: "overdue", moved: 2 }))).toBe("Overdue · moved 2 times");
  });
});

describe("next week", () => {
  it("is Monday to Friday after the week reviewed", () => {
    expect(nextWeekDays("2026-11-02")).toEqual(["2026-11-09", "2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13"]);
  });

  it("holds the open tasks already due in it, Monday to Sunday", () => {
    const tasks = [task("mon", { dueDay: "2026-11-09" }), task("sun", { dueDay: "2026-11-15" }), task("later", { dueDay: "2026-11-16" }), task("this", { dueDay: "2026-11-08" }), task("done", { dueDay: "2026-11-10", doneMs: 1, group: "done" })];
    expect(plannedNext(tasks, "2026-11-02").map((t) => t.id)).toEqual(["mon", "sun"]);
  });

  it("offers open tasks not already in it, found by name, a few at a time, not those picked", () => {
    const tasks = [task("essay"), task("taxes", { dueDay: "2026-11-10" }), task("tidy desk"), task("done", { doneMs: 1, group: "done" }), ...Array.from({ length: 12 }, (_, i) => task(`chore ${i}`))];
    const all = pickable(tasks, "2026-11-02", "", []).map((t) => t.id);
    expect(all).toHaveLength(8);
    expect(all).not.toContain("taxes");
    expect(all).not.toContain("done");
    expect(pickable(tasks, "2026-11-02", "ESS", []).map((t) => t.id)).toEqual(["essay"]);
    expect(pickable(tasks, "2026-11-02", "essay", ["essay"])).toEqual([]);
  });

  it("allows three picks", () => {
    expect(MAX_PICKS).toBe(3);
  });
});

describe("Do it", () => {
  it("puts a task on the next working day: tomorrow Monday to Thursday, Monday after that", () => {
    expect(nextWorkday("2026-11-04")).toBe("2026-11-05");
    expect(nextWorkday("2026-11-05")).toBe("2026-11-06");
    expect(nextWorkday("2026-11-06")).toBe("2026-11-09");
    expect(nextWorkday("2026-11-07")).toBe("2026-11-09");
    expect(nextWorkday("2026-11-08")).toBe("2026-11-09");
  });
});

describe("the Review view", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Sunday evening, 8 November 2026: the week of Monday the 2nd is ending.
    vi.setSystemTime(new Date("2026-11-08T20:00:00"));
  });
  afterEach(() => {
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  function mount(tasks: Task[], reviewed: string | null = null) {
    const calls: unknown[][] = [];
    const root = document.createElement("div");
    document.body.append(root);
    const view = mountReview(root, {
      move: (...a) => calls.push(["move", ...a]),
      remove: (...a) => calls.push(["remove", ...a]),
      openTask: (...a) => calls.push(["open", ...a]),
      chrono: (...a) => calls.push(["chrono", ...a]),
      finish: async (...a) => void calls.push(["finish", ...a]),
    });
    view.render(tasks, reviewed);
    const click = (label: string) => (Array.from(root.querySelectorAll("button")).find((b) => b.getAttribute("aria-label") === label || b.textContent === label) as HTMLButtonElement).click();
    return { root, calls, click, view };
  }

  const week = [task("Taxes", { group: "overdue", dueDay: "2026-11-02", moved: 3 }), task("Essay"), task("Mail"), task("Gym"), task("Read"), task("Sent", { doneMs: at("2026-11-04"), group: "done" })];

  it("counts the week, lists what slipped and offers the actions", () => {
    const { root, calls, click } = mount(week);
    expect(root.textContent).toContain("1 done");
    expect(root.querySelector(".review-why")?.textContent).toBe("Overdue · moved 3 times");
    click("Open this week in Chrono");
    click("Put “Taxes” on Mon 9");
    click("Take the day off “Taxes”");
    click("Delete “Taxes”");
    expect(calls).toEqual([["chrono", "2026-11-02"], ["move", "Taxes", "2026-11-09", null], ["move", "Taxes", null, null], ["remove", "Taxes"]]);
  });

  it("lets three tasks be picked for next week, then says so", () => {
    const { root, calls, click, view } = mount(week);
    const picked: string[] = [];
    for (const [title, day] of [["Essay", "Mon 9"], ["Mail", "Tue 10"], ["Gym", "Wed 11"]] as const) {
      click(`Put “${title}” on ${day}`);
      picked.push(title);
      // The page draws again once the task has its day.
      view.render(week.map((t) => (picked.includes(t.id) ? { ...t, dueDay: "2026-11-09" } : t)), null);
    }
    expect(calls.map((c) => c[1])).toEqual(["Essay", "Mail", "Gym"]);
    expect(root.textContent).toContain("Three picked");
    expect(root.querySelector(".review-search")).toBeNull();
  });

  it("marks the week reviewed once, and then says what next week holds", () => {
    const { root, calls, click, view } = mount(week);
    click("Mark this week reviewed");
    expect(calls[calls.length - 1]).toEqual(["finish", "2026-11-02"]);
    view.render(week, "2026-11-02");
    expect(root.textContent).toContain("Reviewed");
    expect(root.textContent).toContain("Next week is open");
    expect(Array.from(root.querySelectorAll("button")).some((b) => b.textContent === "Done reviewing")).toBe(false);
  });

  it("keeps the keyboard in the search field while the list is redrawn", () => {
    const { root } = mount(week);
    const field = root.querySelector<HTMLInputElement>(".review-search")!;
    field.focus();
    field.value = "es";
    field.dispatchEvent(new Event("input"));
    expect(document.activeElement).toBe(field);
    const lists = root.querySelectorAll(".review-list");
    expect(lists[lists.length - 1].textContent).toContain("Essay");
  });
});
