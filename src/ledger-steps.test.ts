import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Step, Task } from "./api";
import { fromLines, MAX_STEPS, progress, stepsBlock, takeFocus } from "./ledger-steps";

const task = (steps: Step[]): Task => ({
  id: "t1",
  title: "Essay",
  createdMs: 0,
  doneMs: null,
  position: 0,
  focusedMs: 0,
  dueDay: null,
  priority: 1,
  remindMs: null,
  reminded: "",
  note: null,
  steps,
  repeat: null,
  tags: [],
  project: null,
  group: "none",
});
const s = (t: string, d = false): Step => ({ t, d });

let root: HTMLElement;
let save: ReturnType<typeof vi.fn>;

const draw = (steps: Step[]) => {
  save = vi.fn();
  root.replaceChildren(stepsBlock(task(steps), save));
};
const texts = () => [...root.querySelectorAll<HTMLInputElement>(".step-text")];
const key = (el: HTMLElement, k: string, extra: KeyboardEventInit = {}) => el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...extra }));

beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
});

describe("steps on a row", () => {
  it("says how many are done, or nothing without steps", () => {
    expect(progress([])).toBe("");
    expect(progress([s("a", true), s("b"), s("c", true)])).toBe("2/3");
  });
});

describe("pasted lines", () => {
  it("become steps with their bullets, numbers and checkboxes taken off", () => {
    expect(fromLines("- one\n* two\n- [ ] three\n- [x] four\n1. five\n2) six\n\n  seven  \r\n")).toEqual(["one", "two", "three", "four", "five", "six", "seven"]);
  });

  it("are cut to the step limit", () => {
    expect(fromLines("x".repeat(300))[0]).toHaveLength(120);
  });
});

describe("the checklist", () => {
  it("lists the steps with their ticks and the count", () => {
    draw([s("Outline", true), s("Draft")]);
    expect(texts().map((t) => t.value)).toEqual(["Outline", "Draft"]);
    expect(root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[0].checked).toBe(true);
    expect(root.querySelector(".step-count")?.textContent).toContain("1/2");
  });

  it("ticks a step, keeping the others", () => {
    draw([s("Outline"), s("Draft")]);
    const box = root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1];
    box.checked = true;
    box.dispatchEvent(new Event("change"));
    expect(save).toHaveBeenCalledWith([s("Outline"), s("Draft", true)]);
  });

  it("adds a step on Enter, leaving the keyboard in the field for the next", () => {
    draw([s("Outline")]);
    const add = root.querySelector<HTMLInputElement>(".step-add")!;
    add.value = "  Draft  ";
    key(add, "Enter");
    expect(save).toHaveBeenCalledWith([s("Outline"), s("Draft")]);
    // The panel is drawn again with the new list; the keyboard goes where it was promised.
    draw([s("Outline"), s("Draft")]);
    expect(takeFocus(root, "t1")).toBe(true);
    expect(document.activeElement).toBe(root.querySelector(".step-add"));
    expect(takeFocus(root, "t1")).toBe(false);
  });

  it("adds nothing for an empty field", () => {
    draw([]);
    key(root.querySelector<HTMLInputElement>(".step-add")!, "Enter");
    expect(save).not.toHaveBeenCalled();
  });

  it("makes one step per pasted line", () => {
    draw([s("Outline")]);
    const add = root.querySelector<HTMLInputElement>(".step-add")!;
    const paste = new Event("paste", { bubbles: true, cancelable: true }) as Event & { clipboardData: unknown };
    paste.clipboardData = { getData: () => "- Draft\n- Edit\n- Send" };
    add.dispatchEvent(paste);
    expect(paste.defaultPrevented).toBe(true);
    expect(save).toHaveBeenCalledWith([s("Outline"), s("Draft"), s("Edit"), s("Send")]);
  });

  it("lets a single pasted line be pasted as text", () => {
    draw([]);
    const paste = new Event("paste", { bubbles: true, cancelable: true }) as Event & { clipboardData: unknown };
    paste.clipboardData = { getData: () => "just one line" };
    root.querySelector<HTMLInputElement>(".step-add")!.dispatchEvent(paste);
    expect(paste.defaultPrevented).toBe(false);
    expect(save).not.toHaveBeenCalled();
  });

  it("never goes past the limit, and says so in the field", () => {
    const full = Array.from({ length: MAX_STEPS }, (_, i) => s(`step ${i}`));
    draw(full);
    const add = root.querySelector<HTMLInputElement>(".step-add")!;
    expect(add.disabled).toBe(true);
    expect(add.placeholder).toContain(String(MAX_STEPS));
    draw(full.slice(1));
    const paste = new Event("paste", { bubbles: true, cancelable: true }) as Event & { clipboardData: unknown };
    paste.clipboardData = { getData: () => "a\nb\nc" };
    root.querySelector<HTMLInputElement>(".step-add")!.dispatchEvent(paste);
    expect((save.mock.calls[0][0] as Step[]).length).toBe(MAX_STEPS);
  });

  it("renames a step when its text is changed, and drops it when it's emptied", () => {
    draw([s("Outline"), s("Draft")]);
    texts()[1].value = "  Write it  ";
    texts()[1].dispatchEvent(new Event("change"));
    expect(save).toHaveBeenLastCalledWith([s("Outline"), s("Write it")]);
    texts()[0].value = "   ";
    texts()[0].dispatchEvent(new Event("change"));
    expect(save).toHaveBeenLastCalledWith([s("Draft")]);
  });

  it("removes a step with its button, and with Backspace in an empty one", () => {
    draw([s("a"), s("b"), s("c")]);
    root.querySelectorAll<HTMLButtonElement>(".step-del")[1].click();
    expect(save).toHaveBeenLastCalledWith([s("a"), s("c")]);
    texts()[2].value = "";
    key(texts()[2], "Backspace");
    expect(save).toHaveBeenLastCalledWith([s("a"), s("b")]);
    // The keyboard goes to the step above it.
    draw([s("a"), s("b")]);
    expect(takeFocus(root, "t1")).toBe(true);
    expect(document.activeElement).toBe(texts()[1]);
  });

  it("moves a step with Alt and the arrows, and not past the ends", () => {
    draw([s("a"), s("b"), s("c")]);
    key(texts()[1], "ArrowUp", { altKey: true });
    expect(save).toHaveBeenLastCalledWith([s("b"), s("a"), s("c")]);
    save.mockClear();
    key(texts()[0], "ArrowUp", { altKey: true });
    key(texts()[2], "ArrowDown", { altKey: true });
    expect(save).not.toHaveBeenCalled();
    key(texts()[0], "ArrowDown", { altKey: true });
    expect(save).toHaveBeenLastCalledWith([s("b"), s("a"), s("c")]);
  });

  it("goes on to the add field on Enter in an unchanged step", () => {
    draw([s("a")]);
    key(texts()[0], "Enter");
    expect(save).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(root.querySelector(".step-add"));
  });
});
