import type { Task } from "./api";
import { isoDate, parseDate, shiftDate, todayIso } from "./calendar";
import { totals, totalsLine, weekStart } from "./ledger-done";
import { remindTomorrow } from "./ledger-plan";
import { h } from "./ui";

// Ledger's Review view: the week looked back on and the next looked ahead to, on one page. What
// got done (Done's own totals), what keeps slipping, and what next week is for. Nothing here is
// stored but the week marked reviewed; what is moved is moved with Plan's own action.

const DAY_MS = 86_400_000;
/// From this hour on a Monday, the week just ended is over and the new one is the one in hand.
const MONDAY_LOOK_BACK_UNTIL = 17;
/// An open task has slipped once it has been moved to a later day this many times, or is overdue.
export const SLIP_MOVES = 2;
/// How many tasks one review picks for next week, and how many it offers to pick from.
export const MAX_PICKS = 3;
const OFFERED = 8;
/// How many candidates show before "Show more".
const FIRST_OFFERED = 3;

/// The Monday ("2026-11-02") of the week being reviewed at `ms`: the week going on, and on
/// Monday morning the one that just ended.
export function reviewedWeek(ms: number): string {
  const now = new Date(ms);
  const thisWeek = weekStart(ms);
  const lookBack = now.getDay() === 1 && now.getHours() < MONDAY_LOOK_BACK_UNTIL;
  return isoDate(new Date(lookBack ? weekStart(thisWeek - DAY_MS / 2) : thisWeek));
}

/// Whether the week in hand at `ms` has been reviewed: the latest week marked reviewed is that
/// week or a later one.
export function reviewedNow(reviewed: string | null, ms: number): boolean {
  return reviewed !== null && reviewed >= reviewedWeek(ms);
}

/// Where the keyboard was in the review: the button's place in its row, the row's place on the page
/// and what it said, so the page can be rebuilt and the keyboard put back near it.
type Held = { row: number; at: number; text: string };

function heldButton(body: HTMLElement): Held | null {
  const el = document.activeElement;
  if (!(el instanceof HTMLButtonElement) || !body.contains(el)) return null;
  const rows = Array.from(body.querySelectorAll(".review-row"));
  const row = el.closest(".review-row");
  return { row: row ? rows.indexOf(row) : -1, at: row ? Array.from(row.querySelectorAll("button")).indexOf(el) : 0, text: el.textContent ?? "" };
}

/// The same button if it is still there; else the same place in the row now at that spot (the task
/// that moved or went leaves one behind), else a button that says the same, else the first.
function putBack(body: HTMLElement, held: Held): void {
  const buttons = Array.from(body.querySelectorAll("button"));
  const rows = Array.from(body.querySelectorAll(".review-row"));
  const inRow = held.row >= 0 ? rows[Math.min(held.row, rows.length - 1)]?.querySelectorAll("button") : undefined;
  const sameRow = inRow && Array.from(inRow).find((b) => b.textContent === held.text);
  const there = sameRow ?? (inRow && inRow[Math.min(held.at, inRow.length - 1)]) ?? buttons.find((b) => b.textContent === held.text) ?? buttons[0];
  there?.focus();
}

/// The open tasks that slipped, the most moved first.
export function slippedTasks(tasks: Task[]): Task[] {
  return tasks.filter((t) => t.doneMs === null && (t.group === "overdue" || t.moved >= SLIP_MOVES)).sort((a, b) => b.moved - a.moved);
}

/// Why a task slipped, as its row says it: "Overdue", "Moved 3 times", "Overdue · moved 2 times".
export function slipLine(t: Task): string {
  const parts: string[] = [];
  if (t.group === "overdue") parts.push("Overdue");
  if (t.moved >= SLIP_MOVES) parts.push(`${parts.length ? "moved" : "Moved"} ${t.moved} times`);
  return parts.join(" · ");
}

/// Monday to Friday of the week after the one reviewed (given by its Monday).
export function nextWeekDays(monday: string): string[] {
  return [7, 8, 9, 10, 11].map((n) => shiftDate(monday, n));
}

/// The open tasks already due in the week after the one reviewed, Monday to Sunday.
export function plannedNext(tasks: Task[], monday: string): Task[] {
  const [from, to] = [shiftDate(monday, 7), shiftDate(monday, 13)];
  return tasks.filter((t) => t.doneMs === null && t.dueDay !== null && t.dueDay >= from && t.dueDay <= to);
}

/// Open tasks to pick for next week: not already due in it, not picked, those with `query` in the
/// name, a few of them.
export function pickable(tasks: Task[], monday: string, query: string, picked: string[]): Task[] {
  const planned = new Set(plannedNext(tasks, monday).map((t) => t.id));
  const q = query.trim().toLowerCase();
  return tasks.filter((t) => t.doneMs === null && !planned.has(t.id) && !picked.includes(t.id) && t.title.toLowerCase().includes(q)).slice(0, OFFERED);
}

/// The next working day after `iso`: tomorrow Monday to Thursday, Monday after that.
export function nextWorkday(iso: string): string {
  const weekday = parseDate(iso).getDay();
  return shiftDate(iso, weekday === 5 ? 3 : weekday === 6 ? 2 : 1);
}

/// What the Review view asks the page to do.
export type ReviewActions = {
  /** Gives a task another due day (null for none), with its Undo. */
  move(id: string, due: string | null, remind: number | null): void;
  /** Deletes a task, with its Undo. */
  remove(id: string): void;
  openTask(id: string): void;
  /** Opens Chrono on the week's own page. */
  chrono(monday: string): void;
  /** Marks the week reviewed. */
  finish(monday: string): Promise<void>;
};

export type ReviewView = {
  /** Draws the page for these tasks and the week last reviewed, as the list changed or the view opened. */
  render(tasks: Task[], reviewed: string | null): void;
};

/// "Mon 9 Nov", a day with its month, where "this week or next" has to be clear.
const longDay = (iso: string) => `${dayName(iso)} ${parseDate(iso).toLocaleDateString([], { month: "short" })}`;

/// "Mon 9", the short name of a day.
const dayName = (iso: string) => {
  const d = parseDate(iso);
  return `${d.toLocaleDateString([], { weekday: "short" })} ${d.getDate()}`;
};

export function mountReview(root: HTMLElement, act: ReviewActions, now: () => number = Date.now): ReviewView {
  let tasks: Task[] = [];
  let reviewed: string | null = null;
  let query = "";
  let picks: string[] = [];
  let pickedWeek = "";
  /// Which day of next week (0 is Monday) the picks go to, and whether the rest of the candidates show.
  let pickDay = 0;
  let expanded = false;

  const said = h("p", { class: "sr-only", role: "status" });
  const say = (text: string) => {
    said.textContent = "";
    said.textContent = text;
  };
  const search = h("input", { class: "filter review-search", type: "search", placeholder: "Find a task for next week", "aria-label": "Find a task for next week", autocomplete: "off" }) as HTMLInputElement;
  search.addEventListener("input", () => {
    query = search.value;
    draw();
  });
  const body = h("div", { class: "review" });
  root.replaceChildren(said, body);
  // Up and down walk the rows, on the same button of the next (the last it has, in a row with
  // fewer), so a page of three or four buttons a row isn't forty Tab stops.
  body.addEventListener("keydown", (e) => {
    if ((e.key !== "ArrowDown" && e.key !== "ArrowUp") || e.defaultPrevented) return;
    const here = (e.target as HTMLElement).closest<HTMLElement>(".review-row");
    if (!here || (e.target as HTMLElement).tagName !== "BUTTON") return;
    const rows = Array.from(body.querySelectorAll<HTMLElement>(".review-row"));
    const there = rows[rows.indexOf(here) + (e.key === "ArrowDown" ? 1 : -1)];
    if (!there) return;
    e.preventDefault();
    const from = Array.from(here.querySelectorAll("button")).indexOf(e.target as HTMLButtonElement);
    const buttons = there.querySelectorAll("button");
    buttons[Math.min(from, buttons.length - 1)]?.focus();
  });

  // The accessible name starts with what the button says, so voice control can say what it sees; the
  // title after it tells rows with the same button apart.
  const button = (label: string, aria: string | null, onClick: () => void, cls = "btn") => {
    const b = h("button", { class: cls, type: "button", ...(aria ? { "aria-label": aria } : {}) }, label) as HTMLButtonElement;
    b.addEventListener("click", onClick);
    return b;
  };

  function draw() {
    // Noted before the field is moved into the new page, which drops the keyboard from it.
    const typing = document.activeElement === search;
    const caret = search.selectionStart;
    const t = now();
    const monday = reviewedWeek(t);
    // Another week: nothing picked in this review carries over.
    if (monday !== pickedWeek) {
      pickedWeek = monday;
      picks = [];
      pickDay = 0;
      expanded = false;
    }
    const start = parseDate(monday).getTime();
    const mine = totals(tasks, start, start + 7 * DAY_MS);
    const before = totals(tasks, start - 7 * DAY_MS, start);
    const sections: HTMLElement[] = [];

    // On Monday morning the week in hand is the one that just ended, and is said so.
    const ended = monday !== isoDate(new Date(weekStart(t)));
    const thisWeek = ended ? "Last week" : "This week";
    const priorWeek = ended ? "the week before" : "last week";

    // 1. The week.
    const week = h("section", { class: "review-step", "aria-labelledby": "review-h-week" }, h("h2", { id: "review-h-week", class: "plan-head" }, "The week"));
    if (mine.done === 0 && before.done === 0) week.append(h("p", { class: "muted" }, `Nothing was finished ${ended ? "last week" : "this week"}.`));
    else week.append(h("p", {}, h("strong", {}, `${thisWeek} `), totalsLine(mine), h("span", { class: "muted" }, ` · ${priorWeek} ${totalsLine(before)}`)));
    week.append(button("The week in Chrono", null, () => act.chrono(monday), "btn link"));
    sections.push(week);

    // 2. What slipped.
    const slipped = slippedTasks(tasks);
    const slip = h("section", { class: "review-step", "aria-labelledby": "review-h-slip" }, h("h2", { id: "review-h-slip", class: "plan-head" }, "What slipped"));
    if (!slipped.length) slip.append(h("p", { class: "muted" }, "Nothing slipped."));
    else {
      slip.append(h("p", { class: "muted" }, `Overdue, or moved to a later day ${SLIP_MOVES === 2 ? "twice" : `${SLIP_MOVES} times`} or more.`));
      const day = nextWorkday(todayIso());
      const rows = slipped.map((s) => {
        const why = h("span", { class: "muted review-why", id: `review-why-${s.id}` }, slipLine(s));
        const open = button(s.title, `Open “${s.title}”`, () => act.openTask(s.id), "review-title");
        // The reason is read with the task's name.
        open.setAttribute("aria-describedby", why.id);
        const doIt = button(`Do it ${dayName(day)}`, `Do it ${dayName(day)}: “${s.title}”`, () => {
          say(`Put ${s.title} on ${dayName(day)}.`);
          act.move(s.id, day, day === shiftDate(todayIso(), 1) ? remindTomorrow(s.remindMs) : null);
        });
        // What it does: the task stays, with no day.
        const noDay = button("No day", `No day: “${s.title}”`, () => {
          say(`${s.title} has no day now.`);
          act.move(s.id, null, null);
        });
        const del = button("Delete", `Delete “${s.title}”`, () => {
          say(`Deleted ${s.title}. Undo is on the page.`);
          act.remove(s.id);
        }, "btn review-delete");
        return h("li", { class: "review-row", "data-task": s.id }, open, why, h("span", { class: "review-acts" }, doIt, noDay, del));
      });
      slip.append(h("ul", { class: "review-list" }, ...rows));
    }
    sections.push(slip);

    // 3. Next week.
    const planned = plannedNext(tasks, monday);
    const next = h("section", { class: "review-step", "aria-labelledby": "review-h-next" }, h("h2", { id: "review-h-next", class: "plan-head" }, "Next week"));
    const days = nextWeekDays(monday);
    next.append(h("p", { class: "muted" }, `Next week starts ${longDay(days[0])}.`));
    next.append(h("p", {}, planned.length ? `${planned.length} ${planned.length === 1 ? "task is" : "tasks are"} already due.` : "Nothing is due yet."));
    if (planned.length) next.append(h("ul", { class: "review-planned muted" }, ...planned.map((p) => h("li", {}, `${p.title} · ${dayName(p.dueDay!)}`))));
    if (picks.length >= MAX_PICKS) next.append(h("p", { class: "muted" }, "Three picked. That's a good week."));
    else {
      next.append(h("p", { class: "muted" }, `Pick up to ${MAX_PICKS - picks.length} to make the week's focus, all for the day chosen here.`));
      // The day is chosen once, for every pick.
      const chosen = days[Math.min(pickDay, days.length - 1)];
      const chips = days.map((d, i) => {
        const chip = button(dayName(d), null, () => {
          pickDay = i;
          draw();
        }, `btn chip-btn${d === chosen ? " on" : ""}`);
        chip.setAttribute("aria-pressed", String(d === chosen));
        return chip;
      });
      next.append(h("div", { class: "review-days", role: "group", "aria-label": "Day for the picks" }, ...chips), search);
      const found = pickable(tasks, monday, query, picks);
      const shownFound = expanded ? found : found.slice(0, FIRST_OFFERED);
      const rows = shownFound.map((p) => {
        const pick = button("Pick", `Pick “${p.title}” for ${dayName(chosen)}`, () => {
          picks = [...picks, p.id];
          say(`Put ${p.title} on ${dayName(chosen)}.`);
          act.move(p.id, chosen, null);
        }, "btn review-pick");
        return h("li", { class: "review-row" }, button(p.title, `Open “${p.title}”`, () => act.openTask(p.id), "review-title"), h("span", { class: "review-acts" }, pick));
      });
      next.append(rows.length ? h("ul", { class: "review-list" }, ...rows) : h("p", { class: "muted" }, query.trim() ? "No open task has that name." : "No open tasks to pick from."));
      if (!expanded && found.length > FIRST_OFFERED) {
        next.append(button(`Show ${found.length - FIRST_OFFERED} more`, `Show ${found.length - FIRST_OFFERED} more tasks to pick from`, () => {
          expanded = true;
          draw();
        }, "btn link"));
      }
    }
    sections.push(next);

    // 4. Finish.
    const finish = h("section", { class: "review-step", "aria-labelledby": "review-h-finish" });
    const total = planned.length;
    if (reviewed !== null && reviewed >= monday) {
      finish.append(h("h2", { id: "review-h-finish", class: "plan-head" }, "Reviewed"), h("p", { class: "muted" }, total ? `Next week has ${total} ${total === 1 ? "thing" : "things"}.` : "Next week is open."));
    } else {
      // When the one before was, so a skipped week is seen, not guessed.
      const before = reviewed ? parseDate(reviewed) : null;
      finish.append(h("h2", { id: "review-h-finish", class: "plan-head" }, "Finish"));
      if (before) finish.append(h("p", { class: "muted" }, `Last reviewed the week of ${before.getDate()} ${before.toLocaleDateString([], { month: "short" })}.`));
      finish.append(button("Done reviewing", null, () => void act.finish(monday).then(() => say("Week reviewed.")), "btn primary"));
    }
    sections.push(finish);

    // Noted before the page is rebuilt, which drops the keyboard from a button that was on it.
    const held = heldButton(body);
    // The field stays the same element, but moving it drops the keyboard from it.
    body.replaceChildren(...sections);
    if (typing && search.isConnected) {
      search.focus();
      search.setSelectionRange(caret, caret);
    } else if (held) putBack(body, held);
  }

  return {
    render(next, was) {
      tasks = next;
      reviewed = was;
      draw();
    },
  };
}
