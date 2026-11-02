import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { renderTaskPicker, type PickerActions } from "./tasks";

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
let act: { [K in keyof PickerActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  document.body.innerHTML = `<section id="root"></section>`;
  root = document.getElementById("root")!;
  act = { start: vi.fn(), openLedger: vi.fn() };
});

describe("Chrono's task picker", () => {
  it("offers open tasks in Ledger's order and starts on the one picked", () => {
    renderTaskPicker(root, [task("a", "Late", { group: "overdue" }), task("b", "Someday"), task("c", "Old", { group: "done", doneMs: 1 })], null, act);
    const pick = root.querySelector<HTMLSelectElement>("select")!;
    expect([...pick.options].map((o) => o.textContent)).toEqual(["Late", "Someday"]);
    pick.value = "b";
    root.querySelector<HTMLButtonElement>(".task-start")!.click();
    expect(act.start).toHaveBeenCalledWith("b");
  });

  it("says which task a running session is on", () => {
    renderTaskPicker(root, [task("a", "Essay")], { task: "a" }, act);
    expect(root.querySelector(".task-now")!.textContent).toBe("Focusing on “Essay”");
    expect(root.querySelector(".task-start")).toBeNull();
  });

  it("points to Ledger with no open tasks", () => {
    renderTaskPicker(root, [task("c", "Old", { group: "done", doneMs: 1 })], null, act);
    expect(root.querySelector("select")).toBeNull();
    expect(root.querySelector(".muted")!.textContent).toBe("Add tasks in Ledger, then start a focus session on one here.");
    root.querySelector<HTMLButtonElement>(".open-ledger")!.click();
    expect(act.openLedger).toHaveBeenCalled();
  });
});
