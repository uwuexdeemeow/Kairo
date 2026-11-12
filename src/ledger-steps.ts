import type { Step, Task } from "./api";
import { glyph } from "./glyphs";
import { h } from "./ui";

/// Steps a task may hold, and how long one may be: what the backend keeps (tasks.rs).
export const MAX_STEPS = 30;
export const MAX_STEP_CHARS = 120;

/// Where the keyboard goes once the panel is drawn again with the new list: the field to add a
/// step, or one step's text. Set by what was just done, taken by the next drawing.
let next: { task: string; at: "add" | number } | null = null;

/// Takes the place the keyboard was promised for this task's panel, and puts it there.
export function takeFocus(root: HTMLElement, task: string): boolean {
  const want = next;
  next = null;
  if (!want || want.task !== task) return false;
  const target = want.at === "add" ? root.querySelector<HTMLElement>(".step-add") : root.querySelectorAll<HTMLElement>(".step-text")[want.at];
  target?.focus();
  return !!target;
}

/// "2/5" for a task's row; none for no steps.
export function progress(steps: Step[]): string {
  return steps.length ? `${steps.filter((s) => s.d).length}/${steps.length}` : "";
}

/// Lines of pasted text as steps: bullets, numbers and empty checkboxes taken off the front,
/// blank lines dropped, each cut to the limit.
export function fromLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.replace(/^\s*(?:[-*•]\s+(?:\[[ xX]?\]\s*)?|\d+[.)]\s+)/, "").trim().slice(0, MAX_STEP_CHARS))
    .filter(Boolean);
}

/// The checklist inside a task's panel. `save` is given the whole new list.
export function stepsBlock(t: Task, save: (steps: Step[]) => void): HTMLElement {
  const steps = t.steps.map((s) => ({ ...s }));
  const commit = (list: Step[], at?: "add" | number) => {
    next = at === undefined ? null : { task: t.id, at };
    save(list);
  };

  const add = h("input", { class: "filter step-add", maxlength: "2000", placeholder: steps.length >= MAX_STEPS ? `Up to ${MAX_STEPS} steps` : "Add a step", "aria-label": "Add a step", autocomplete: "off" }) as HTMLInputElement;
  add.disabled = steps.length >= MAX_STEPS;

  const rows = steps.map((s, i) => {
    const tick = h("input", { type: "checkbox", "aria-label": `Step ${i + 1}: ${s.t}` }) as HTMLInputElement;
    tick.checked = s.d;
    tick.addEventListener("change", () => commit(steps.map((x, j) => (j === i ? { ...x, d: tick.checked } : x))));
    const text = h("input", { class: `filter step-text${s.d ? " done" : ""}`, maxlength: String(MAX_STEP_CHARS), "aria-label": `Step ${i + 1} text`, autocomplete: "off" }) as HTMLInputElement;
    text.value = s.t;
    // Kept when it's left or Enter is pressed; emptied, the step goes with it.
    text.addEventListener("change", () => {
      const value = text.value.trim();
      commit(value ? steps.map((x, j) => (j === i ? { ...x, t: value } : x)) : steps.filter((_, j) => j !== i));
    });
    text.addEventListener("keydown", (e) => {
      if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        // Alt and an arrow moves the step; the keyboard goes with it.
        e.preventDefault();
        const to = i + (e.key === "ArrowUp" ? -1 : 1);
        if (to < 0 || to >= steps.length) return;
        const moved = steps.slice();
        [moved[i], moved[to]] = [moved[to], moved[i]];
        commit(moved, to);
      } else if (e.key === "Enter") {
        // What's typed is kept, and the keyboard goes on to the field to add the next step.
        e.preventDefault();
        if (text.value.trim() === s.t) return add.focus();
        text.dispatchEvent(new Event("change"));
        // (The change saved with no promise of its own; this one is made after it.)
        next = { task: t.id, at: "add" };
      } else if (e.key === "Backspace" && !text.value) {
        e.preventDefault();
        commit(steps.filter((_, j) => j !== i), Math.max(0, i - 1));
      }
    });
    const del = h("button", { class: "link-btn step-del", type: "button", "aria-label": `Remove step ${i + 1}`, title: "Remove step" }, glyph("close"));
    del.addEventListener("click", () => commit(steps.filter((_, j) => j !== i)));
    return h("li", { class: "step", "data-step": String(i) }, tick, text, del);
  });

  const addLines = (lines: string[]) => {
    if (!lines.length) return;
    commit([...steps, ...lines.map((line) => ({ t: line, d: false }))].slice(0, MAX_STEPS), "add");
  };
  add.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addLines(fromLines(add.value));
    }
  });
  // Several lines pasted at once are several steps, not one long one.
  add.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text") ?? "";
    if (!/\r?\n/.test(text.trim())) return;
    e.preventDefault();
    addLines(fromLines(text));
  });

  const head = h("span", {}, "Steps", steps.length ? h("span", { class: "muted step-count" }, ` ${progress(steps)}`) : "");
  return h("div", { class: "detail-field detail-steps" }, head, h("ul", { class: "step-list", "aria-label": "Steps" }, ...rows), add);
}
