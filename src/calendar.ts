/// The month view Kairo uses in place of the native date picker, which ignores the app's theme:
/// Chrono's day pickers and Ledger's due day and reminder.
import type { DayTotal } from "./api";
import { fmtDur, h } from "./ui";

const pad2 = (n: number) => String(n).padStart(2, "0");
/// Local `YYYY-MM-DD`; `toISOString` would give the UTC date instead.
export const isoDate = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
export const parseDate = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
};
export const todayIso = () => isoDate(new Date());
export const shiftDate = (iso: string, days: number) => {
  const d = parseDate(iso);
  d.setDate(d.getDate() + days);
  return isoDate(d);
};

export interface CalendarOptions {
  /// Positioned element the popover opens under.
  anchor: HTMLElement;
  /// Opens the calendar; focus returns here when it closes.
  trigger: HTMLButtonElement;
  label: string;
  /// The picked day, or null for none (the calendar then opens on today).
  selected: () => string | null;
  min: () => string | null;
  max: () => string | null;
  onPick: (iso: string) => void;
  /// Time tracked on each day from `from` to `to`, for a dot under the day that deepens with
  /// hours on screen. Without it, days carry no dots.
  totals?: (from: string, to: string) => Promise<DayTotal[]>;
}

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const HOUR_MS = 3_600_000;
/// The steps of the `seq-*` ramp the dots are coloured from.
const RAMP_STEPS = 7;
/// Hours on screen that earn the deepest dot.
const FULL_HOURS = 8;
const LIGHTEST_STEP = 3;

export const monthStart = (iso: string) => `${iso.slice(0, 7)}-01`;
const monthEnd = (iso: string) => {
  const d = parseDate(iso);
  return isoDate(new Date(d.getFullYear(), d.getMonth() + 1, 0));
};
/// The same day of another month, or that month's last day when it's shorter.
export const shiftMonthKeepDay = (iso: string, months: number) => {
  const d = parseDate(iso);
  const target = new Date(d.getFullYear(), d.getMonth() + months, 1);
  target.setDate(Math.min(d.getDate(), new Date(target.getFullYear(), target.getMonth() + 1, 0).getDate()));
  return isoDate(target);
};
export const weekdayIndex = (iso: string) => (parseDate(iso).getDay() + 6) % 7;

export function calendar(opts: CalendarOptions): { toggle(): void; close(): void } {
  const pop = h("div", { class: "calendar", role: "dialog", "aria-label": opts.label });
  pop.hidden = true;
  opts.anchor.append(pop);
  const totals = new Map<string, DayTotal>();
  const fetched = new Set<string>();
  let month = "";
  let focused = "";

  const clamp = (iso: string) => {
    const min = opts.min();
    const max = opts.max();
    return min && iso < min ? min : max && iso > max ? max : iso;
  };

  async function fetchMonth(m: string) {
    if (!opts.totals || fetched.has(m)) return;
    fetched.add(m);
    try {
      for (const t of await opts.totals(m, monthEnd(m))) totals.set(t.date, t);
    } catch {
      fetched.delete(m);
      return;
    }
    if (!pop.hidden && month === m) render(false);
  }

  function go(iso: string) {
    focused = clamp(iso);
    month = monthStart(focused);
    render(true);
    void fetchMonth(month);
  }

  function render(moveFocus: boolean) {
    const min = opts.min();
    const max = opts.max();
    const selected = opts.selected();
    const today = todayIso();
    const first = parseDate(month);
    const title = first.toLocaleDateString([], { month: "long", year: "numeric" });

    const prev = h("button", { type: "button", class: "cal-step", "aria-label": "Previous month" }, "‹") as HTMLButtonElement;
    const next = h("button", { type: "button", class: "cal-step", "aria-label": "Next month" }, "›") as HTMLButtonElement;
    prev.disabled = !!min && month <= monthStart(min);
    next.disabled = !!max && month >= monthStart(max);
    prev.addEventListener("click", () => go(shiftMonthKeepDay(focused, -1)));
    next.addEventListener("click", () => go(shiftMonthKeepDay(focused, 1)));

    const grid = h("div", { class: "cal-grid", role: "group", "aria-label": title });
    grid.append(...WEEKDAYS.map((d) => h("span", { class: "cal-weekday", "aria-hidden": "true" }, d)));
    for (let i = 0; i < weekdayIndex(month); i++) grid.append(h("span"));
    const days = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
    for (let n = 1; n <= days; n++) {
      const iso = isoDate(new Date(first.getFullYear(), first.getMonth(), n));
      const t = totals.get(iso);
      const day = h("button", { type: "button", class: "cal-day", "data-date": iso, tabindex: iso === focused ? "0" : "-1" }, String(n)) as HTMLButtonElement;
      day.disabled = (!!min && iso < min) || (!!max && iso > max);
      day.classList.toggle("selected", iso === selected);
      day.classList.toggle("today", iso === today);
      day.setAttribute("aria-pressed", String(iso === selected));
      const name = parseDate(iso).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
      day.setAttribute("aria-label", t ? `${name}, ${fmtDur(t.activeMs)} tracked` : name);
      if (t) {
        // The darkest blues vanish on the popover, so even a light day starts a few shades up.
        const share = Math.min(t.activeMs / HOUR_MS / FULL_HOURS, 1);
        const step = LIGHTEST_STEP + Math.round(share * (RAMP_STEPS - 1 - LIGHTEST_STEP));
        day.append(h("span", { class: `cal-dot seq-${step}`, "aria-hidden": "true" }));
      }
      day.addEventListener("click", () => {
        close();
        opts.onPick(iso);
      });
      grid.append(day);
    }

    // Redrawing replaces the focused button, so a redraw while the calendar has focus keeps it.
    const hadFocus = pop.contains(document.activeElement);
    pop.replaceChildren(h("div", { class: "cal-head" }, prev, h("span", { class: "cal-title" }, title), next), grid);
    if (moveFocus || hadFocus) pop.querySelector<HTMLButtonElement>(`[data-date="${focused}"]`)?.focus();
  }

  const onOutside = (e: PointerEvent) => {
    if (!opts.anchor.contains(e.target as Node)) close(false);
  };

  function open() {
    pop.hidden = false;
    document.addEventListener("pointerdown", onOutside, true);
    go(opts.selected() ?? todayIso());
  }

  function close(refocus = true) {
    if (pop.hidden) return;
    pop.hidden = true;
    document.removeEventListener("pointerdown", onOutside, true);
    if (refocus) opts.trigger.focus();
  }

  pop.addEventListener("keydown", (e) => {
    // The page's own arrow keys and Esc must not also fire while picking a date.
    e.stopPropagation();
    const moves: Record<string, () => string> = {
      ArrowLeft: () => shiftDate(focused, -1),
      ArrowRight: () => shiftDate(focused, 1),
      ArrowUp: () => shiftDate(focused, -7),
      ArrowDown: () => shiftDate(focused, 7),
      PageUp: () => shiftMonthKeepDay(focused, -1),
      PageDown: () => shiftMonthKeepDay(focused, 1),
      Home: () => shiftDate(focused, -weekdayIndex(focused)),
      End: () => shiftDate(focused, 6 - weekdayIndex(focused)),
    };
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (moves[e.key]) {
      e.preventDefault();
      go(moves[e.key]());
    }
  });

  return { toggle: () => (pop.hidden ? open() : close()), close: () => close(false) };
}
