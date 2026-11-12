import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";
import { cleanTag, filterBar, filterSummary, labelsBlock, labelsInUse, matches, MAX_TAGS, NO_FILTER, takeLabelFocus } from "./ledger-labels";

const task = (id: string, extra: Partial<Task> = {}): Task => ({
  id,
  title: id,
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
beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
});

describe("tags", () => {
  it("are cleaned the way the backend keeps them", () => {
    expect(cleanTag("  #School ")).toBe("school");
    expect(cleanTag("a b!c")).toBe("abc");
    expect(cleanTag("###")).toBe("");
    expect(cleanTag("x".repeat(60))).toHaveLength(24);
    expect(cleanTag("Étude_2")).toBe("étude_2");
  });
});

describe("narrowing by tag or project", () => {
  const tasks = [
    task("a", { tags: ["school"], project: "Kairo", focusedMs: 90 * 60_000 }),
    task("b", { tags: ["school", "urgent"] }),
    task("c", { project: "kairo" }),
    task("d", { tags: ["home"], doneMs: 5 }),
  ];

  it("matches a tag, a project whatever its case, or both", () => {
    const ids = (f: { tag: string | null; project: string | null }) => tasks.filter((t) => matches(t, f)).map((t) => t.id);
    expect(ids(NO_FILTER)).toEqual(["a", "b", "c", "d"]);
    expect(ids({ tag: "school", project: null })).toEqual(["a", "b"]);
    expect(ids({ tag: null, project: "Kairo" })).toEqual(["a", "c"]);
    expect(ids({ tag: "school", project: "Kairo" })).toEqual(["a"]);
    expect(ids({ tag: "nothing", project: null })).toEqual([]);
  });

  it("offers the tags and projects open tasks use, in order, and not a done task's", () => {
    // ("kairo" and "Kairo" are one project.)
    expect(labelsInUse(tasks)).toEqual({ tags: ["school", "urgent"], projects: ["Kairo"] });
  });

  it("says how many are open and how long was focused on what's narrowed to", () => {
    expect(filterSummary(tasks, NO_FILTER)).toBe("");
    expect(filterSummary(tasks, { tag: null, project: "Kairo" })).toBe("Kairo · 2 open · 1h 30m focused");
    expect(filterSummary(tasks, { tag: "school", project: "Kairo" })).toBe("Kairo #school · 1 open · 1h 30m focused");
    expect(filterSummary(tasks, { tag: "urgent", project: null })).toBe("#urgent · 1 open");
  });

  it("shows chips that press to narrow and press again to let go, with Show all", () => {
    const change = vi.fn();
    filterBar(root, tasks, NO_FILTER, change);
    const chips = [...root.querySelectorAll<HTMLButtonElement>(".label-chip")];
    expect(chips.map((c) => c.textContent)).toEqual(["Kairo", "#school", "#urgent"]);
    expect(root.querySelector(".label-clear")).toBeNull();
    chips[1].click();
    expect(change).toHaveBeenLastCalledWith({ tag: "school", project: null });
    filterBar(root, tasks, { tag: "school", project: "Kairo" }, change);
    const pressed = [...root.querySelectorAll<HTMLButtonElement>('.label-chip[aria-pressed="true"]')].map((c) => c.textContent);
    expect(pressed).toEqual(["Kairo", "#school"]);
    root.querySelector<HTMLButtonElement>('.label-chip[aria-pressed="true"].tag')!.click();
    expect(change).toHaveBeenLastCalledWith({ tag: null, project: "Kairo" });
    root.querySelector<HTMLButtonElement>(".label-clear")!.click();
    expect(change).toHaveBeenLastCalledWith(NO_FILTER);
  });

  it("hides itself when nothing has a tag or project, and keeps a chosen one offered", () => {
    filterBar(root, [task("a")], NO_FILTER, vi.fn());
    expect(root.hidden).toBe(true);
    filterBar(root, [task("a")], { tag: "gone", project: null }, vi.fn());
    expect(root.hidden).toBe(false);
    expect(root.querySelector(".label-chip")?.textContent).toBe("#gone");
  });
});

describe("the panel's project and tags", () => {
  let act: { labels: ReturnType<typeof vi.fn>; newProject: ReturnType<typeof vi.fn> };
  const draw = (t: Task, projects = ["Kairo", "Home"]) => root.replaceChildren(...labelsBlock(t, projects, act));
  const key = (el: HTMLElement, k: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));

  beforeEach(() => {
    act = { labels: vi.fn(), newProject: vi.fn() };
  });

  it("offers the projects Chrono has, and sets or clears one", () => {
    draw(task("a", { tags: ["x"], project: "Home" }));
    const select = root.querySelector<HTMLSelectElement>(".project-select")!;
    expect([...select.options].map((o) => o.textContent)).toEqual(["No project", "Kairo", "Home", "New project…"]);
    expect(select.value).toBe("Home");
    select.value = "Kairo";
    select.dispatchEvent(new Event("change"));
    expect(act.labels).toHaveBeenLastCalledWith("a", ["x"], "Kairo");
    select.value = "";
    select.dispatchEvent(new Event("change"));
    expect(act.labels).toHaveBeenLastCalledWith("a", ["x"], null);
  });

  it("makes a new project from a name typed in the select's place, and Esc gives the select back", () => {
    draw(task("a", { tags: ["x"] }));
    const select = root.querySelector<HTMLSelectElement>(".project-select")!;
    select.value = select.options[select.options.length - 1].value;
    select.dispatchEvent(new Event("change"));
    expect(root.querySelector(".project-select")).toBeNull();
    const name = root.querySelector<HTMLInputElement>(".project-new")!;
    expect(document.activeElement).toBe(name);
    key(name, "Enter");
    expect(act.newProject).not.toHaveBeenCalled();
    name.value = "  Thesis ";
    key(name, "Enter");
    expect(act.newProject).toHaveBeenCalledWith("a", "Thesis");
    key(name, "Escape");
    expect(root.querySelector(".project-select")).not.toBeNull();
    expect(root.querySelector(".project-new")).toBeNull();
  });

  it("adds tags on Enter or comma, cleaned and without repeats, keeping the keyboard for the next", () => {
    draw(task("a", { tags: ["school"] }));
    const add = root.querySelector<HTMLInputElement>(".tag-add")!;
    add.value = "#Urgent, school home";
    key(add, "Enter");
    expect(act.labels).toHaveBeenLastCalledWith("a", ["school", "urgent", "home"], null);
    draw(task("a", { tags: ["school", "urgent", "home"] }));
    expect(takeLabelFocus(root, "a")).toBe(true);
    expect(document.activeElement).toBe(root.querySelector(".tag-add"));
    expect(takeLabelFocus(root, "a")).toBe(false);
  });

  it("adds nothing for words already there or nothing typed", () => {
    draw(task("a", { tags: ["school"] }));
    const add = root.querySelector<HTMLInputElement>(".tag-add")!;
    add.value = "School";
    key(add, "Enter");
    key(add, ",");
    expect(act.labels).not.toHaveBeenCalled();
    expect(add.value).toBe("");
  });

  it("removes a tag with its button, and the last with Backspace in an empty field", () => {
    draw(task("a", { tags: ["one", "two"], project: "Kairo" }));
    root.querySelectorAll<HTMLButtonElement>(".tag-del")[0].click();
    expect(act.labels).toHaveBeenLastCalledWith("a", ["two"], "Kairo");
    key(root.querySelector<HTMLInputElement>(".tag-add")!, "Backspace");
    expect(act.labels).toHaveBeenLastCalledWith("a", ["one"], "Kairo");
  });

  it("stops at the limit and says so", () => {
    const tags = Array.from({ length: MAX_TAGS }, (_, i) => `t${i}`);
    draw(task("a", { tags }));
    const add = root.querySelector<HTMLInputElement>(".tag-add")!;
    expect(add.disabled).toBe(true);
    expect(add.placeholder).toContain(String(MAX_TAGS));
  });
});
