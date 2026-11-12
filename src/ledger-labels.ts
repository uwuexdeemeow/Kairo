import type { Task } from "./api";
import { glyph } from "./glyphs";
import { fmtDur, h } from "./ui";

/// Tags a task may carry, and the longest one: what the backend keeps (tasks.rs).
export const MAX_TAGS = 8;
export const MAX_TAG_CHARS = 24;

/// What Ledger's lists are narrowed to: a tag, a project, both, or (null, null) everything.
export type Filter = { tag: string | null; project: string | null };
export const NO_FILTER: Filter = { tag: null, project: null };

export const isFiltered = (f: Filter) => f.tag !== null || f.project !== null;

/// Whether a task is among what `f` narrows to.
export function matches(t: Task, f: Filter): boolean {
  return (f.tag === null || t.tags.includes(f.tag)) && (f.project === null || t.project?.toLowerCase() === f.project.toLowerCase());
}

/// A tag as the backend keeps it: lowercase letters, digits, `-` and `_`; a leading # dropped.
export function cleanTag(text: string): string {
  return text.trim().replace(/^#+/, "").toLowerCase().replace(/[^\p{L}\p{N}_-]/gu, "").slice(0, MAX_TAG_CHARS);
}

/// The tags and projects the open tasks use, each in alphabetical order.
export function labelsInUse(tasks: Task[]): { tags: string[]; projects: string[] } {
  const tags = new Set<string>();
  // A project by its name in any case is one project: the first spelling seen.
  const projects = new Map<string, string>();
  for (const t of tasks) {
    if (t.doneMs !== null) continue;
    t.tags.forEach((x) => tags.add(x));
    if (t.project && !projects.has(t.project.toLowerCase())) projects.set(t.project.toLowerCase(), t.project);
  }
  const sort = (s: Iterable<string>) => [...s].sort((a, b) => a.localeCompare(b));
  return { tags: sort(tags), projects: sort(projects.values()) };
}

/// "Kairo #urgent · 5 open · 3h 20m focused" for what the lists are narrowed to; none when they
/// aren't.
export function filterSummary(tasks: Task[], f: Filter): string {
  if (!isFiltered(f)) return "";
  const mine = tasks.filter((t) => matches(t, f));
  const open = mine.filter((t) => t.doneMs === null).length;
  const focused = mine.reduce((sum, t) => sum + t.focusedMs, 0);
  const name = [f.project, f.tag && `#${f.tag}`].filter(Boolean).join(" ");
  return `${name} · ${open} open${focused >= 60_000 ? ` · ${fmtDur(focused)} focused` : ""}`;
}

/// The chips above a list: each project and tag in use, pressed to narrow the list to it (a
/// project and a tag together narrow to both); pressed again, to let it go. Empty when there are none.
export function filterBar(root: HTMLElement, tasks: Task[], f: Filter, change: (next: Filter) => void): void {
  const { tags, projects } = labelsInUse(tasks);
  // What's narrowed to stays offered, though no open task carries it now, so it can be let go.
  if (f.project && !projects.includes(f.project)) projects.push(f.project);
  if (f.tag && !tags.includes(f.tag)) tags.push(f.tag);
  root.hidden = !projects.length && !tags.length;
  const chip = (text: string, kind: "project" | "tag", on: boolean, next: Filter) => {
    const b = h("button", { class: `label-chip ${kind}`, type: "button", "aria-pressed": String(on) }, kind === "tag" ? `#${text}` : text);
    b.addEventListener("click", () => change(next));
    return b;
  };
  // Two kinds of thing, named as two groups: a project is where a task lives, a tag is a label on it.
  const group = (name: string, chips: HTMLElement[]): Node[] => (chips.length ? [h("span", { class: "label-group" }, name), ...chips] : []);
  const nodes: Node[] = [
    ...group("Project", projects.map((p) => chip(p, "project", f.project === p, { ...f, project: f.project === p ? null : p }))),
    ...group("Tag", tags.map((x) => chip(x, "tag", f.tag === x, { ...f, tag: f.tag === x ? null : x }))),
  ];
  if (isFiltered(f)) {
    const clear = h("button", { class: "link-btn label-clear", type: "button" }, "Show all");
    clear.addEventListener("click", () => change(NO_FILTER));
    nodes.push(clear);
  }
  root.replaceChildren(...nodes);
}

/// What the task panel's label controls ask the page to do.
export type LabelActions = {
  /** Gives a task its tags and project (null for none). */
  labels(task: string, tags: string[], project: string | null): void;
  /** Makes a Chrono project of that name, and puts the task in it. */
  newProject(task: string, name: string): void;
};

/// Where the keyboard goes once the panel is drawn again: the field to add a tag.
let next: string | null = null;

/// Puts the keyboard where it was promised for this task's panel; whether it did.
export function takeLabelFocus(root: HTMLElement, task: string): boolean {
  const want = next;
  next = null;
  if (want !== task) return false;
  const field = root.querySelector<HTMLElement>(".tag-add");
  field?.focus();
  return !!field;
}

const NEW_PROJECT = "\u0000new";

/// The panel's Project and Tags: a choice of the Chrono projects (or a new one), and the tags with
/// a field to add the next. `projects` are the names Chrono has.
export function labelsBlock(t: Task, projects: string[], act: LabelActions): [HTMLElement, HTMLElement] {
  const select = h("select", { class: "filter project-select", "aria-label": "Project" }) as HTMLSelectElement;
  select.append(h("option", { value: "" }, "No project"), ...projects.map((p) => h("option", { value: p }, p)), h("option", { value: NEW_PROJECT }, "New project…"));
  select.value = t.project ?? "";
  const projectField = h("div", { class: "detail-field detail-project" }, h("span", {}, "Project"), select);
  select.addEventListener("change", () => {
    if (select.value !== NEW_PROJECT) return act.labels(t.id, t.tags, select.value || null);
    // A name for the new one, in the select's place; Esc gives the select back.
    const name = h("input", { class: "filter project-new", maxlength: "80", placeholder: "Project name", "aria-label": "New project name", autocomplete: "off" }) as HTMLInputElement;
    select.replaceWith(name);
    name.focus();
    name.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && name.value.trim()) act.newProject(t.id, name.value.trim());
      else if (e.key === "Escape") {
        e.stopPropagation();
        select.value = t.project ?? "";
        name.replaceWith(select);
        select.focus();
      }
    });
  });

  const list = h("div", { class: "tag-list", role: "list", "aria-label": "Tags" });
  for (const tag of t.tags) {
    const remove = h("button", { class: "tag-del", type: "button", "aria-label": `Remove tag ${tag}`, title: "Remove tag" }, glyph("close"));
    remove.addEventListener("click", () => act.labels(t.id, t.tags.filter((x) => x !== tag), t.project));
    list.append(h("span", { class: "tag-chip", role: "listitem" }, `#${tag}`, remove));
  }
  const add = h("input", {
    class: "filter tag-add",
    maxlength: "40",
    placeholder: t.tags.length >= MAX_TAGS ? `Up to ${MAX_TAGS} tags` : "Add a tag",
    "aria-label": "Add a tag",
    autocomplete: "off",
  }) as HTMLInputElement;
  add.disabled = t.tags.length >= MAX_TAGS;
  const addTags = () => {
    const words = add.value.split(/[\s,]+/).map(cleanTag).filter(Boolean);
    const tags = [...new Set([...t.tags, ...words])].slice(0, MAX_TAGS);
    if (!words.length || tags.length === t.tags.length) {
      add.value = "";
      return;
    }
    next = t.id;
    act.labels(t.id, tags, t.project);
  };
  add.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addTags();
    } else if (e.key === "Backspace" && !add.value && t.tags.length) {
      // Backspace in an empty field takes the last tag back.
      e.preventDefault();
      next = t.id;
      act.labels(t.id, t.tags.slice(0, -1), t.project);
    }
  });
  const tagField = h("div", { class: "detail-field detail-tags" }, h("span", {}, "Tags"), list, add);
  return [projectField, tagField];
}
