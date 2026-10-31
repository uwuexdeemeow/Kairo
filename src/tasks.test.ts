import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { renderTasks, type TaskActions } from "./tasks";

const task = (id: string, title: string, extra: Partial<Task> = {}): Task => ({ id, title, createdMs: 0, doneMs: null, position: 0, focusedMs: 0, ...extra });

let root: HTMLElement;
let act: { [K in keyof TaskActions]: ReturnType<typeof vi.fn> };

function draw(tasks: Task[], session: { task: string | null } | null = null) {
  renderTasks(root, tasks, session, act as unknown as TaskActions);
}

const field = () => root.querySelector<HTMLInputElement>(".task-add")!;
const type = (input: HTMLInputElement, text: string, key = "Enter") => {
  input.value = text;
  input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
};

beforeEach(() => {
  document.body.innerHTML = "";
  root = document.createElement("section");
  document.body.append(root);
  act = { add: vi.fn(), rename: vi.fn(), done: vi.fn(), remove: vi.fn(), start: vi.fn() };
});

describe("the Tasks block", () => {
  it("adds on Enter and ignores blank", () => {
    draw([]);
    type(field(), "Essay");
    expect(act.add).toHaveBeenCalledWith("Essay");
    // Cleared at once, so a second Enter before the redraw adds nothing more.
    expect(field().value).toBe("");
    type(field(), "");
    expect(act.add).toHaveBeenCalledTimes(1);
    act.add.mockClear();
    type(field(), "   ");
    expect(act.add).not.toHaveBeenCalled();
  });

  it("ticks a task into Done", () => {
    draw([task("a", "Essay"), task("b", "Taxes", { doneMs: 5 })]);
    root.querySelector<HTMLInputElement>('[data-task="a"] input[type=checkbox]')!.click();
    expect(act.done).toHaveBeenCalledWith("a", true);
    const done = root.querySelector("details")!;
    expect(done.open).toBe(false);
    expect(done.querySelector("summary")!.textContent).toBe("Done (1)");
    done.querySelector<HTMLInputElement>('[data-task="b"] input[type=checkbox]')!.click();
    expect(act.done).toHaveBeenCalledWith("b", false);
  });

  it("keeps Done open across a redraw once opened", () => {
    draw([task("a", "Essay", { doneMs: 5 }), task("b", "Taxes", { doneMs: 6 })]);
    root.querySelector("details")!.open = true;
    draw([task("a", "Essay", { doneMs: 5 })]);
    expect(root.querySelector("details")!.open).toBe(true);
  });

  it("renames in place, Esc cancels", () => {
    draw([task("a", "Essay")]);
    root.querySelector<HTMLElement>('[data-task="a"] .task-title')!.click();
    const edit = root.querySelector<HTMLInputElement>('[data-task="a"] .task-edit')!;
    type(edit, "New");
    expect(act.rename).toHaveBeenCalledWith("a", "New");

    draw([task("a", "Essay")]);
    root.querySelector<HTMLElement>('[data-task="a"] .task-title')!.click();
    type(root.querySelector<HTMLInputElement>('[data-task="a"] .task-edit')!, "Nope", "Escape");
    expect(act.rename).toHaveBeenCalledTimes(1);
    expect(root.querySelector('[data-task="a"] .task-title')!.textContent).toBe("Essay");
  });

  it("keeps the add field focused across a redraw, to add several in a row", () => {
    draw([]);
    field().focus();
    type(field(), "Essay");
    draw([task("a", "Essay")]);
    expect(document.activeElement).toBe(field());
  });

  it("shows Focusing now for the running task, and no Start anywhere while a session runs", () => {
    draw([task("a", "Essay"), task("b", "Taxes")], { task: "a" });
    expect(root.querySelector('[data-task="a"]')!.textContent).toContain("Focusing now");
    expect(root.querySelectorAll(".task-start")).toHaveLength(0);
    draw([task("a", "Essay"), task("b", "Taxes")], { task: null });
    expect(root.querySelectorAll(".task-start")).toHaveLength(0);
    draw([task("a", "Essay"), task("b", "Taxes")]);
    root.querySelector<HTMLButtonElement>('[data-task="b"] .task-start')!.click();
    expect(act.start).toHaveBeenCalledWith("b");
  });

  it("survives a running task that was deleted", () => {
    draw([task("a", "Essay")], { task: "gone" });
    expect(root.textContent).not.toContain("Focusing now");
    expect(root.querySelector('[data-task="a"]')).not.toBeNull();
  });

  it("deletes with ×", () => {
    draw([task("a", "Essay")]);
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-remove')!.click();
    expect(act.remove).toHaveBeenCalledWith("a");
  });

  it("shows the empty line", () => {
    draw([]);
    expect(root.textContent).toContain("Add what you're working on, then start a focus session on it.");
  });

  it("shows titles as text", () => {
    draw([task("a", '<b>"x"</b>')]);
    expect(root.querySelector('[data-task="a"] .task-title')!.textContent).toBe('<b>"x"</b>');
    expect(root.querySelector("b")).toBeNull();
  });

  it("shows focus time", () => {
    draw([task("a", "Essay", { focusedMs: 80 * 60_000 }), task("b", "Taxes")]);
    expect(root.querySelector('[data-task="a"] .task-time')!.textContent).toBe("1h 20m");
    expect(root.querySelector('[data-task="b"] .task-time')?.textContent ?? "").toBe("");
  });
});
