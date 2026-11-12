import type { CalendarEvent, Task } from "./api";
import { isoDate, parseDate, shiftDate, todayIso } from "./calendar";
import { fmtClock, fmtDur, h } from "./ui";

/// What the Calendar view asks the page to do.
export type CalendarDeps = {
  /** Meetings from the connected calendars between two times. */
  events(fromMs: number, toMs: number): Promise<CalendarEvent[]>;
  /** Opens the task's panel in Tasks. */
  openTask(id: string): void;
  done(id: string, done: boolean): void;
  /** Adds a task from quick-add text, due on `day` unless the text names another. */
  add(text: string, day: string): void | Promise<void>;
};

export type CalendarView = {
  /** Draws the month for these tasks, as the list changed or the view opened. */
  render(tasks: Task[]): void;
};

/// Tasks shown in a day's cell before "+n more".
const CELL_TASKS = 2;

/// The Monday on or before `iso`.
export function weekStartOf(iso: string): string {
  const d = parseDate(iso);
  return shiftDate(iso, -((d.getDay() + 6) % 7));
}

/// The days a month's grid shows: whole weeks from the Monday on or before the 1st, to the Sunday
/// on or after the last day.
export function monthGrid(year: number, month: number): string[] {
  const first = isoDate(new Date(year, month, 1));
  const last = isoDate(new Date(year, month + 1, 0));
  const days: string[] = [];
  for (let day = weekStartOf(first); day <= weekStartOf(last) || days.length % 7 !== 0; day = shiftDate(day, 1)) days.push(day);
  return days;
}

/// The open tasks by their due day.
export function openByDay(tasks: Task[]): Map<string, Task[]> {
  const by = new Map<string, Task[]>();
  for (const t of tasks) {
    if (t.doneMs !== null || !t.dueDay) continue;
    by.set(t.dueDay, [...(by.get(t.dueDay) ?? []), t]);
  }
  return by;
}

/// A meeting's local day; one running past midnight is on the day it starts.
const eventDay = (e: CalendarEvent) => isoDate(new Date(e.startMs));

/// The Calendar view: a month of days, each with its tasks and meetings; a day chosen lists them,
/// and adds a task due on it.
export function mountCalendar(root: HTMLElement, deps: CalendarDeps, today: () => string = todayIso): CalendarView {
  let tasks: Task[] = [];
  let events: CalendarEvent[] = [];
  const first = parseDate(today());
  let year = first.getFullYear();
  let month = first.getMonth();
  let selected = today();
  let asked = 0;
  let refocus: string | null = null;

  const head = h("div", { class: "cal-head" });
  const grid = h("div", { class: "cal-grid", role: "grid" });
  const side = h("section", { class: "cal-day", "aria-live": "polite" });
  root.replaceChildren(head, h("div", { class: "cal-body" }, grid, side));

  const loadEvents = async () => {
    const days = monthGrid(year, month);
    const mine = ++asked;
    const found = await deps.events(parseDate(days[0]).getTime(), parseDate(shiftDate(days[days.length - 1], 1)).getTime()).catch(() => []);
    // Only the latest month asked for.
    if (mine !== asked) return;
    events = found;
    draw();
  };

  const go = (toYear: number, toMonth: number) => {
    const d = new Date(toYear, toMonth, 1);
    [year, month] = [d.getFullYear(), d.getMonth()];
    events = [];
    draw();
    void loadEvents();
  };

  /// Chooses a day, moving the month along when it's in another.
  const choose = (iso: string, focus = false) => {
    selected = iso;
    const d = parseDate(iso);
    if (focus) refocus = iso;
    if (d.getFullYear() !== year || d.getMonth() !== month) go(d.getFullYear(), d.getMonth());
    else draw();
  };

  const drawHead = () => {
    const title = parseDate(isoDate(new Date(year, month, 1))).toLocaleDateString([], { month: "long", year: "numeric" });
    const step = (label: string, by: number, text: string) => {
      const b = h("button", { class: "btn cal-step", type: "button", "aria-label": label }, text);
      b.addEventListener("click", () => go(year, month + by));
      return b;
    };
    const now = h("button", { class: "btn cal-today", type: "button" }, "Today");
    now.addEventListener("click", () => choose(today(), true));
    const undated = tasks.filter((t) => t.doneMs === null && !t.dueDay).length;
    head.replaceChildren(
      step("Previous month", -1, "‹"),
      h("h2", { class: "cal-title", "aria-live": "polite" }, title),
      step("Next month", 1, "›"),
      now,
      undated ? h("span", { class: "muted cal-undated" }, `${undated} open ${undated === 1 ? "task has" : "tasks have"} no due day`) : "",
    );
  };

  const drawGrid = () => {
    const days = monthGrid(year, month);
    const open = openByDay(tasks);
    const meetings = new Map<string, number>();
    for (const e of events) meetings.set(eventDay(e), (meetings.get(eventDay(e)) ?? 0) + 1);
    const t = today();
    const names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map((n) => h("div", { class: "cal-name", role: "columnheader" }, n));
    const cells = days.map((iso) => {
      const d = parseDate(iso);
      const due = open.get(iso) ?? [];
      const meets = meetings.get(iso) ?? 0;
      const label = [d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" }), due.length ? `${due.length} ${due.length === 1 ? "task" : "tasks"}` : "", meets ? `${meets} ${meets === 1 ? "meeting" : "meetings"}` : ""].filter(Boolean).join(", ");
      const cell = h(
        "button",
        {
          class: `cal-cell${d.getMonth() !== month ? " other" : ""}${iso === t ? " today" : ""}${iso === selected ? " selected" : ""}${due.length && iso < t ? " late" : ""}`,
          type: "button",
          role: "gridcell",
          "data-day": iso,
          "aria-label": label,
          "aria-selected": String(iso === selected),
          tabindex: iso === selected ? "0" : "-1",
        },
        h("span", { class: "cal-num", "aria-hidden": "true" }, String(d.getDate())),
      );
      const inner = h("span", { class: "cal-items", "aria-hidden": "true" });
      for (const task of due.slice(0, CELL_TASKS)) inner.append(h("span", { class: "cal-task" }, task.title));
      if (due.length > CELL_TASKS) inner.append(h("span", { class: "cal-more" }, `+${due.length - CELL_TASKS} more`));
      if (meets) inner.append(h("span", { class: "cal-meet" }, `${meets} ${meets === 1 ? "meeting" : "meetings"}`));
      cell.append(inner);
      cell.addEventListener("click", () => choose(iso));
      return cell;
    });
    grid.replaceChildren(...names, ...cells);
  };

  grid.addEventListener("keydown", (e) => {
    const by = ({ ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 } as Record<string, number>)[e.key];
    if (by !== undefined) {
      e.preventDefault();
      choose(shiftDate(selected, by), true);
    } else if (e.key === "PageUp" || e.key === "PageDown") {
      e.preventDefault();
      const d = parseDate(selected);
      const to = new Date(d.getFullYear(), d.getMonth() + (e.key === "PageUp" ? -1 : 1), 1);
      // The same day of that month, or its last.
      const last = new Date(to.getFullYear(), to.getMonth() + 1, 0).getDate();
      choose(isoDate(new Date(to.getFullYear(), to.getMonth(), Math.min(d.getDate(), last))), true);
    }
  });

  const drawDay = () => {
    const title = parseDate(selected).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
    const mine = tasks.filter((t) => t.dueDay === selected).sort((a, b) => Number(a.doneMs !== null) - Number(b.doneMs !== null) || b.priority - a.priority || a.position - b.position);
    const meetings = events.filter((e) => eventDay(e) === selected);
    const nodes: Node[] = [h("h3", { class: "cal-day-title" }, title)];

    const list = h("ul", { class: "cal-tasks", "aria-label": `Tasks due ${title}` });
    for (const t of mine) {
      const done = t.doneMs !== null;
      const tick = h("input", { type: "checkbox", "aria-label": done ? `Reopen “${t.title}”` : `Mark “${t.title}” done` }) as HTMLInputElement;
      tick.checked = done;
      tick.addEventListener("change", () => deps.done(t.id, !done));
      const name = h("button", { class: `cal-task-name${done ? " done" : ""}`, type: "button", title: "Details" }, t.title);
      name.addEventListener("click", () => deps.openTask(t.id));
      const meta = [t.project, ...t.tags.map((x) => `#${x}`), t.focusedMs >= 60_000 ? fmtDur(t.focusedMs) : ""].filter(Boolean).join(" · ");
      list.append(h("li", { class: "cal-task-row" }, tick, name, meta ? h("span", { class: "muted cal-task-meta" }, meta) : ""));
    }
    if (mine.length) nodes.push(list);
    else nodes.push(h("p", { class: "muted cal-empty" }, "No tasks due this day."));

    if (meetings.length) {
      const meets = h("ul", { class: "cal-meetings", "aria-label": `Meetings ${title}` });
      for (const e of meetings) meets.append(h("li", {}, h("span", { class: "muted cal-meet-time" }, `${fmtClock(e.startMs)}–${fmtClock(e.endMs)}`), e.title));
      nodes.push(h("h4", { class: "cal-sub" }, "Meetings"), meets);
    }

    const add = h("input", { class: "filter cal-add", placeholder: "Add a task for this day", "aria-label": `Add a task for ${title}`, maxlength: "200", autocomplete: "off" }) as HTMLInputElement;
    add.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter" || !add.value.trim()) return;
      e.preventDefault();
      const text = add.value;
      add.value = "";
      try {
        await deps.add(text, selected);
      } catch {
        // Kept for another try, rather than lost.
        add.value = text;
      }
    });
    nodes.push(add);
    // A redraw (the list refreshing) leaves what's being typed, and the keyboard in it, alone.
    const typing = side.querySelector<HTMLInputElement>(".cal-add");
    const keep = typing && typing.dataset.day === selected ? { value: typing.value, focused: document.activeElement === typing } : null;
    add.dataset.day = selected;
    side.replaceChildren(...nodes);
    if (keep) {
      add.value = keep.value;
      if (keep.focused) add.focus();
    }
  };

  const draw = () => {
    const hadFocus = grid.contains(document.activeElement);
    drawHead();
    drawGrid();
    drawDay();
    if (refocus || hadFocus) {
      grid.querySelector<HTMLElement>(`[data-day="${refocus ?? selected}"]`)?.focus();
      refocus = null;
    }
  };

  void loadEvents();
  return {
    render: (all) => {
      tasks = all;
      draw();
    },
  };
}
