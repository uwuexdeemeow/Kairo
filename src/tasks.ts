import type { Task } from "./api";
import { h } from "./ui";

/// What the Focus page's task picker asks the page to do.
export type PickerActions = {
  start(id: string): void;
  openLedger(): void;
};

/// The running focus session's task, by id (null for a session on no task), or null with none.
export type RunningSession = { task: string | null } | null;

/// Draws the Focus page's Tasks block: a pick of the open tasks, in Ledger's order, to start a
/// session on, and the way to Ledger, where tasks are kept.
export function renderTaskPicker(root: HTMLElement, tasks: Task[], session: RunningSession, act: PickerActions): void {
  const open = tasks.filter((t) => t.doneMs === null);
  const ledger = h("button", { class: "link-btn open-ledger" }, "Open Ledger");
  ledger.addEventListener("click", () => act.openLedger());
  const parts: Node[] = [h("h2", {}, "Tasks")];
  const running = session?.task ? open.find((t) => t.id === session.task) : undefined;
  if (running) {
    parts.push(h("p", { class: "task-now" }, `Focusing on “${running.title}”`));
  } else if (!open.length) {
    parts.push(h("p", { class: "muted" }, "Add tasks in Ledger, then start a focus session on one here."));
  } else if (!session) {
    const pick = h("select", { "aria-label": "Focus on a task" }, ...open.map((t) => h("option", { value: t.id }, t.title))) as HTMLSelectElement;
    const start = h("button", { class: "btn task-start" }, "Start focus");
    start.addEventListener("click", () => act.start(pick.value));
    parts.push(h("div", { class: "row-buttons" }, pick, start));
  }
  parts.push(h("div", { class: "row-buttons" }, ledger));
  root.replaceChildren(...parts);
}
