import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { api, type Category, type CategoryEntry, type DayGoals, type DaySummary, type DayTotal, type FocusBlock, type FocusSession, type Week, type DeleteRange, type GoalSettings, type Patterns, type PrivacyMode, type Usage } from "./api";

const CATEGORIES: Category[] = ["productive", "neutral", "distracting"];
const LABELS: Record<Category, string> = { productive: "Productive", neutral: "Neutral", distracting: "Distracting" };
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const RAMP_STEPS = 7;
/// Heatmap cells with less tracked time than this are too thin to colour.
const MIN_CELL_MS = 15 * 60_000;
const REFRESH_MS = 30_000;
const STATUS_POLL_MS = 10_000;
const HOUR_MS = 3_600_000;

// ---------- helpers ----------

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;

function h(tag: string, attrs: Record<string, string> = {}, ...children: (Node | string)[]): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  el.append(...children);
  return el;
}

const SVG_NS = "http://www.w3.org/2000/svg";
function s(tag: string, attrs: Record<string, string | number> = {}): SVGElement {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
}

function fmtDur(ms: number): string {
  const mins = Math.round(ms / 60_000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}m`;
}

function fmtClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function fmtHour(hour: number): string {
  const h24 = ((hour % 24) + 24) % 24;
  const n = h24 % 12 === 0 ? 12 : h24 % 12;
  return `${n}${h24 < 12 ? "am" : "pm"}`;
}

/// "14:32" for minutes:seconds under an hour, "1:05:09" above.
function fmtElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const [hh, mm, ss] = [Math.floor(total / 3600), Math.floor((total % 3600) / 60), total % 60];
  const pad = (n: number) => String(n).padStart(2, "0");
  return hh ? `${hh}:${pad(mm)}:${pad(ss)}` : `${pad(mm)}:${pad(ss)}`;
}

const pct = (part: number, whole: number) => (whole ? Math.round((part / whole) * 100) : 0);

// Labels come from window titles, which any web page controls, so escape before using innerHTML.
const esc = (text: string) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

const pad2 = (n: number) => String(n).padStart(2, "0");
/// Local `YYYY-MM-DD`; `toISOString` would give the UTC date instead.
const isoDate = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
const parseDate = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
};
const todayIso = () => isoDate(new Date());
const shiftDate = (iso: string, days: number) => {
  const d = parseDate(iso);
  d.setDate(d.getDate() + days);
  return isoDate(d);
};

// ---------- tooltip ----------

const tooltip = $("#tooltip");

function tip(target: Element, html: () => string) {
  target.addEventListener("pointerenter", () => {
    tooltip.innerHTML = html();
    tooltip.hidden = false;
  });
  target.addEventListener("pointermove", (e) => {
    const ev = e as PointerEvent;
    const { width, height } = tooltip.getBoundingClientRect();
    const x = Math.min(ev.clientX + 14, window.innerWidth - width - 8);
    const y = ev.clientY + 16 + height > window.innerHeight ? ev.clientY - height - 12 : ev.clientY + 16;
    tooltip.style.transform = `translate(${x}px, ${y}px)`;
  });
  target.addEventListener("pointerleave", () => (tooltip.hidden = true));
}

const dotHtml = (c: Category) => `<span class="dot ${c}"></span>`;

function renderLegends() {
  for (const el of document.querySelectorAll<HTMLElement>("[data-legend]")) {
    el.replaceChildren(...CATEGORIES.map((c) => h("span", { class: "legend-item" }, h("span", { class: `dot ${c}` }), LABELS[c])));
  }
}

// ---------- calendar ----------

interface CalendarOptions {
  /// Positioned element the popover opens under.
  anchor: HTMLElement;
  /// Opens the calendar; focus returns here when it closes.
  trigger: HTMLButtonElement;
  label: string;
  selected: () => string;
  min: () => string | null;
  max: () => string;
  onPick: (iso: string) => void;
}

/// Hours on screen that earn the deepest dot.
const CAL_FULL_HOURS = 8;
const CAL_LIGHTEST_STEP = 3;

const monthStart = (iso: string) => `${iso.slice(0, 7)}-01`;
const monthEnd = (iso: string) => {
  const d = parseDate(iso);
  return isoDate(new Date(d.getFullYear(), d.getMonth() + 1, 0));
};
/// The same day of another month, or that month's last day when it's shorter.
const shiftMonthKeepDay = (iso: string, months: number) => {
  const d = parseDate(iso);
  const target = new Date(d.getFullYear(), d.getMonth() + months, 1);
  target.setDate(Math.min(d.getDate(), new Date(target.getFullYear(), target.getMonth() + 1, 0).getDate()));
  return isoDate(target);
};
const weekdayIndex = (iso: string) => (parseDate(iso).getDay() + 6) % 7;

/// A month view in place of the native date picker, which ignores the app's theme. Days with
/// tracked time carry a dot that deepens with hours on screen.
function calendar(opts: CalendarOptions) {
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
    return min && iso < min ? min : iso > max ? max : iso;
  };

  async function fetchMonth(m: string) {
    if (fetched.has(m)) return;
    fetched.add(m);
    try {
      for (const t of await api.dayTotals(m, monthEnd(m))) totals.set(t.date, t);
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
    fetchMonth(month);
  }

  function render(moveFocus: boolean) {
    const min = opts.min();
    const max = opts.max();
    const selected = opts.selected();
    const today = todayIso();
    const first = parseDate(month);
    const title = first.toLocaleDateString([], { month: "long", year: "numeric" });

    const prev = h("button", { class: "cal-step", "aria-label": "Previous month" }, "‹") as HTMLButtonElement;
    const next = h("button", { class: "cal-step", "aria-label": "Next month" }, "›") as HTMLButtonElement;
    prev.disabled = !!min && month <= monthStart(min);
    next.disabled = month >= monthStart(max);
    prev.addEventListener("click", () => go(shiftMonthKeepDay(focused, -1)));
    next.addEventListener("click", () => go(shiftMonthKeepDay(focused, 1)));

    const grid = h("div", { class: "cal-grid", role: "group", "aria-label": title });
    grid.append(...WEEKDAYS.map((d) => h("span", { class: "cal-weekday", "aria-hidden": "true" }, d.slice(0, 2))));
    for (let i = 0; i < weekdayIndex(month); i++) grid.append(h("span"));
    const days = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
    for (let n = 1; n <= days; n++) {
      const iso = isoDate(new Date(first.getFullYear(), first.getMonth(), n));
      const t = totals.get(iso);
      const day = h("button", { class: "cal-day", "data-date": iso, tabindex: iso === focused ? "0" : "-1" }, String(n)) as HTMLButtonElement;
      day.disabled = (!!min && iso < min) || iso > max;
      day.classList.toggle("selected", iso === selected);
      day.classList.toggle("today", iso === today);
      day.setAttribute("aria-pressed", String(iso === selected));
      const name = parseDate(iso).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
      day.setAttribute("aria-label", t ? `${name}, ${fmtDur(t.activeMs)} tracked` : name);
      if (t) {
        // The darkest blues vanish on the popover, so even a light day starts a few shades up.
        const share = Math.min(t.activeMs / HOUR_MS / CAL_FULL_HOURS, 1);
        const step = CAL_LIGHTEST_STEP + Math.round(share * (RAMP_STEPS - 1 - CAL_LIGHTEST_STEP));
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
    go(opts.selected());
  }

  function close(refocus = true) {
    if (pop.hidden) return;
    pop.hidden = true;
    document.removeEventListener("pointerdown", onOutside, true);
    if (refocus) opts.trigger.focus();
  }

  pop.addEventListener("keydown", (e) => {
    // The Today view's own arrow keys must not also fire while picking a date.
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

  return { toggle: () => (pop.hidden ? open() : close()) };
}

// ---------- today ----------

function stat(label: string, value: string, extra = ""): HTMLElement {
  const dd = h("dd", {}, value);
  if (extra) dd.append(h("span", {}, ` · ${extra}`));
  return h("div", {}, h("dt", {}, label), dd);
}

/// The day shown on the Today view; null follows the current day, including across midnight.
let selectedDay: string | null = null;
/// Earliest day with any recorded session, or null before anything is tracked.
let firstDay: string | null = null;

const shownDay = () => selectedDay ?? todayIso();

function renderDayNav() {
  const day = shownDay();
  const today = todayIso();
  const isToday = day === today;
  const date = parseDate(day);
  const sameYear = date.getFullYear() === new Date().getFullYear();
  $("#today-date").textContent = date.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", ...(sameYear ? {} : { year: "numeric" }) });
  $<HTMLButtonElement>("#day-prev").disabled = !firstDay || day <= firstDay;
  $<HTMLButtonElement>("#day-next").disabled = isToday;
  $("#day-today").hidden = isToday;
}

function goToDay(iso: string | null) {
  const today = todayIso();
  if (iso && firstDay && iso < firstDay) iso = firstDay;
  selectedDay = !iso || iso >= today ? null : iso;
  load("today");
}

function renderToday(d: DaySummary) {
  const isToday = selectedDay === null;
  renderDayNav();
  $("#today-active").textContent = d.activeMs ? fmtDur(d.activeMs) : "0m";
  const sub = $("#today-sub");
  sub.replaceChildren(d.activeMs ? "on screen, " : isToday ? "on screen so far" : "on screen", ...(d.activeMs ? [h("strong", {}, `${pct(d.productiveMs, d.activeMs)}% productive`)] : []));

  const perHour = d.activeMs ? Math.round(d.switches / (d.activeMs / HOUR_MS)) : 0;
  $("#today-stats").replaceChildren(
    stat("Longest focus", fmtDur(d.longestFocusMs)),
    stat("Deep-work blocks", String(d.deepBlocks)),
    stat("Distracted", fmtDur(d.distractingMs)),
    stat("Switches", String(d.switches), d.activeMs ? `${perHour}/h` : ""),
  );
  renderTimeline(d, isToday);
  if (isToday) renderHeadsUp();
  else $("#heads-up").hidden = true;
  renderUsage($("#today-apps"), d.topApps, isToday ? "No app activity yet today." : "No app activity this day.");
  renderUsage($("#today-sites"), d.topSites, isToday ? "No recognised sites yet today." : "No recognised sites this day.");
}

/// Focus sessions on the day the Today view shows, loaded alongside it.
let shownFocus: FocusBlock[] = [];

function renderTimeline(d: DaySummary, isToday: boolean) {
  const t0 = parseDate(shownDay()).getTime();
  const span = 24 * HOUR_MS;
  const strip = h("div", { class: "timeline", role: "img", "aria-label": `Activity on ${$("#today-date").textContent} from midnight to midnight` });
  for (const seg of d.timeline) {
    const el = h("span", { class: `seg ${seg.category}` });
    el.style.left = `${((seg.start - t0) / span) * 100}%`;
    el.style.width = `max(calc(${((seg.end - seg.start) / span) * 100}% - 2px), 2px)`;
    tip(el, () => `<strong>${esc(seg.label)}</strong><div>${dotHtml(seg.category)}${LABELS[seg.category]}</div><div class="muted">${fmtClock(seg.start)}–${fmtClock(seg.end)} · ${fmtDur(seg.end - seg.start)}</div>`);
    strip.append(el);
  }
  if (isToday) {
    const now = h("span", { class: "now", "aria-hidden": "true" });
    now.style.left = `${((Date.now() - t0) / span) * 100}%`;
    strip.append(now);
  }

  const axis = h("div", { class: "axis", "aria-hidden": "true" }, ...["00", "06", "12", "18", "24"].map((t) => h("span", {}, t)));
  const parts: Node[] = [strip, axis];
  // Focus sessions run as a thin lane above the activity they covered.
  if (shownFocus.length) {
    const lane = h("div", { class: "focus-lane", role: "img", "aria-label": `${shownFocus.length} focus ${shownFocus.length === 1 ? "session" : "sessions"}` });
    for (const b of shownFocus) {
      const el = h("span", { class: `focus-block${b.completed ? "" : " cut"}` });
      el.style.left = `${((b.start - t0) / span) * 100}%`;
      el.style.width = `max(${((b.end - b.start) / span) * 100}%, 3px)`;
      tip(el, () => `<strong>Focus session</strong><div class="muted">${fmtClock(b.start)}–${fmtClock(b.end)} · ${fmtDur(b.end - b.start)}${b.completed ? "" : " · ended early"}</div>`);
      lane.append(el);
    }
    parts.unshift(lane);
  }
  if (!d.timeline.length) parts.push(h("p", { class: "empty" }, isToday ? "Nothing tracked yet today. Kairo records in the background while it runs." : "Nothing was tracked this day."));
  $("#today-timeline").replaceChildren(...parts);
}

function renderUsage(root: HTMLElement, items: Usage[], emptyText: string) {
  if (!items.length) {
    root.replaceChildren(h("p", { class: "empty" }, emptyText));
    return;
  }
  const max = items[0].ms;
  root.replaceChildren(
    ...items.map((u) => {
      const line = h("div", { class: `line ${u.category}` });
      line.style.width = `${Math.max((u.ms / max) * 100, 1)}%`;
      const row = h("div", { class: "usage" }, h("div", { class: "usage-top" }, h("span", { class: "usage-label" }, u.label), h("span", { class: "usage-value" }, fmtDur(u.ms))), line);
      tip(row, () => `<strong>${esc(u.label)}</strong><div>${dotHtml(u.category)}${LABELS[u.category]}<b>${fmtDur(u.ms)}</b></div>`);
      return row;
    }),
  );
}

/// Goal amounts read "0m" rather than "<1m" when nothing has been used yet.
const fmtGoal = (ms: number) => (ms < 30_000 ? "0m" : fmtDur(ms));

function renderGoals(g: DayGoals, isToday: boolean) {
  const box = $("#goals");
  box.hidden = !g.items.length;
  if (!g.items.length) return;
  // The streak runs up to yesterday, so it only reads naturally next to today.
  $("#goals-streak").textContent = isToday && g.streak ? `${g.streak}-day streak` : "";
  $("#goals-list").replaceChildren(
    ...g.items.map((p) => {
      const over = !p.met && p.kind !== "productive";
      const tone = p.kind === "productive" ? "productive" : over ? "distracting" : "neutral";
      const fill = h("div", { class: `line ${tone}` });
      fill.style.width = `${p.targetMs ? Math.min(p.usedMs / p.targetMs, 1) * 100 : 0}%`;
      const value = over ? `${fmtGoal(p.usedMs)} · ${fmtDur(p.usedMs - p.targetMs)} over` : `${fmtGoal(p.usedMs)} of ${fmtDur(p.targetMs)}`;
      return h(
        "div",
        { class: `usage goal${over ? " over" : ""}` },
        h("div", { class: "usage-top" }, h("span", { class: "usage-label" }, p.label), h("span", { class: "usage-value" }, value)),
        h("div", { class: "track", role: "img", "aria-label": `${p.label}: ${value}` }, fill),
      );
    }),
  );
}

async function renderHeadsUp() {
  const box = $("#heads-up");
  const h = await api.headsUp().catch(() => null);
  box.hidden = !h;
  if (!h) return;
  $("#heads-up-time").textContent = new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  $("#heads-up-title").textContent = h.title;
  $("#heads-up-detail").textContent = h.detail;
}

// ---------- week ----------

/// The Monday of the week shown; null follows the current week.
let selectedWeek: string | null = null;

const mondayOf = (iso: string) => shiftDate(iso, -weekdayIndex(iso));
const shownWeek = () => selectedWeek ?? mondayOf(todayIso());
const TOTAL_KEYS = { productive: "productiveMs", neutral: "neutralMs", distracting: "distractingMs" } as const;

/// "+12%" against the week before, or nothing when that week had no time to compare with.
function vsLastWeek(current: number, previous: number): string {
  if (!previous) return "";
  const change = Math.round(((current - previous) / previous) * 100);
  return `${change >= 0 ? "+" : "−"}${Math.abs(change)}%`;
}

function goToWeek(monday: string | null) {
  const current = mondayOf(todayIso());
  if (monday && firstDay && monday < mondayOf(firstDay)) monday = mondayOf(firstDay);
  selectedWeek = !monday || monday >= current ? null : monday;
  load("week");
}

/// Opens a day from the week chart on the Today view.
function openDay(iso: string) {
  selectedDay = iso >= todayIso() ? null : iso;
  $<HTMLButtonElement>('[data-view="today"]').click();
}

function renderWeek(w: Week) {
  const isThisWeek = selectedWeek === null;
  const start = parseDate(w.start);
  const end = parseDate(shiftDate(w.start, 6));
  const otherYear = end.getFullYear() !== new Date().getFullYear();
  const from = start.toLocaleDateString([], start.getMonth() === end.getMonth() ? { day: "numeric" } : { day: "numeric", month: "long" });
  $("#week-range").textContent = `${from} – ${end.toLocaleDateString([], { day: "numeric", month: "long", ...(otherYear ? { year: "numeric" } : {}) })}`;
  $("#week-title").textContent = isThisWeek ? "This week so far" : `Week of ${start.toLocaleDateString([], { day: "numeric", month: "long" })}`;
  $<HTMLButtonElement>("#week-prev").disabled = !firstDay || w.start <= mondayOf(firstDay);
  $<HTMLButtonElement>("#week-next").disabled = isThisWeek;
  $("#week-this").hidden = isThisWeek;

  const [t, p] = [w.totals, w.previous];
  $("#week-stats").replaceChildren(
    stat("Productive", fmtGoal(t.productiveMs), vsLastWeek(t.productiveMs, p.productiveMs)),
    stat("Distracting", fmtGoal(t.distractingMs), vsLastWeek(t.distractingMs, p.distractingMs)),
    stat("On screen", fmtGoal(t.activeMs), vsLastWeek(t.activeMs, p.activeMs)),
    stat("Goals met", w.goalDays ? `${w.goalDaysMet} of ${w.goalDays} days` : "—"),
    stat("Best day", w.bestDay ? parseDate(w.bestDay).toLocaleDateString([], { weekday: "long" }) : "—"),
    stat("Focus sessions", String(w.focusSessions), w.focusSessions ? fmtDur(w.focusMs) : ""),
  );
  const before = parseDate(shiftDate(w.start, -7));
  const beforeEnd = parseDate(shiftDate(w.start, -1));
  $("#week-compare").textContent = p.activeMs
    ? `Changes compare with ${before.toLocaleDateString([], { day: "numeric", month: "long" })} – ${beforeEnd.toLocaleDateString([], { day: "numeric", month: "long" })}.`
    : "";
  renderWeekChart(w);
  renderWeekApps(w);

  $("#week-limits").replaceChildren(
    ...(w.limitsOver.length
      ? w.limitsOver.map((l) =>
          h(
            "div",
            { class: "usage" },
            h(
              "div",
              { class: "usage-top" },
              h("span", { class: "usage-label" }, l.label),
              h("span", { class: "usage-value over" }, `${fmtDur(l.overMs)} over · ${l.daysOver} ${l.daysOver === 1 ? "day" : "days"}`),
            ),
          ),
        )
      : [h("p", { class: "empty" }, "No limits passed. Daily limits are set in Categories.")]),
  );
}

function renderWeekChart(w: Week) {
  const max = Math.max(HOUR_MS, ...w.days.map((d) => d.totals.activeMs));
  const today = todayIso();
  $("#week-chart").replaceChildren(
    ...w.days.map((d) => {
      const date = parseDate(d.date);
      const name = date.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
      // Productive sits at the bottom of each bar, distracting on top.
      const bar = h("div", { class: "wk-bar" });
      for (const c of ["distracting", "neutral", "productive"] as const) {
        const ms = d.totals[TOTAL_KEYS[c]];
        if (!ms) continue;
        const part = h("span", { class: `wk-part ${c}` });
        part.style.height = `${(ms / max) * 100}%`;
        bar.append(part);
      }
      const goal = d.goalsMet === null ? "" : d.goalsMet ? "✓" : "×";
      const col = h(
        "button",
        { class: `wk-day${d.date === today ? " today" : ""}`, "aria-label": `${name}: ${fmtGoal(d.totals.activeMs)} on screen` },
        h("span", { class: "wk-value" }, d.totals.activeMs ? fmtDur(d.totals.activeMs) : ""),
        bar,
        h("span", { class: "wk-label" }, date.toLocaleDateString([], { weekday: "short" })),
        h("span", { class: `wk-goal${d.goalsMet === false ? " missed" : ""}`, title: d.goalsMet === null ? "" : d.goalsMet ? "Goals met" : "Goals missed" }, goal),
      ) as HTMLButtonElement;
      col.disabled = d.date > today;
      tip(col, () => `<strong>${esc(name)}</strong>${CATEGORIES.map((c) => `<div>${dotHtml(c)}${LABELS[c]}<b>${fmtGoal(d.totals[TOTAL_KEYS[c]])}</b></div>`).join("")}`);
      col.addEventListener("click", () => openDay(d.date));
      return col;
    }),
  );
}

function renderWeekApps(w: Week) {
  const root = $("#week-apps");
  if (!w.topApps.length) {
    root.replaceChildren(h("p", { class: "empty" }, "Nothing tracked this week."));
    return;
  }
  const top = w.topApps[0].ms;
  root.replaceChildren(
    ...w.topApps.map((a) => {
      const line = h("div", { class: `line ${a.category}` });
      line.style.width = `${Math.max((a.ms / top) * 100, 1)}%`;
      const diff = a.ms - a.previousMs;
      const delta = !a.previousMs ? "new" : `${diff >= 0 ? "+" : "−"}${fmtGoal(Math.abs(diff))}`;
      return h(
        "div",
        { class: "usage" },
        h(
          "div",
          { class: "usage-top" },
          h("span", { class: "usage-label" }, a.label),
          h("span", { class: "usage-delta", title: "Change from last week" }, delta),
          h("span", { class: "usage-value" }, fmtDur(a.ms)),
        ),
        line,
      );
    }),
  );
}

// ---------- patterns ----------

let rangeDays = 30;
let lastPatterns: Patterns | null = null;

function renderPatterns(p: Patterns) {
  lastPatterns = p;
  $("#patterns-days").textContent = `Last ${rangeDays} days · ${p.daysWithData} active`;
  $("#insights").replaceChildren(
    ...p.insights.map((i) =>
      h(
        "article",
        { class: `insight ${i.kind}` },
        h("div", {}, h("div", { class: "insight-metric" }, i.metric), h("div", { class: "insight-unit" }, i.unit)),
        h("div", {}, h("h3", {}, i.title), h("p", {}, i.detail)),
      ),
    ),
  );
  renderHourly(p);
  renderHeatmap(p);
}

function renderHourly(p: Patterns) {
  const root = $("#hourly");
  const days = Math.max(p.daysWithData, 1);
  const width = root.clientWidth || 900;
  const height = 210;
  const m = { top: 4, bottom: 22 };
  const innerH = height - m.top - m.bottom;

  const perDay = p.hourly.map((hr) => ({
    hour: hr.hour,
    productive: hr.productiveMs / days / 60_000,
    neutral: hr.neutralMs / days / 60_000,
    distracting: hr.distractingMs / days / 60_000,
    active: hr.activeMs / days / 60_000,
    switchRate: hr.activeMs ? hr.switches / (hr.activeMs / HOUR_MS) : 0,
  }));
  // The top line is a round number of minutes just above the busiest hour.
  const top = Math.max(10, Math.ceil(Math.max(...perDay.map((d) => d.active)) / 10) * 10);
  const band = width / 24;
  const barW = Math.max(band - 8, 3);
  const base = m.top + innerH;

  const svg = s("svg", { width, height, viewBox: `0 0 ${width} ${height}`, role: "img", "aria-label": "Average minutes per hour of day, split by category" });
  svg.append(s("line", { x1: 0, x2: width, y1: m.top, y2: m.top, class: "top-line" }));
  const topLabel = s("text", { x: width, y: m.top - 6, "text-anchor": "end", class: "axis-text" });
  topLabel.textContent = `${top}m`;
  svg.append(topLabel, s("line", { x1: 0, x2: width, y1: base, y2: base, class: "base-line" }));

  for (const d of perDay) {
    const x = d.hour * band + (band - barW) / 2;
    let y = base;
    const stack = CATEGORIES.map((c) => ({ c, v: d[c] })).filter((seg) => seg.v > 0);
    stack.forEach(({ c, v }, i) => {
      const hgt = (v / top) * innerH;
      const isTop = i === stack.length - 1;
      // 2px background gap between stacked segments.
      const drawH = Math.max(hgt - (isTop ? 0 : 2), 0.5);
      svg.append(isTop ? s("path", { d: roundedTop(x, y - hgt, barW, drawH, 2), class: `fill-${c}` }) : s("rect", { x, y: y - hgt + 2, width: barW, height: drawH, class: `fill-${c}` }));
      y -= hgt;
    });
    if (d.hour % 6 === 0) {
      const t = s("text", { x: d.hour * band + band / 2, y: height - 4, "text-anchor": "middle", class: "axis-text" });
      t.textContent = String(d.hour).padStart(2, "0");
      svg.append(t);
    }
    // Hit target covers the whole column so thin bars are easy to hover.
    const hit = s("rect", { x: d.hour * band, y: m.top, width: band, height: innerH, class: "hit" });
    tip(hit, () =>
      d.active < 0.5
        ? `<strong>${fmtHour(d.hour)}–${fmtHour(d.hour + 1)}</strong><div class="muted">No activity</div>`
        : `<strong>${fmtHour(d.hour)}–${fmtHour(d.hour + 1)}</strong>
           <div class="muted">${fmtDur(d.active * 60_000)} on an average day</div>
           ${CATEGORIES.map((c) => `<div>${dotHtml(c)}${LABELS[c]}<b>${fmtDur(d[c] * 60_000)}</b></div>`).join("")}
           <div class="muted">${Math.round(d.switchRate)} switches an hour</div>`,
    );
    svg.append(hit);
  }
  root.replaceChildren(svg);
}

function roundedTop(x: number, y: number, w: number, hgt: number, r: number): string {
  const rr = Math.min(r, w / 2, hgt);
  return `M${x},${y + hgt} V${y + rr} Q${x},${y} ${x + rr},${y} H${x + w - rr} Q${x + w},${y} ${x + w},${y + rr} V${y + hgt} Z`;
}

function renderHeatmap(p: Patterns) {
  const grid = h("div", { class: "heatmap", role: "img", "aria-label": "Productive share by weekday and hour; hours without data are small dots" });
  p.heatmap.forEach((row, wd) => {
    grid.append(h("span", {}, WEEKDAYS[wd]));
    row.forEach((cell, hr) => {
      const enough = cell.activeMs >= MIN_CELL_MS;
      const step = Math.min(RAMP_STEPS - 1, Math.floor((enough ? cell.productiveMs / cell.activeMs : 0) * RAMP_STEPS));
      const el = h("span", { class: enough ? "hm-cell" : "hm-cell empty" }, h("span", { class: enough ? `seq-${step}` : "" }));
      tip(el, () =>
        enough
          ? `<strong>${WEEKDAYS[wd]} ${fmtHour(hr)}–${fmtHour(hr + 1)}</strong><div>${pct(cell.productiveMs, cell.activeMs)}% productive · ${pct(cell.distractingMs, cell.activeMs)}% distracting</div><div class="muted">${fmtDur(cell.activeMs)} tracked in range</div>`
          : `<strong>${WEEKDAYS[wd]} ${fmtHour(hr)}–${fmtHour(hr + 1)}</strong><div class="muted">Not enough data</div>`,
      );
      grid.append(el);
    });
  });
  grid.append(h("span"));
  for (let hr = 0; hr < 24; hr++) grid.append(h("span", { class: "hm-col" }, hr % 6 === 0 ? String(hr).padStart(2, "0") : ""));
  $("#heatmap").replaceChildren(grid);

  $("#heatmap-legend").replaceChildren(h("span", {}, "0%"), ...Array.from({ length: RAMP_STEPS }, (_, i) => h("span", { class: `ramp seq-${i}` })), h("span", {}, "100%"));
}

// ---------- categories ----------

function renderCategories(entries: CategoryEntry[]) {
  const root = $("#categories");
  if (!entries.length) {
    root.replaceChildren(h("p", { class: "empty" }, "Nothing tracked in the last 30 days yet."));
    return;
  }
  const rows = entries.map((e) => {
    const select = h("select", { "aria-label": `Category for ${e.label}` }) as HTMLSelectElement;
    for (const c of CATEGORIES) {
      const opt = h("option", { value: c }, LABELS[c]) as HTMLOptionElement;
      opt.selected = c === e.category;
      select.append(opt);
    }
    const dot = h("span", { class: `dot ${e.category}` });
    select.addEventListener("change", async () => {
      const c = select.value as Category;
      await api.setCategory(e.key, c);
      dot.className = `dot ${c}`;
    });

    const recording = h("select", { "aria-label": `What Kairo records for ${e.label}` }) as HTMLSelectElement;
    for (const m of PRIVACY_MODES) {
      const opt = h("option", { value: m }, RECORDING[m]) as HTMLOptionElement;
      opt.selected = m === e.privacy;
      recording.append(opt);
    }
    recording.addEventListener("change", async () => {
      const m = recording.value as PrivacyMode;
      await api.setPrivacyRule(e.key, m);
      showPrivacyNotice(e, m);
    });

    const limit = minutesSelect(LIMIT_CHOICES, e.limitMin, `Daily limit for ${e.label}`);
    limit.addEventListener("change", () => api.setLimit(e.key, Number(limit.value)));
    return h(
      "tr",
      {},
      h("td", {}, dot, e.label),
      h("td", { class: "kind" }, e.kind),
      h("td", { class: "num" }, fmtDur(e.ms)),
      h("td", {}, select),
      h("td", {}, limit),
      h("td", {}, recording),
    );
  });
  root.replaceChildren(
    h(
      "table",
      { class: "table" },
      h(
        "thead",
        {},
        h("tr", {}, h("th", {}, "Name"), h("th", {}, "Type"), h("th", { class: "num" }, "Time"), h("th", {}, "Category"), h("th", {}, "Daily limit"), h("th", {}, "Recording")),
      ),
      h("tbody", {}, ...rows),
    ),
  );
}

// ---------- goals ----------

const LIMIT_CHOICES = [0, 15, 30, 45, 60, 90, 120, 180, 240];
const PRODUCTIVE_CHOICES = [0, 60, 120, 180, 240, 300, 360, 480];
const BREAK_CHOICES = [0, 45, 60, 90, 120];
const DISTRACTING_CHOICES = [0, 15, 30, 45, 60, 90, 120, 180];

/// A minutes picker where 0 is "off"; a saved value outside the presets is kept as an option.
function minutesSelect(choices: number[], current: number, label: string, offText = "None"): HTMLSelectElement {
  const select = h("select", { "aria-label": label }) as HTMLSelectElement;
  const values = choices.includes(current) ? choices : [...choices, current].sort((a, b) => a - b);
  for (const m of values) {
    const opt = h("option", { value: String(m) }, m ? fmtDur(m * 60_000) : offText) as HTMLOptionElement;
    opt.selected = m === current;
    select.append(opt);
  }
  return select;
}

function renderGoalSettings(s: GoalSettings) {
  for (const [id, choices, current, kind] of [
    ["#goal-productive", PRODUCTIVE_CHOICES, s.productiveMin, "productive"],
    ["#goal-distracting", DISTRACTING_CHOICES, s.distractingMaxMin, "distracting"],
  ] as const) {
    const fresh = minutesSelect([...choices], current, "", "Off");
    const select = $<HTMLSelectElement>(id);
    select.replaceChildren(...fresh.children);
    select.value = String(current);
    select.onchange = () => api.setGoal(kind, Number(select.value));
  }
}

// ---------- privacy ----------

const PRIVACY_MODES: PrivacyMode[] = ["track", "hide_title", "ignore"];
const RECORDING: Record<PrivacyMode, string> = { track: "Record", hide_title: "Hide titles", ignore: "Don't record" };
const RETENTION_LABELS: Record<string, string> = { "0": "forever", "365": "1 year", "90": "90 days", "30": "30 days" };
const ARMED_FOR_MS = 5000;

/// Destructive buttons take a second click within a few seconds instead of a confirm dialog.
function confirmTwice(btn: HTMLButtonElement, armedText: () => string, action: () => Promise<void>) {
  let resting = "";
  let timer = 0;
  const disarm = () => {
    if (!btn.classList.contains("armed")) return;
    clearTimeout(timer);
    btn.classList.remove("armed");
    btn.textContent = resting;
  };
  btn.addEventListener("click", async () => {
    if (!btn.classList.contains("armed")) {
      resting = btn.textContent ?? "";
      btn.classList.add("armed");
      btn.textContent = armedText();
      timer = window.setTimeout(disarm, ARMED_FOR_MS);
      return;
    }
    disarm();
    await action();
  });
  return disarm;
}

let noticeKey = "";

function showPrivacyNotice(e: CategoryEntry, mode: PrivacyMode) {
  const box = $("#privacy-notice");
  const btn = $<HTMLButtonElement>("#privacy-notice-apply");
  disarmNotice();
  noticeKey = e.key;
  box.hidden = mode === "track";
  btn.hidden = false;
  if (mode === "hide_title") {
    $("#privacy-notice-text").textContent = `Kairo won't save window titles from ${e.label} from now on.`;
    btn.textContent = "Also clear titles already saved";
  } else if (mode === "ignore") {
    $("#privacy-notice-text").textContent = `Kairo won't record ${e.label} from now on.`;
    btn.textContent = "Also delete what's already recorded";
  }
}

const disarmNotice = confirmTwice(
  $<HTMLButtonElement>("#privacy-notice-apply"),
  () => "Click again to confirm",
  async () => {
    const text = $("#privacy-notice-text");
    try {
      const n = await api.applyPrivacyToPast(noticeKey);
      text.textContent = n ? `Done. ${n} recorded ${n === 1 ? "session" : "sessions"} changed.` : "Nothing already recorded needed changing.";
      $("#privacy-notice-apply").hidden = true;
      renderCategories(await api.categories());
    } catch (err) {
      text.textContent = `Couldn't change past data: ${err}`;
    }
  },
);

let savedRetention = "0";

function renderRetention(days: number) {
  savedRetention = String(days);
  $<HTMLSelectElement>("#retention").value = savedRetention;
  $("#retention-apply").hidden = true;
}

/// Keeping history for less time deletes the overflow, so that waits for a confirmed click.
$<HTMLSelectElement>("#retention").addEventListener("change", async (e) => {
  const value = (e.target as HTMLSelectElement).value;
  const shorter = value !== "0" && (savedRetention === "0" || Number(value) < Number(savedRetention));
  const apply = $<HTMLButtonElement>("#retention-apply");
  $("#retention-status").textContent = "";
  disarmRetention();
  if (shorter) {
    apply.textContent = `Save and delete history older than ${RETENTION_LABELS[value]}`;
    apply.hidden = false;
    return;
  }
  await saveRetention(Number(value));
});

async function saveRetention(days: number) {
  const out = $("#retention-status");
  try {
    const n = await api.setRetention(days);
    renderRetention(days);
    out.textContent = n ? `Saved. Older history deleted.` : "Saved.";
  } catch (err) {
    out.textContent = `Couldn't save: ${err}`;
  }
}

const disarmRetention = confirmTwice(
  $<HTMLButtonElement>("#retention-apply"),
  () => "Click again to delete older history",
  () => saveRetention(Number($<HTMLSelectElement>("#retention").value)),
);

function deleteTarget(): { range: DeleteRange; date?: string; what: string } | null {
  const choice = $<HTMLSelectElement>("#delete-range").value;
  if (choice === "hour") return { range: "hour", what: "the last hour" };
  if (choice === "today") return { range: "day", what: "today" };
  if (choice === "all") return { range: "all", what: "all history" };
  if (!deleteDate) return null;
  return { range: "day", date: deleteDate, what: longDate(deleteDate) };
}

const longDate = (iso: string) => parseDate(iso).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });

let deleteDate = "";

function setDeleteDate(iso: string) {
  deleteDate = iso;
  const btn = $("#delete-date");
  btn.textContent = longDate(iso);
  btn.setAttribute("aria-label", `Day to delete: ${longDate(iso)}`);
}

const deleteCalendar = calendar({
  anchor: $("#delete-date-pick"),
  trigger: $<HTMLButtonElement>("#delete-date"),
  label: "Day to delete",
  selected: () => deleteDate,
  min: () => firstDay,
  max: todayIso,
  onPick: (iso) => {
    setDeleteDate(iso);
    disarmDelete();
  },
});
$("#delete-date").addEventListener("click", () => deleteCalendar.toggle());

$<HTMLSelectElement>("#delete-range").addEventListener("change", (e) => {
  $("#delete-date-pick").hidden = (e.target as HTMLSelectElement).value !== "day";
  if (!deleteDate) setDeleteDate(shiftDate(todayIso(), -1));
  disarmDelete();
  $("#delete-status").textContent = "";
});

const disarmDelete = confirmTwice(
  $<HTMLButtonElement>("#delete-run"),
  () => `Click again to delete ${deleteTarget()?.what ?? "it"}`,
  async () => {
    const out = $("#delete-status");
    const target = deleteTarget();
    if (!target) {
      out.textContent = "Pick a day first.";
      return;
    }
    try {
      const n = await api.deleteHistory(target.range, target.date);
      out.textContent = n ? `Deleted ${target.what}.` : `Nothing was recorded for ${target.what}.`;
      if (selectedDay && (target.range === "all" || target.date === selectedDay)) selectedDay = null;
      renderCategories(await api.categories());
      refreshStatus();
    } catch (err) {
      out.textContent = `Couldn't delete: ${err}`;
    }
  },
);

// ---------- status ----------

let status: { app: string | null; sinceMs: number | null; tracking: boolean } | null = null;

let focusSession: FocusSession | null = null;

async function refreshStatus() {
  try {
    [status, focusSession] = await Promise.all([api.status(), api.focus()]);
  } catch {
    status = null;
  }
  paintStatus();
}

/// The sidebar's focus box: start buttons when idle, a countdown while a session runs.
function paintFocus() {
  const running = focusSession && Date.now() < focusSession.startMs + focusSession.plannedMs ? focusSession : null;
  $("#focus-idle").hidden = !!running;
  $("#focus-active").hidden = !running;
  if (!running) return;
  const left = running.startMs + running.plannedMs - Date.now();
  $("#focus-left").textContent = `${fmtElapsed(left)} left`;
  $("#focus-progress").style.width = `${(1 - left / running.plannedMs) * 100}%`;
}

for (const btn of document.querySelectorAll<HTMLButtonElement>("[data-focus]")) {
  btn.addEventListener("click", async () => {
    focusSession = await api.startFocus(Number(btn.dataset.focus));
    paintFocus();
    if (view === "today" && selectedDay === null) load("today");
  });
}

$("#focus-stop").addEventListener("click", async () => {
  await api.stopFocus();
  focusSession = null;
  paintFocus();
  if (view === "today" && selectedDay === null) load("today");
});

function paintStatus() {
  paintFocus();
  const el = $("#status");
  if (!status) {
    el.textContent = "";
    el.className = "status";
    return;
  }
  const live = status.tracking && status.app && status.sinceMs;
  el.className = live ? "status live" : "status";
  // The app name gets its own line so long names truncate instead of wrapping into the timer.
  const lines = !status.tracking ? ["Paused"] : live ? [status.app!, fmtElapsed(Date.now() - status.sinceMs!)] : ["Idle"];
  el.replaceChildren(h("span", { class: "status-text" }, ...lines.map((l) => h("span", { title: l }, l))));
}

// ---------- wiring ----------

type View = "today" | "week" | "patterns" | "categories" | "settings";
/// In sidebar order, for Ctrl+1 to Ctrl+5.
const VIEWS: View[] = ["today", "week", "patterns", "categories", "settings"];
let view: View = "today";

async function load(v: View = view) {
  try {
    if (v === "today") {
      firstDay = await api.firstDay();
      const date = selectedDay ?? undefined;
      const [day, goals, focusBlocks] = await Promise.all([api.day(date), api.goals(date), api.focusBlocks(date)]);
      shownFocus = focusBlocks;
      renderToday(day);
      renderGoals(goals, selectedDay === null);
    }
    if (v === "week") {
      firstDay ??= await api.firstDay();
      renderWeek(await api.week(selectedWeek ?? undefined));
    }
    if (v === "patterns") renderPatterns(await api.patterns(rangeDays));
    if (v === "categories") renderCategories(await api.categories());
    if (v === "settings") {
      $("#settings-version").textContent = `Kairo ${await api.version()}`;
      renderRetention(await api.retention());
      renderGoalSettings(await api.goalSettings());
      const breaks = $<HTMLSelectElement>("#break-every");
      breaks.replaceChildren(...minutesSelect(BREAK_CHOICES, await api.breakMinutes(), "", "Never").children);
      breaks.onchange = () => api.setBreakMinutes(Number(breaks.value));
      $<HTMLInputElement>("#nudges-enabled").checked = await api.nudgesEnabled();
      const auto = await api.autostart();
      const box = $<HTMLInputElement>("#autostart-enabled");
      box.checked = auto.enabled;
      box.disabled = !auto.available;
      if (!auto.available) $("#autostart-note").textContent = "Only available in the installed app, not in development builds.";
    }
  } catch (e) {
    const target = { today: "#today-timeline", week: "#week-chart", patterns: "#insights", categories: "#categories", settings: "#update-status" }[v];
    $(target).replaceChildren(h("p", { class: "empty" }, `Couldn't load data: ${e}`));
  }
}

for (const tab of document.querySelectorAll<HTMLButtonElement>("[data-view]")) {
  tab.addEventListener("click", () => {
    view = tab.dataset.view as View;
    for (const t of document.querySelectorAll<HTMLButtonElement>("[data-view]")) t.setAttribute("aria-selected", String(t === tab));
    for (const sec of document.querySelectorAll<HTMLElement>(".view")) sec.hidden = sec.id !== `view-${view}`;
    // Each view starts at its top, not wherever the last one was scrolled to.
    window.scrollTo(0, 0);
    load();
  });
}

for (const btn of document.querySelectorAll<HTMLButtonElement>("#range button")) {
  btn.addEventListener("click", () => {
    rangeDays = Number(btn.dataset.days);
    for (const b of document.querySelectorAll("#range button")) b.setAttribute("aria-pressed", String(b === btn));
    load("patterns");
  });
}

$("#day-prev").addEventListener("click", () => goToDay(shiftDate(shownDay(), -1)));
$("#day-next").addEventListener("click", () => goToDay(shiftDate(shownDay(), 1)));
$("#day-today").addEventListener("click", () => goToDay(null));
const dayCalendar = calendar({
  anchor: $("#today-date-pick"),
  trigger: $<HTMLButtonElement>("#today-date"),
  label: "Pick a day",
  selected: shownDay,
  min: () => firstDay,
  max: todayIso,
  onPick: goToDay,
});
$("#today-date").addEventListener("click", () => dayCalendar.toggle());

// Kairo is an app, not a page: no browser menu on right-click, and no reload, find, print or
// view-source shortcuts. Development builds keep them for debugging.
const BROWSER_KEYS = new Set(["r", "f", "g", "p", "u", "j", "h", "s"]);
if (import.meta.env.PROD) {
  document.addEventListener("contextmenu", (e) => {
    if (!(e.target as HTMLElement).closest("input, textarea")) e.preventDefault();
  });
  document.addEventListener("keydown", (e) => {
    const ctrl = e.ctrlKey || e.metaKey;
    if (e.key === "F5" || e.key === "F7" || (ctrl && BROWSER_KEYS.has(e.key.toLowerCase()))) e.preventDefault();
  });
}

/// Ctrl+1 to Ctrl+5 open the views in sidebar order.
document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
  const next = VIEWS[Number(e.key) - 1];
  if (!next) return;
  e.preventDefault();
  $<HTMLButtonElement>(`[data-view="${next}"]`).click();
});

$("#update-check").addEventListener("click", async () => {
  const out = $("#update-status");
  try {
    await api.checkForUpdates();
    out.textContent = "Checking… the result shows as a notification.";
  } catch (err) {
    out.textContent = `Couldn't check: ${err}`;
  }
});

$("#week-prev").addEventListener("click",() => goToWeek(shiftDate(shownWeek(), -7)));
$("#week-next").addEventListener("click", () => goToWeek(shiftDate(shownWeek(), 7)));
$("#week-this").addEventListener("click", () => goToWeek(null));

/// ←/→ step a day on Today and a week on Week; T jumps back to the present.
document.addEventListener("keydown", (e) => {
  if ((view !== "today" && view !== "week") || e.altKey || e.ctrlKey || e.metaKey) return;
  if ((e.target as HTMLElement).closest("input, select, textarea")) return;
  const step = (n: number) => (view === "today" ? goToDay(shiftDate(shownDay(), n)) : goToWeek(shiftDate(shownWeek(), 7 * n)));
  if (e.key === "ArrowLeft") step(-1);
  else if (e.key === "ArrowRight") step(1);
  else if (e.key === "t" || e.key === "T") (view === "today" ? goToDay : goToWeek)(null);
  else return;
  e.preventDefault();
});

$("#heads-up-dismiss").addEventListener("click", async () => {
  await api.dismissHeadsUp();
  $("#heads-up").hidden = true;
});

$<HTMLInputElement>("#nudges-enabled").addEventListener("change", async (e) => {
  await api.setNudgesEnabled((e.target as HTMLInputElement).checked);
});

$<HTMLInputElement>("#autostart-enabled").addEventListener("change", async (e) => {
  const box = e.target as HTMLInputElement;
  try {
    await api.setAutostart(box.checked);
  } catch (err) {
    box.checked = !box.checked;
    $("#autostart-note").textContent = `Couldn't change it: ${err}`;
  }
});

$("#nudge-test").addEventListener("click", async () => {
  const out = $("#nudge-status");
  try {
    await api.testNudge();
    out.textContent = "Sent. Check your notifications.";
  } catch (e) {
    out.textContent = `Couldn't send: ${e}`;
  }
});

$("#demo-load").addEventListener("click", async () => {
  const n = await api.seedDemo();
  $("#demo-status").textContent = `Loaded ${n} demo sessions. Open Patterns to see insights.`;
  load();
});
$("#demo-clear").addEventListener("click", async () => {
  const n = await api.clearDemo();
  $("#demo-status").textContent = `Removed ${n} demo sessions.`;
  load();
});

let resizeTimer = 0;
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => lastPatterns && view === "patterns" && renderHourly(lastPatterns), 100);
});

setInterval(() => {
  // Past days don't change, so only the live day needs refreshing.
  if (view === "today" && selectedDay === null) load("today");
}, REFRESH_MS);
setInterval(refreshStatus, STATUS_POLL_MS);
// The elapsed counter ticks locally between polls.
setInterval(paintStatus, 1000);

renderLegends();
refreshStatus();
load();
