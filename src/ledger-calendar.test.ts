import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CalendarEvent, Task } from "./api";
import { monthGrid, mountCalendar, openByDay, weekStartOf } from "./ledger-calendar";

// Thursday 12 November 2026.
const TODAY = "2026-11-12";
const at = (y: number, m: number, d: number, hh = 10, mm = 0) => new Date(y, m - 1, d, hh, mm).getTime();

const task = (id: string, dueDay: string | null, extra: Partial<Task> = {}): Task => ({
  id,
  title: id,
  createdMs: 0,
  doneMs: null,
  position: 0,
  focusedMs: 0,
  dueDay,
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
  group: dueDay ? "upcoming" : "none",
  ...extra,
});

describe("the month grid", () => {
  it("starts on the Monday on or before the 1st and ends on a Sunday", () => {
    // 1 Nov 2026 is a Sunday; 30 Nov a Monday.
    const days = monthGrid(2026, 10);
    expect(days[0]).toBe("2026-10-26");
    expect(days[days.length - 1]).toBe("2026-12-06");
    expect(days.length % 7).toBe(0);
    expect(days).toHaveLength(42);
  });

  it("is four weeks for a February that fits, and always whole weeks", () => {
    // February 2027 starts on a Monday and has 28 days.
    expect(monthGrid(2027, 1)).toHaveLength(28);
    for (let m = 0; m < 12; m++) expect(monthGrid(2026, m).length % 7).toBe(0);
  });

  it("finds a day's Monday", () => {
    expect(weekStartOf("2026-11-12")).toBe("2026-11-09");
    expect(weekStartOf("2026-11-09")).toBe("2026-11-09");
    expect(weekStartOf("2026-11-15")).toBe("2026-11-09");
  });

  it("groups the open dated tasks by day and leaves the rest", () => {
    const by = openByDay([task("a", TODAY), task("b", TODAY), task("c", "2026-11-13"), task("d", TODAY, { doneMs: 1 }), task("e", null)]);
    expect([...by.keys()]).toEqual([TODAY, "2026-11-13"]);
    expect(by.get(TODAY)!.map((t) => t.id)).toEqual(["a", "b"]);
  });
});

describe("the Calendar view", () => {
  let root: HTMLElement;
  let deps: { events: ReturnType<typeof vi.fn>; openTask: ReturnType<typeof vi.fn>; done: ReturnType<typeof vi.fn>; add: ReturnType<typeof vi.fn> };
  const mount = (events: CalendarEvent[] = [], tasks: Task[] = []) => {
    deps.events.mockResolvedValue(events);
    const view = mountCalendar(root, deps, () => TODAY);
    view.render(tasks);
    return view;
  };
  const cell = (iso: string) => root.querySelector<HTMLButtonElement>(`[data-day="${iso}"]`)!;
  const key = (el: HTMLElement, k: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));

  beforeEach(() => {
    document.body.innerHTML = `<div id="root"></div>`;
    root = document.getElementById("root")!;
    deps = { events: vi.fn(async () => []), openTask: vi.fn(), done: vi.fn(), add: vi.fn(async () => {}) };
  });

  it("opens on this month with today marked and chosen", () => {
    mount();
    expect(root.querySelector(".cal-title")?.textContent).toMatch(/November/);
    expect(cell(TODAY).classList.contains("today")).toBe(true);
    expect(cell(TODAY).getAttribute("aria-selected")).toBe("true");
    expect(root.querySelector(".cal-day-title")?.textContent).toMatch(/12/);
    expect(root.querySelectorAll(".cal-name")).toHaveLength(7);
  });

  it("shows each day's open tasks, two a cell and the rest as +n more, marking late ones", () => {
    mount([], [task("a", TODAY), task("b", TODAY), task("c", TODAY), task("old", "2026-11-03"), task("done", TODAY, { doneMs: 1 })]);
    expect([...cell(TODAY).querySelectorAll(".cal-task")].map((t) => t.textContent)).toEqual(["a", "b"]);
    expect(cell(TODAY).querySelector(".cal-more")?.textContent).toBe("+1 more");
    expect(cell("2026-11-03").classList.contains("late")).toBe(true);
    expect(cell(TODAY).classList.contains("late")).toBe(false);
    // Spoken as one label, not the cell's pieces.
    expect(cell(TODAY).getAttribute("aria-label")).toMatch(/3 tasks/);
  });

  it("lists the chosen day's tasks, done ones last and struck, and acts on them", () => {
    mount([], [task("b", TODAY, { priority: 1 }), task("a", TODAY, { priority: 2, tags: ["x"], project: "Kairo" }), task("z", TODAY, { doneMs: 5 })]);
    const rows = [...root.querySelectorAll<HTMLElement>(".cal-task-row")];
    expect(rows.map((r) => r.querySelector(".cal-task-name")?.textContent)).toEqual(["a", "b", "z"]);
    expect(rows[0].querySelector(".cal-task-meta")?.textContent).toBe("Kairo · #x");
    expect(rows[2].querySelector(".cal-task-name")?.classList.contains("done")).toBe(true);
    const tick = rows[0].querySelector<HTMLInputElement>("input")!;
    tick.checked = true;
    tick.dispatchEvent(new Event("change"));
    expect(deps.done).toHaveBeenCalledWith("a", true);
    rows[2].querySelector<HTMLInputElement>("input")!.dispatchEvent(new Event("change"));
    expect(deps.done).toHaveBeenLastCalledWith("z", false);
    rows[1].querySelector<HTMLButtonElement>(".cal-task-name")!.click();
    expect(deps.openTask).toHaveBeenCalledWith("b");
  });

  it("chooses a day on a click and says when it has nothing", () => {
    mount([], [task("a", TODAY)]);
    cell("2026-11-20").click();
    expect(cell("2026-11-20").getAttribute("aria-selected")).toBe("true");
    expect(cell(TODAY).getAttribute("aria-selected")).toBe("false");
    expect(root.querySelector(".cal-empty")?.textContent).toBe("No tasks due this day.");
  });

  it("moves between days with the arrows, a week with up and down, and into the next month", () => {
    mount();
    cell(TODAY).focus();
    key(cell(TODAY), "ArrowRight");
    expect(cell("2026-11-13").getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(cell("2026-11-13"));
    key(cell("2026-11-13"), "ArrowDown");
    expect(cell("2026-11-20").getAttribute("aria-selected")).toBe("true");
    key(cell("2026-11-20"), "ArrowUp");
    key(cell("2026-11-13"), "ArrowLeft");
    expect(cell(TODAY).getAttribute("aria-selected")).toBe("true");
    // Past the month's end the view goes on to the next.
    for (let i = 0; i < 19; i++) key(root.querySelector<HTMLElement>('[aria-selected="true"]')!, "ArrowRight");
    expect(root.querySelector(".cal-title")?.textContent).toMatch(/December/);
    expect(cell("2026-12-01").getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(cell("2026-12-01"));
  });

  it("goes back and forward a month, and Today comes back", () => {
    mount();
    root.querySelector<HTMLButtonElement>('[aria-label="Next month"]')!.click();
    expect(root.querySelector(".cal-title")?.textContent).toMatch(/December/);
    root.querySelector<HTMLButtonElement>('[aria-label="Previous month"]')!.click();
    root.querySelector<HTMLButtonElement>('[aria-label="Previous month"]')!.click();
    expect(root.querySelector(".cal-title")?.textContent).toMatch(/October/);
    root.querySelector<HTMLButtonElement>(".cal-today")!.click();
    expect(root.querySelector(".cal-title")?.textContent).toMatch(/November/);
    expect(cell(TODAY).getAttribute("aria-selected")).toBe("true");
  });

  it("PageUp and PageDown keep the day of the month, or its last", () => {
    mount();
    const selected = () => root.querySelector<HTMLElement>('[aria-selected="true"]')!;
    cell("2026-11-30").click();
    key(selected(), "PageDown");
    expect(selected().dataset.day).toBe("2026-12-30");
    key(selected(), "PageDown");
    expect(selected().dataset.day).toBe("2027-01-30");
    // February is shorter: its last day.
    key(selected(), "PageDown");
    expect(selected().dataset.day).toBe("2027-02-28");
    key(selected(), "PageUp");
    expect(selected().dataset.day).toBe("2027-01-28");
    cell("2027-01-28").click();
    for (let i = 0; i < 3; i++) key(selected(), "PageUp");
    expect(selected().dataset.day).toBe("2026-10-28");
  });

  it("asks for the meetings of the days the grid shows, and shows them", async () => {
    const meeting: CalendarEvent = { startMs: at(2026, 11, 12, 14), endMs: at(2026, 11, 12, 15), title: "Standup" };
    mount([meeting, { startMs: at(2026, 11, 12, 16), endMs: at(2026, 11, 12, 17), title: "Review" }]);
    await vi.waitFor(() => expect(cell(TODAY).querySelector(".cal-meet")?.textContent).toBe("2 meetings"));
    const [from, to] = deps.events.mock.calls[0];
    expect(from).toBe(at(2026, 10, 26, 0));
    expect(to).toBe(at(2026, 12, 7, 0));
    expect([...root.querySelectorAll(".cal-meetings li")].map((li) => li.textContent)).toEqual([expect.stringContaining("Standup"), expect.stringContaining("Review")]);
    expect(cell(TODAY).getAttribute("aria-label")).toMatch(/2 meetings/);
  });

  it("carries on with no meetings when the calendar can't be read", async () => {
    deps.events.mockRejectedValue(new Error("no"));
    const view = mountCalendar(root, deps, () => TODAY);
    view.render([task("a", TODAY)]);
    await Promise.resolve();
    expect(cell(TODAY).querySelector(".cal-task")?.textContent).toBe("a");
    expect(root.querySelector(".cal-meet")).toBeNull();
  });

  it("adds a task for the chosen day, and keeps the text if it can't", async () => {
    mount();
    cell("2026-11-18").click();
    const field = root.querySelector<HTMLInputElement>(".cal-add")!;
    field.value = "Call Sam !h";
    key(field, "Enter");
    expect(deps.add).toHaveBeenCalledWith("Call Sam !h", "2026-11-18");
    await Promise.resolve();
    expect(field.value).toBe("");
    deps.add.mockRejectedValueOnce(new Error("busy"));
    field.value = "Lost";
    key(field, "Enter");
    await vi.waitFor(() => expect(field.value).toBe("Lost"));
    field.value = "  ";
    key(field, "Enter");
    expect(deps.add).toHaveBeenCalledTimes(2);
  });

  it("leaves what's typed in the add field when the list refreshes", () => {
    const view = mount();
    const field = root.querySelector<HTMLInputElement>(".cal-add")!;
    field.focus();
    field.value = "half a thought";
    view.render([task("a", TODAY)]);
    const again = root.querySelector<HTMLInputElement>(".cal-add")!;
    expect(again.value).toBe("half a thought");
    expect(document.activeElement).toBe(again);
    // Another day starts empty.
    cell("2026-11-20").click();
    expect(root.querySelector<HTMLInputElement>(".cal-add")!.value).toBe("");
  });

  it("counts the open tasks with no day, which no cell can show", () => {
    mount([], [task("a", null), task("b", null), task("c", null, { doneMs: 1 }), task("d", TODAY)]);
    expect(root.querySelector(".cal-undated")?.textContent).toBe("2 open tasks have no due day");
    mount([], [task("a", TODAY)]);
    expect(root.querySelector(".cal-undated")).toBeNull();
  });
});
