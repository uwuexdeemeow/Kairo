import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { forgetAddedDetails, ledgerLine, renderDetail, renderTaskList, taskSummary, type LedgerActions } from "./ledger-tasks";

/// Adds a detail from the panel's "Add…" choice, as a person would.
const addDetail = (host: HTMLElement, kind: string) => {
  const choice = host.querySelector<HTMLSelectElement>(".add-detail")!;
  choice.value = kind;
  choice.dispatchEvent(new Event("change"));
};

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
  note: null,
  steps: [],
  repeat: null,
  tags: [],
  project: null,
  group: "none",
  ...extra,
});

let root: HTMLElement;
let act: { [K in keyof LedgerActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  forgetAddedDetails();
  act = {
    add: vi.fn(),
    preview: vi.fn(async () => "High · Fri 6 Nov"),
    done: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    select: vi.fn(),
    rename: vi.fn(),
    plan: vi.fn(),
    remove: vi.fn(),
    openNote: vi.fn(),
    attach: vi.fn(),
    steps: vi.fn(),
    repeat: vi.fn(),
    labels: vi.fn(),
    newProject: vi.fn(),
    newNoteFor: vi.fn(),
  };
});

const headings = () => [...root.querySelectorAll(".ledger-group > h2")].map((e) => e.textContent);
const press = (el: Element, key: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));

const key = (el: Element, k: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
const titleOf = (id: string) => root.querySelector<HTMLElement>(`[data-task="${id}"] .task-title`)!;

describe("the add field's help", () => {
  it("folds the shorthand and keys away, and the preview isn't read out at every key", () => {
    renderTaskList(root, [], null, act);
    const help = root.querySelector<HTMLDetailsElement>("details.task-help")!;
    expect(help.open).toBe(false);
    expect(help.querySelector("summary")!.textContent).toBe("Shorthand and keys");
    expect(help.textContent).toContain("!h  !m  !l");
    expect(help.textContent).toContain("Ctrl+N  /  Ctrl+1 2 3 4 5");
    expect(root.querySelector(".task-preview")!.hasAttribute("aria-live")).toBe(false);
    expect(root.querySelector(".task-spoken")!.getAttribute("aria-live")).toBe("polite");
  });
});

describe("a focus session on a task", () => {
  it("says its length on Start, and time left with a Stop while it runs", () => {
    renderTaskList(root, [task("a", "Essay")], null, { ...act, focusMinutes: () => 50 });
    expect(root.querySelector('[data-task="a"] .task-start-len')!.textContent).toBe("50m");
    const ends = Date.now() + 18 * 60_000 - 1000;
    renderTaskList(root, [task("a", "Essay")], { task: "a", endsMs: ends }, act);
    expect(root.querySelector('[data-task="a"] .task-now')!.textContent).toBe("Focusing now · 18m left · Stop");
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-stop')!.click();
    expect(act.stop).toHaveBeenCalled();
  });
});

describe("the task list's keys", () => {

  it("is one Tab stop: the first task's title", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk")], null, act);
    const stops = [...root.querySelectorAll<HTMLElement>("[data-task] [tabindex='0'], [data-task] button:not([tabindex]), [data-task] input:not([tabindex])")];
    expect(stops).toEqual([titleOf("a")]);
  });

  it("moves with the arrows, ticks with Space, renames with F2 and deletes with Delete", () => {
    vi.useFakeTimers();
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk")], null, act);
    titleOf("a").focus();
    key(titleOf("a"), "ArrowDown");
    expect(document.activeElement).toBe(titleOf("b"));
    expect(titleOf("b").tabIndex).toBe(0);
    expect(titleOf("a").tabIndex).toBe(-1);
    key(titleOf("b"), " ");
    vi.advanceTimersByTime(300);
    expect(act.done).toHaveBeenCalledWith("b", true);
    key(titleOf("b"), "Delete");
    expect(act.remove).toHaveBeenCalledWith("b");
    key(titleOf("a"), "F2");
    expect(root.querySelector('[data-task="a"] .task-rename')).not.toBeNull();
    vi.useRealTimers();
  });

  it("reaches Start with the right arrow and comes back with the left", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    titleOf("a").focus();
    key(titleOf("a"), "ArrowRight");
    const start = root.querySelector<HTMLElement>('[data-task="a"] .task-start')!;
    expect(document.activeElement).toBe(start);
    key(start, "ArrowLeft");
    expect(document.activeElement).toBe(titleOf("a"));
    // Start says what it does: it blocks apps, not just a timer.
    expect(start.getAttribute("aria-label")).toContain("blocking distracting apps");
  });

  it("reaches Stop on the running task, so a session can be ended by keyboard", () => {
    renderTaskList(root, [task("a", "Essay")], { task: "a" }, act);
    titleOf("a").focus();
    key(titleOf("a"), "ArrowRight");
    const stop = root.querySelector<HTMLButtonElement>('[data-task="a"] .task-stop')!;
    expect(document.activeElement).toBe(stop);
    stop.click();
    expect(act.stop).toHaveBeenCalled();
    key(stop, "ArrowLeft");
    expect(document.activeElement).toBe(titleOf("a"));
  });
});

describe("the keyboard through a redraw", () => {
  it("isn't disturbed by a refresh that changes nothing", () => {
    const list = [task("a", "Essay"), task("b", "Milk")];
    renderTaskList(root, list, null, act);
    const tick = root.querySelector<HTMLInputElement>('[data-task="b"] input')!;
    tick.focus();
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk")], null, act);
    // The same element, still in the keyboard: nothing was redrawn.
    expect(root.querySelector('[data-task="b"] input')).toBe(tick);
    expect(document.activeElement).toBe(tick);
  });

  it("goes back to the same control on the same task when something changed", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk")], null, act);
    root.querySelector<HTMLElement>('[data-task="b"] .task-title')!.focus();
    renderTaskList(root, [task("a", "Essay draft"), task("b", "Milk")], null, act);
    expect(document.activeElement).toBe(root.querySelector('[data-task="b"] .task-title'));
  });

  it("moves to the next open task once one is ticked off into Done", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk"), task("c", "Bank")], null, act);
    root.querySelector<HTMLElement>('[data-task="b"] input')!.focus();
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk", { group: "done", doneMs: 1 }), task("c", "Bank")], null, act);
    expect(document.activeElement).toBe(root.querySelector('[data-task="c"] input'));
  });
});

describe("a done task", () => {
  it("isn't offered for renaming", () => {
    renderTaskList(root, [task("a", "Essay", { group: "done", doneMs: 1 }), task("b", "Milk")], null, act);
    key(titleOf("a"), "F2");
    expect(root.querySelector('[data-task="a"] .task-rename')).toBeNull();
    key(titleOf("b"), "F2");
    expect(root.querySelector('[data-task="b"] .task-rename')).not.toBeNull();
  });
});

describe("the open task", () => {
  it("is lit, and its title says the panel is open", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk")], null, act, "b");
    expect(root.querySelector('[data-task="b"]')!.classList.contains("open")).toBe(true);
    expect(root.querySelector('[data-task="a"]')!.classList.contains("open")).toBe(false);
    expect(root.querySelector('[data-task="b"] .task-title')!.getAttribute("aria-expanded")).toBe("true");
    expect(root.querySelector('[data-task="a"] .task-title')!.getAttribute("aria-expanded")).toBe("false");
  });
});

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
    expect(row.querySelector(".task-day")!.textContent).toBe("Due Fri 6 Nov");
    expect(row.querySelector(".task-bell")).not.toBeNull();
  });

  it("doesn't repeat the day for today's tasks, and marks only priorities other than Medium", () => {
    renderTaskList(root, [task("a", "Essay", { group: "today", dueDay: "2026-11-03", priority: 0 }), task("b", "Milk", { priority: 1 }), task("c", "Bank", { priority: 2 })], null, act);
    expect(root.querySelector('[data-task="a"] .task-day')).toBeNull();
    // Medium is where every task starts: no mark. Low a faint ring, High a filled dot.
    expect(root.querySelector('[data-task="a"] .prio-dot.prio-low')).not.toBeNull();
    expect(root.querySelector('[data-task="b"] .prio-dot')).toBeNull();
    expect(root.querySelector('[data-task="c"] .prio-dot.prio-high')).not.toBeNull();
  });

  it("names each tick for its task", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Milk", { group: "done", doneMs: 1 })], null, act);
    expect(root.querySelector('[data-task="a"] input')!.getAttribute("aria-label")).toBe("Mark “Essay” done");
    expect(root.querySelector('[data-task="b"] input')!.getAttribute("aria-label")).toBe("Reopen “Milk”");
  });

  it("ticks, starts and opens a task", () => {
    vi.useFakeTimers();
    renderTaskList(root, [task("a", "Essay")], null, act);
    root.querySelector<HTMLInputElement>('[data-task="a"] input[type="checkbox"]')!.click();
    // The ink stroke is drawn through it first.
    expect(root.querySelector('[data-task="a"]')!.classList.contains("inking")).toBe(true);
    expect(act.done).not.toHaveBeenCalled();
    vi.advanceTimersByTime(280);
    expect(act.done).toHaveBeenCalledWith("a", true);
    vi.useRealTimers();
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-start')!.click();
    expect(act.start).toHaveBeenCalledWith("a");
    root.querySelector<HTMLButtonElement>('[data-task="a"] .task-title')!.click();
    expect(act.select).toHaveBeenCalledWith("a");
  });

  it("keeps a second tick made while the first is inking", () => {
    vi.useFakeTimers();
    renderTaskList(root, [task("a", "Essay"), task("b", "Call")], null, act);
    root.querySelector<HTMLInputElement>('[data-task="a"] input')!.click();
    root.querySelector<HTMLInputElement>('[data-task="b"] input')!.click();
    vi.advanceTimersByTime(280);
    expect(act.done).toHaveBeenCalledWith("a", true);
    expect(act.done).toHaveBeenCalledWith("b", true);
    vi.useRealTimers();
  });

  it("ticks at once for those who ask for no motion", () => {
    const was = window.matchMedia;
    window.matchMedia = ((q: string) => ({ matches: q.includes("reduce") })) as unknown as typeof window.matchMedia;
    try {
      renderTaskList(root, [task("a", "Essay")], null, act);
      root.querySelector<HTMLInputElement>('[data-task="a"] input')!.click();
      expect(act.done).toHaveBeenCalledWith("a", true);
    } finally {
      window.matchMedia = was;
    }
  });

  it("ribbons overdue tasks, and draws the notebook when there are none", () => {
    renderTaskList(root, [task("a", "Late", { group: "overdue", dueDay: "2026-11-01" }), task("b", "Fine")], null, act);
    expect(root.querySelector('[data-task="a"].ribbon')).not.toBeNull();
    expect(root.querySelector('[data-task="b"].ribbon')).toBeNull();
    renderTaskList(root, [], null, act);
    expect(root.querySelector(".ledger-empty-art .logo-ledger")).not.toBeNull();
  });

  it("says when a search finds nothing, rather than that there's nothing to do", () => {
    renderTaskList(root, [], null, act, null, "bank");
    expect(root.querySelector(".ledger-empty")!.textContent).toBe("No tasks match “bank”. Esc clears the search.");
    expect(root.querySelector(".ledger-empty-art")).toBeNull();
    // A search finding only done tasks isn't "all done".
    renderTaskList(root, [task("a", "Bank", { group: "done" })], null, act, null, "bank");
    expect(root.querySelector(".ledger-all-done")).toBeNull();
  });

  it("says when everything is done", () => {
    renderTaskList(root, [task("a", "Essay", { group: "done" })], null, act);
    expect(root.querySelector(".ledger-all-done")!.textContent).toBe("All done. Nothing left open.");
    renderTaskList(root, [task("a", "Essay", { group: "done" }), task("b", "Other")], null, act);
    expect(root.querySelector(".ledger-all-done")).toBeNull();
  });

  it("says Focusing now for the running task", () => {
    renderTaskList(root, [task("a", "Essay"), task("b", "Other")], { task: "a" }, act);
    expect(root.querySelector('[data-task="a"] .task-now')!.textContent).toBe("Focusing now · Stop");
    expect(root.querySelector(".task-start")).toBeNull();
  });

  it("folds done tasks away", () => {
    renderTaskList(root, [task("a", "Old", { group: "done", doneMs: 4 })], null, act);
    expect(root.querySelector("details.task-done summary")!.textContent).toBe("Done (1)");
  });

  it("renames a task in the list with F2, with no bubble on the card", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    expect(root.querySelector('[data-task="a"] .rename-bubble')).toBeNull();
    key(titleOf("a"), "F2");
    const field = root.querySelector<HTMLInputElement>('[data-task="a"] .task-rename')!;
    expect(field.value).toBe("Essay");
    expect(document.activeElement).toBe(field);
    // A redraw while renaming leaves it be.
    renderTaskList(root, [task("a", "Essay"), task("b", "New")], null, act);
    expect(root.querySelector('[data-task="a"] .task-rename')).toBe(field);
    field.value = "Essay draft";
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(act.rename).toHaveBeenCalledWith("a", "Essay draft");
  });

  it("puts the title back on Esc, or when emptied", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    key(titleOf("a"), "F2");
    let field = root.querySelector<HTMLInputElement>(".task-rename")!;
    field.value = "Other";
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.querySelector(".task-rename")).toBeNull();
    expect(root.querySelector('[data-task="a"] .task-title')!.textContent).toBe("Essay");
    key(titleOf("a"), "F2");
    field = root.querySelector<HTMLInputElement>(".task-rename")!;
    field.value = "  ";
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(act.rename).not.toHaveBeenCalled();
    expect(root.querySelector('[data-task="a"] .task-title')!.textContent).toBe("Essay");
  });

  it("keeps what's being typed when the list is drawn again", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    const add = root.querySelector<HTMLInputElement>(".task-add")!;
    add.focus();
    add.value = "call mu";
    // Ledger redraws every few seconds, and when tasks change elsewhere.
    renderTaskList(root, [task("a", "Essay"), task("b", "From the laptop")], null, act);
    const after = root.querySelector<HTMLInputElement>(".task-add")!;
    expect(after.value).toBe("call mu");
    expect(document.activeElement).toBe(after);
    expect(root.querySelectorAll(".task-row").length).toBe(2);
  });

  it("adds on Enter and shows what it read", async () => {
    renderTaskList(root, [], null, act);
    // The hint above the field teaches the shorthand; the empty list doesn't repeat it.
    expect(root.querySelector(".muted.ledger-empty")!.textContent).toBe("Nothing to do yet. What are you working on?");
    const add = root.querySelector<HTMLInputElement>(".task-add")!;
    add.value = "essay !h fri";
    add.dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(root.querySelector(".task-preview")!.textContent).toBe("High · Fri 6 Nov"));
    expect(act.preview).toHaveBeenCalledWith("essay !h fri");
    press(add, "Enter");
    expect(act.add).toHaveBeenCalledWith("essay !h fri");
    // Emptied once it's kept.
    await vi.waitFor(() => expect(add.value).toBe(""));
  });

  it("keeps what was typed when adding fails, to try again", async () => {
    renderTaskList(root, [], null, act);
    const add = root.querySelector<HTMLInputElement>(".task-add")!;
    act.add.mockRejectedValueOnce("database is locked");
    add.value = "call the bank";
    press(add, "Enter");
    await vi.waitFor(() => expect(root.querySelector(".task-preview")!.textContent).toBe("Couldn't add this. Press Enter to try again."));
    expect(add.value).toBe("call the bank");
  });
});

describe("a task's note", () => {
  const notes = [
    { id: "n1", title: "Plan" },
    { id: "n2", title: "Shopping" },
  ];

  it("marks a row whose task has a note", () => {
    renderTaskList(root, [task("a", "Essay", { note: "n1" }), task("b", "Other")], null, act);
    expect(root.querySelector('[data-task="a"] .task-note')).not.toBeNull();
    expect(root.querySelector('[data-task="b"] .task-note')).toBeNull();
  });

  it("opens or detaches the attached note", () => {
    renderDetail(root, task("a", "Essay", { note: "n1" }), act, notes);
    const open = root.querySelector<HTMLButtonElement>(".detail-note .open-note")!;
    expect(open.textContent).toBe("Plan");
    expect(open.querySelector(".glyph-out")).not.toBeNull();
    open.click();
    expect(act.openNote).toHaveBeenCalledWith("n1");
    root.querySelector<HTMLButtonElement>(".detail-note .detach-note")!.click();
    expect(act.attach).toHaveBeenCalledWith("a", null);
  });

  it("attaches a note picked from a search, or a new one named after the task", () => {
    renderDetail(root, task("a", "Essay"), act, notes);
    addDetail(root, "note");
    root.querySelector<HTMLButtonElement>(".attach-note")!.click();
    const options = () => [...root.querySelectorAll<HTMLButtonElement>(".note-pick button")];
    expect(options().map((b) => b.textContent)).toEqual(["New note: Essay", "Plan", "Shopping"]);
    const find = root.querySelector<HTMLInputElement>(".note-pick input")!;
    find.value = "sho";
    find.dispatchEvent(new Event("input"));
    expect(options().map((b) => b.textContent)).toEqual(["New note: Essay", "Shopping"]);
    options()[1].click();
    expect(act.attach).toHaveBeenCalledWith("a", "n2");
    options()[0].click();
    expect(act.newNoteFor).toHaveBeenCalledWith("a", "Essay");
  });
});

describe("the Tasks screen as cards", () => {
  it("sums the list up under the title", () => {
    expect(taskSummary([task("a", "A", { group: "today" }), task("b", "B", { group: "today" }), task("c", "C", { group: "overdue" }), task("d", "D")])).toBe(
      "2 due today · 1 overdue · 4 open",
    );
    expect(taskSummary([task("d", "D"), task("e", "E", { group: "done", doneMs: 1 })])).toBe("Nothing due · 1 open");
    expect(taskSummary([])).toBe("");
  });

  it("draws each task as a card with its chips", () => {
    renderTaskList(
      root,
      [task("a", "Essay", { group: "overdue", dueDay: "2026-11-02", remindMs: new Date(2026, 10, 2, 15, 0).getTime(), note: "n1", focusedMs: 80 * 60_000 })],
      null,
      act,
    );
    const card = root.querySelector('[data-task="a"]')!;
    expect(card.classList.contains("task-card")).toBe(true);
    const chips = [...card.querySelectorAll(".task-chips > span")].map((c) => c.className.split(" ")[0]);
    expect(chips).toEqual(["task-day", "task-bell", "task-note", "task-time"]);
    expect(card.querySelector(".task-day")!.classList.contains("late")).toBe(true);
    const bell = card.querySelector(".task-bell")!;
    // A drawn bell, not an emoji, and the time it rings said aloud.
    expect(bell.querySelector("svg.glyph-bell")).not.toBeNull();
    // Read out as "Reminder at 3:00": the words are there, unseen, beside the drawn bell.
    expect(bell.textContent).toMatch(/^Reminder at 0?3:00/);
    expect(bell.querySelector(".sr-only")!.textContent).toBe("Reminder at ");
    expect(card.querySelector(".task-note svg.glyph-note")).not.toBeNull();
    expect(card.querySelector(".task-note .sr-only")!.textContent).toBe("Has a note");
    expect(card.querySelector(".task-time")!.textContent).toBe("Focused 1h 20m");
  });

  it("starts focus from a small play button", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    const play = root.querySelector<HTMLButtonElement>('[data-task="a"] .task-start')!;
    expect(play.querySelector("svg.glyph-play")).not.toBeNull();
    expect(play.getAttribute("aria-label")).toBe("Focus 25 minutes on Essay, blocking distracting apps");
    expect(play.title).toBe("Focus 25 min: blocks distracting apps");
    play.click();
    expect(act.start).toHaveBeenCalledWith("a");
  });

  it("shows a hint under an empty Add a task", () => {
    renderTaskList(root, [task("a", "Essay")], null, act);
    expect(root.querySelector(".task-preview")!.textContent).toBe("Try “essay draft !h fri” for High priority, due Friday");
  });

  it("closes the task panel from its ×", () => {
    renderDetail(root, task("a", "Essay"), act);
    root.querySelector<HTMLButtonElement>(".detail-close")!.click();
    expect(act.select).toHaveBeenCalledWith("a");
  });
});

describe("Ledger's line on the dashboard", () => {
  it("counts today's and overdue tasks", () => {
    const today = (id: string) => task(id, id, { group: "today" });
    expect(ledgerLine([today("a"), today("b"), today("c"), task("d", "d", { group: "overdue" })])).toBe("3 due today · 1 overdue");
    expect(ledgerLine([task("d", "d", { group: "overdue" })])).toBe("1 overdue");
    expect(ledgerLine([today("a"), today("b")])).toBe("2 due today");
    expect(ledgerLine([task("e", "e"), task("f", "f", { group: "done", doneMs: 1 })])).toBe("Nothing due today");
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

  it("shows only what a task has, with one Add… choice for the rest", () => {
    renderDetail(panel(), task("a", "Essay", { dueDay: "2026-11-06" }), act, [], null, ["Kairo"]);
    // Always: title, Mark done, Focus, Due, Priority, and Delete in the foot.
    for (const sel of [".detail-title", ".detail-done", ".detail-start", ".day-field", '[data-priority="2"]', ".detail-foot .detail-delete"]) expect(panel().querySelector(sel), sel).not.toBeNull();
    // Not until it has them or they're added.
    for (const sel of [".repeat-select", ".project-select", ".tag-add", ".step-add", ".attach-note", ".remind-when"]) expect(panel().querySelector(sel), sel).toBeNull();
    const choice = panel().querySelector<HTMLSelectElement>(".add-detail")!;
    expect([...choice.options].map((o) => o.textContent)).toEqual(["Add…", "Repeat", "Project", "Tags", "Reminder", "Steps", "Note"]);
  });

  it("opens a field from Add… with the keyboard in it, and keeps it while the panel is redrawn", () => {
    const t = task("a", "Essay");
    renderDetail(panel(), t, act, [], null, ["Kairo"]);
    addDetail(panel(), "tags");
    expect(document.activeElement).toBe(panel().querySelector(".tag-add"));
    // What's been added is no longer offered, and stays when the list refreshes.
    expect([...panel().querySelectorAll<HTMLOptionElement>(".add-detail option")].map((o) => o.textContent)).not.toContain("Tags");
    renderDetail(panel(), t, act, [], null, ["Kairo"]);
    expect(panel().querySelector(".tag-add")).not.toBeNull();
    addDetail(panel(), "project");
    addDetail(panel(), "steps");
    expect(document.activeElement).toBe(panel().querySelector(".step-add"));
  });

  it("shows the fields a task already has and offers only what's missing", () => {
    renderDetail(panel(), task("a", "Essay", { tags: ["x"], project: "Kairo", steps: [{ t: "One", d: false }], repeat: "week", remindMs: 5, note: "n1" }), act, [{ id: "n1", title: "Plan" }], null, ["Kairo"]);
    for (const sel of [".repeat-select", ".project-select", ".tag-add", ".step-add", ".remind-when", ".detail-note"]) expect(panel().querySelector(sel), sel).not.toBeNull();
    // Nothing missing: no Add… at all.
    expect(panel().querySelector(".add-detail")).toBeNull();
  });

  it("chooses how a task repeats, and never", () => {
    renderDetail(panel(), task("a", "Essay"), act);
    addDetail(panel(), "repeat");
    const select = panel().querySelector<HTMLSelectElement>(".repeat-select")!;
    expect(select.getAttribute("aria-label")).toBe("Repeat");
    // The keyboard is in the field just added.
    expect(document.activeElement).toBe(select);
    expect([...select.options].map((o) => o.textContent)).toEqual(["Never", "Every day", "Every weekday", "Every week", "Every month", "Every year"]);
    expect(select.value).toBe("");
    select.value = "week";
    select.dispatchEvent(new Event("change"));
    expect(act.repeat).toHaveBeenLastCalledWith("a", "week");
    renderDetail(panel(), task("a", "Essay", { repeat: "week" }), act);
    const again = panel().querySelector<HTMLSelectElement>(".repeat-select")!;
    expect(again.value).toBe("week");
    again.value = "";
    again.dispatchEvent(new Event("change"));
    expect(act.repeat).toHaveBeenLastCalledWith("a", null);
  });

  it("says on a task's row that it repeats, in words too", () => {
    renderTaskList(root, [task("a", "Water plants", { repeat: "month", group: "none" }), task("b", "Essay")], null, act as unknown as LedgerActions, null, "");
    const chip = root.querySelector('[data-task="a"] .task-repeat')!;
    expect(chip.textContent).toContain("monthly");
    expect(chip.getAttribute("title")).toBe("Every month");
    expect(root.querySelector('[data-task="b"] .task-repeat')).toBeNull();
  });

  it("shows a task's project and tags on its row, and offers them in the panel", () => {
    const t = task("a", "Ship it", { project: "Kairo", tags: ["release", "urgent"] });
    renderTaskList(root, [t, task("b", "Plain")], null, act as unknown as LedgerActions, null, "");
    expect(root.querySelector('[data-task="a"] .task-project')?.textContent).toContain("Kairo");
    // The first tag and a count; the tooltip has them all.
    expect([...root.querySelectorAll('[data-task="a"] .task-tag')].map((x) => x.textContent)).toEqual(["#release +1"]);
    expect(root.querySelector('[data-task="a"] .task-tag')!.getAttribute("title")).toBe("#release #urgent");
    expect(root.querySelector('[data-task="b"] .task-project')).toBeNull();
    expect(root.querySelector('[data-task="b"] .task-tag')).toBeNull();
    renderDetail(panel(), t, act, [], null, ["Kairo", "Home"]);
    expect(panel().querySelector<HTMLSelectElement>(".project-select")!.value).toBe("Kairo");
    expect(panel().querySelectorAll(".tag-chip")).toHaveLength(2);
  });

  it("keeps the keyboard on the priority just chosen when it's redrawn", () => {
    renderDetail(panel(), task("a", "Essay", { priority: 1 }), act);
    panel().querySelector<HTMLButtonElement>('[data-priority="2"]')!.focus();
    renderDetail(panel(), task("a", "Essay", { priority: 2 }), act);
    expect(document.activeElement).toBe(panel().querySelector('[data-priority="2"]'));
  });

  it("names its × buttons by what they do", () => {
    renderDetail(panel(), task("a", "Essay", { dueDay: "2026-11-06", remindMs: 5 }), act);
    expect(panel().querySelector(".clear-due")!.getAttribute("aria-label")).toBe("Clear due day");
    expect(panel().querySelector(".clear-remind")!.getAttribute("aria-label")).toBe("Clear reminder");
  });

  it("ticks the task, and starts or stops its focus, from the panel", () => {
    renderDetail(panel(), task("a", "Essay"), act);
    panel().querySelector<HTMLButtonElement>(".detail-done")!.click();
    expect(act.done).toHaveBeenCalledWith("a", true);
    expect(panel().querySelector(".detail-now")!.textContent).toBe("Blocks distracting apps");
    panel().querySelector<HTMLButtonElement>(".detail-start")!.click();
    expect(act.start).toHaveBeenCalledWith("a");
    renderDetail(panel(), task("a", "Essay"), act, [], { task: "a" });
    panel().querySelector<HTMLButtonElement>(".detail-stop")!.click();
    expect(act.stop).toHaveBeenCalled();
    // A day to clear is offered only when there is one.
    expect(panel().querySelector(".clear-due")).toBeNull();
  });

  it("offers a reminder rather than showing a time that looks set", () => {
    renderDetail(panel(), task("a", "Essay", { dueDay: "2026-11-06" }), act);
    expect(panel().querySelector("select.remind-time")).toBeNull();
    // "Reminder" in Add… makes one at 9 on the due day, at once, to move from there.
    addDetail(panel(), "remind");
    expect(act.plan).toHaveBeenCalledWith("a", "2026-11-06", 1, new Date(2026, 10, 6, 9, 0).getTime());
  });

  it("sets a day from Kairo's calendar and a reminder from a day and a time, with no native date field", () => {
    const nine = new Date(2026, 10, 6, 9, 0).getTime();
    renderDetail(panel(), task("a", "Essay", { dueDay: "2026-11-06", remindMs: nine }), act);
    expect(panel().querySelector('input[type="date"], input[type="datetime-local"]')).toBeNull();
    const [due, remindDay] = panel().querySelectorAll<HTMLButtonElement>(".day-field");
    expect(due.textContent).toMatch(/6/);
    expect(remindDay.textContent).toMatch(/6/);

    due.click();
    const cal = panel().querySelector<HTMLElement>(".calendar:not([hidden])")!;
    expect(cal.querySelector('[data-date="2026-11-06"]')!.classList.contains("selected")).toBe(true);
    cal.querySelector<HTMLButtonElement>('[data-date="2026-11-09"]')!.click();
    expect(act.plan).toHaveBeenCalledWith("a", "2026-11-09", 1, nine);
    expect(cal.hidden).toBe(true);

    const time = panel().querySelector<HTMLSelectElement>("select.remind-time")!;
    expect(panel().querySelector('input[type="time"]')).toBeNull();
    time.value = "15:00";
    remindDay.click();
    panel().querySelector<HTMLButtonElement>('.calendar:not([hidden]) [data-date="2026-11-08"]')!.click();
    expect(act.plan).toHaveBeenLastCalledWith("a", "2026-11-06", 1, new Date(2026, 10, 8, 15, 0).getTime());
  });

  it("moves a reminder's time on the same day", () => {
    const at = new Date(2026, 10, 8, 9, 0).getTime();
    renderDetail(panel(), task("a", "Essay", { remindMs: at }), act);
    const time = panel().querySelector<HTMLSelectElement>("select.remind-time")!;
    expect(time.value).toBe("09:00");
    // Every quarter hour, as this computer writes times.
    expect(time.options.length).toBe(96);
    time.value = "16:30";
    time.dispatchEvent(new Event("change"));
    expect(act.plan).toHaveBeenLastCalledWith("a", null, 1, new Date(2026, 10, 8, 16, 30).getTime());
  });

  it("keeps a reminder's time off the quarter hours as its own choice", () => {
    renderDetail(panel(), task("a", "Essay", { remindMs: new Date(2026, 10, 8, 9, 7).getTime() }), act);
    const time = panel().querySelector<HTMLSelectElement>("select.remind-time")!;
    expect(time.value).toBe("09:07");
    expect(time.options.length).toBe(97);
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
