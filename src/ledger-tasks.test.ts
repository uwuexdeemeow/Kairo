import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { renderDetail, renderTaskList, type LedgerActions } from "./ledger-tasks";

const task = (id: string, title: string, extra: Partial<Task> = {}): Task => ({
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
  group: "none",
  ...extra,
});

let root: HTMLElement;
let act: { [K in keyof LedgerActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  act = {
    add: vi.fn(),
    preview: vi.fn(async () => "High · Fri 6 Nov"),
    done: vi.fn(),
    start: vi.fn(),
    select: vi.fn(),
    rename: vi.fn(),
    plan: vi.fn(),
    remove: vi.fn(),
  };
});

const headings = () => [...root.querySelectorAll(".ledger-group > h3")].map((e) => e.textContent);
const press = (el: Element, key: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));

describe("the task list", () => {
  it("shows only the groups with tasks, in order", () => {
    renderTaskList(root, [task("a", "Late", { group: "overdue", dueDay: "2026-11-01" }), task("b", "Someday")], null, act);
    expect(headings()).toEqual(["Overdue", "No date"]);
    expect(root.querySelector(".ledger-group.overdue")).not.toBeNull();
  });

  it("marks priority, the day and a reminder on a row", () => {
    renderTaskList(root, [task("a", "Essay", { group: "upcoming", dueDay: "2026-11-06", priority: 2, remindMs: 5 })], null, act);
    const row = root.querySelector('[data-task="a"]')!;
    expect(row.querySelector(".prio-high")).not.toBeNull();
    expect(row.querySelector(".task-day")!.textContent).toBe("Fri 6 Nov");
    expect(row.querySelector(".task-bell")).not.toBeNull();
  });

  it("doesn't repeat the day for today's tasks, and Low has no dot", () => {
    renderTaskList(root, [task("a", "Essay", { group: "today", dueDay: "2026-11-03", priority: 0 })], null, act);
    const row = root.querySelector('[data-task="a"]')!;
    expect(row.querySelector(".task-day")).toBeNull();
    expect(row.querySelector(".prio-dot")).toBeNull();
  });

  it("ticks, starts and opens a task", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    root.querySelector<HTMLInputElement>('[data-task="a"] input[type="checkbox"]')!.click();
    expect(act.done).toHaveBeenCalledWith("a", true);
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-start')!.click();
    expect(act.start).toHaveBeenCalledWith("a");
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-title')!.click();
    expect(act.select).toHaveBeenCalledWith("a");
  });

  it("says Focusing now for the running task", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Other")], { task: "a" }, act);
    expect(root.querySelector('[data-task="a"] .task-now')!.textContent).toBe("Focusing now");
    expect(root.querySelector(".task-start")).toBeNull();
  });

  it("folds done tasks away", () => {
    renderTaskList(root, [task("a", "Old", { group: "done", doneMs: 4 })], null, act);
    expect(root.querySelector("details.task-done summary")!.textContent).toBe("Done (1)");
  });

  it("adds on Enter and shows what it read", async () => {
    renderTaskList(root, [], null, act);
    expect(root.querySelector(".muted.ledger-empty")!.textContent).toBe(
      "Add what you're working on. Give it a day with “fri” or a priority with “!h”.",
    );
    const add = root.querySelector<HTMLInputElement>(".task-add")!;
    add.value = "essay !h fri";
    add.dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(root.querySelector(".task-preview")!.textContent).toBe("High · Fri 6 Nov"));
    expect(act.preview).toHaveBeenCalledWith("essay !h fri");
    press(add, "Enter");
    expect(act.add).toHaveBeenCalledWith("essay !h fri");
    expect(add.value).toBe("");
  });
});

describe("the detail panel", () => {
  const panel = () => document.getElementById("root")!;

  it("sets the priority and clears the day", () => {
    renderDetail(panel(), task("a", "Essay", { dueDay: "2026-11-06", priority: 1, remindMs: 99 }), act);
    panel().querySelector<HTMLButtonElement>('[data-priority="2"]')!.click();
    expect(act.plan).toHaveBeenCalledWith("a", "2026-11-06", 2, 99);
    panel().querySelector<HTMLButtonElement>(".clear-due")!.click();
    expect(act.plan).toHaveBeenLastCalledWith("a", null, 1, 99);
  });

  it("sets a day from the date field and a reminder from its own", () => {
    renderDetail(panel(), task("a", "Essay"), act);
    const due = panel().querySelector<HTMLInputElement>('input[type="date"]')!;
    due.value = "2026-11-09";
    due.dispatchEvent(new Event("change"));
    expect(act.plan).toHaveBeenCalledWith("a", "2026-11-09", 1, null);
    const remind = panel().querySelector<HTMLInputElement>('input[type="datetime-local"]')!;
    remind.value = "2026-11-08T15:00";
    remind.dispatchEvent(new Event("change"));
    expect(act.plan).toHaveBeenLastCalledWith("a", null, 1, new Date(2026, 10, 8, 15, 0).getTime());
  });

  it("renames on Enter, keeps the title when emptied, and deletes", () => {
    renderDetail(panel(), task("a", "Essay"), act);
    const title = panel().querySelector<HTMLInputElement>(".detail-title")!;
    title.value = "Essay draft";
    press(title, "Enter");
    expect(act.rename).toHaveBeenCalledWith("a", "Essay draft");
    title.value = "  ";
    press(title, "Enter");
    expect(act.rename).toHaveBeenCalledTimes(1);
    expect(title.value).toBe("Essay");
    panel().querySelector<HTMLButtonElement>(".detail-delete")!.click();
    expect(act.remove).toHaveBeenCalledWith("a");
  });
});
