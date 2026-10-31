import type { Task } from "./api";
import { fmtDur, h } from "./ui";

/// What the Tasks block asks the page to do; each redraws the block once done.
export type TaskActions = {
  add(title: string): void;
  rename(id: string, title: string): void;
  done(id: string, done: boolean): void;
  remove(id: string): void;
  start(id: string): void;
};

/// A task's title, which turns into a field when clicked: Enter or leaving it saves, Esc puts the
/// title back.
function title(t: Task, act: TaskActions): HTMLElement {
  const el = h("button", { class: "task-title", title: "Rename" }, t.title);
  el.addEventListener("click", () => {
    const edit = h("input", { class: "filter task-edit", maxlength: "80", "aria-label": "Task" }) as HTMLInputElement;
    edit.value = t.title;
    let finished = false;
    const finish = (save: boolean) => {
      if (finished) return;
      finished = true;
      const text = edit.value.trim();
      if (save && text && text !== t.title) act.rename(t.id, text);
      else edit.replaceWith(el);
    };
    edit.addEventListener("keydown", (e) => {
      if (e.key === "Enter") finish(true);
      if (e.key === "Escape") {
        e.stopPropagation();
        finish(false);
      }
    });
    edit.addEventListener("blur", () => finish(true));
    el.replaceWith(edit);
    edit.focus();
  });
  return el;
}

/// The running focus session's task, by id (null for a session on no task), or null with none.
export type RunningSession = { task: string | null } | null;

function row(t: Task, session: RunningSession, act: TaskActions): HTMLElement {
  const done = t.doneMs !== null;
  const tick = h("input", { type: "checkbox", "aria-label": done ? "Reopen" : "Mark done" }) as HTMLInputElement;
  tick.checked = done;
  tick.addEventListener("click", () => act.done(t.id, !done));
  const el = h("div", { class: `task-row${done ? " done" : ""}`, "data-task": t.id }, tick, title(t, act));
  el.append(h("span", { class: "task-time muted" }, t.focusedMs > 0 ? fmtDur(t.focusedMs) : ""));
  if (!done) {
    // While a session runs, Start would end it, so only the running task's row says anything.
    if (session && t.id === session.task) {
      el.append(h("span", { class: "task-now" }, "Focusing now"));
    } else if (!session) {
      const start = h("button", { class: "btn task-start", title: "Start a focus session on it" }, "Start");
      start.addEventListener("click", () => act.start(t.id));
      el.append(start);
    }
  }
  const remove = h("button", { class: "link-btn task-remove", "aria-label": `Delete ${t.title}`, title: "Delete" }, "×");
  remove.addEventListener("click", () => act.remove(t.id));
  el.append(remove);
  return el;
}

/// Draws the Focus page's Tasks block: the field to add one, the open tasks, and the done ones
/// folded away under "Done (n)".
export function renderTasks(root: HTMLElement, tasks: Task[], session: RunningSession, act: TaskActions): void {
  // Typing several tasks in a row keeps the field, though each add draws the block anew.
  const typing = document.activeElement?.classList.contains("task-add") && root.contains(document.activeElement);
  // Done stays open once opened, so reopening several doesn't mean unfolding it each time.
  const doneOpen = root.querySelector<HTMLDetailsElement>("details.task-done")?.open ?? false;
  const add = h("input", { class: "filter task-add", maxlength: "80", placeholder: "Add a task", "aria-label": "Add a task", autocomplete: "off" }) as HTMLInputElement;
  add.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    const text = add.value.trim();
    // Emptied at once, so a second Enter before the redraw can't add it twice.
    add.value = "";
    if (text) act.add(text);
  });
  const open = tasks.filter((t) => t.doneMs === null);
  const done = tasks.filter((t) => t.doneMs !== null);
  const parts: Node[] = [h("h2", {}, "Tasks"), add];
  if (!tasks.length) parts.push(h("p", { class: "muted" }, "Add what you're working on, then start a focus session on it."));
  parts.push(h("div", { class: "task-list" }, ...open.map((t) => row(t, session, act))));
  if (done.length) {
    const folded = h("details", { class: "task-done" }, h("summary", {}, `Done (${done.length})`), ...done.map((t) => row(t, session, act))) as HTMLDetailsElement;
    folded.open = doneOpen;
    parts.push(folded);
  }
  root.replaceChildren(...parts);
  if (typing) add.focus();
}
