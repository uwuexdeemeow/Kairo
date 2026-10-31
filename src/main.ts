import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type AwayStretch, type CalendarStatus, type Project, type ProjectTime, type ProjectDetail, type Span, type WeekNote, type Category, type CategoryEntry, type DayGoals, type DaySummary, type DayTotal, type FocusAction, type FocusBlock, type FocusRule, type FocusOverview, type FocusSession, type FocusSlot, type BlockList, type BlockLists, type Period, type DeleteRange, type GoalSettings, type Patterns, type PrivacyMode, type SyncState, type Task, type Usage } from "./api";
import { shownControl, useDropdowns } from "./dropdown";
import { jumpTo } from "./jump";
import { renderTasks, type TaskActions } from "./tasks";
import { $, IS_LINUX, IS_MAC, esc, fmtClock, fmtDur, followTheme, h, pct, s } from "./ui";

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

// ---------- icons ----------

/// App icons by executable once asked for: a data URL, or null for apps without one.
const icons = new Map<string, string | null>();
const wantedIcons = new Set<string>();
let iconTimer = 0;

/// An app's icon, or a letter badge for sites and apps without one. Icons not yet loaded are
/// fetched in one batch shortly after, and every badge waiting on them is filled in.
function appIcon(key: string, label: string): HTMLElement {
  const el = h("span", { class: "app-icon", "aria-hidden": "true", "data-icon-key": key, "data-icon-label": label });
  paintIcon(el);
  const exe = key.startsWith("app:") ? key.slice(4) : null;
  if (exe && !icons.has(exe)) {
    wantedIcons.add(exe);
    window.clearTimeout(iconTimer);
    iconTimer = window.setTimeout(fetchIcons, 20);
  }
  return el;
}

function paintIcon(el: HTMLElement) {
  const key = el.dataset.iconKey ?? "";
  const url = key.startsWith("app:") ? icons.get(key.slice(4)) : null;
  el.classList.toggle("letter", !url);
  if (url) el.replaceChildren(h("img", { src: url, alt: "" }));
  else el.textContent = (el.dataset.iconLabel ?? "?").replace(/^www\./, "").charAt(0).toUpperCase();
}

async function fetchIcons() {
  const exes = [...wantedIcons];
  wantedIcons.clear();
  if (!exes.length) return;
  let found: Record<string, string> = {};
  try {
    found = await api.appIcons(exes);
  } catch {
    // Letter badges stay; nothing else depends on icons.
  }
  for (const exe of exes) icons.set(exe, found[exe] ?? null);
  for (const el of document.querySelectorAll<HTMLElement>(".app-icon.letter")) paintIcon(el);
}

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
    stat("Focus sessions", String(shownFocus.length), shownFocus.length ? fmtDur(shownFocus.reduce((ms, b) => ms + focusedMs(b), 0)) : ""),
  );
  renderTimeline(d, isToday);
  if (isToday) renderHeadsUp();
  else $("#heads-up").hidden = true;
  renderUsage($("#today-apps"), d.topApps, isToday ? "No app activity yet today." : "No app activity this day.");
  renderUsage($("#today-sites"), d.topSites, isToday ? "No recognised sites yet today." : "No recognised sites this day.");
}

/// Focus sessions on the day the Today view shows, loaded alongside it.
let shownFocus: FocusBlock[] = [];

/// The whole hours the day's graph spans: from the hour activity began to the hour it ended (or
/// now), at least three hours wide, so short stretches are wide enough to see and point at. A day
/// with nothing in it spans all of it.
function timelineHours(day0: number, times: number[]): [number, number] {
  if (!times.length) return [0, 24];
  const hourOf = (ms: number) => (ms - day0) / HOUR_MS;
  let from = Math.max(0, Math.floor(hourOf(Math.min(...times))));
  let to = Math.min(24, Math.ceil(hourOf(Math.max(...times))));
  const MIN_HOURS = 3;
  if (to - from < MIN_HOURS) {
    to = Math.min(24, from + MIN_HOURS);
    from = Math.max(0, to - MIN_HOURS);
  }
  return [from, to];
}

function renderTimeline(d: DaySummary, isToday: boolean) {
  const day0 = parseDate(shownDay()).getTime();
  const times = [...d.timeline.flatMap((s) => [s.start, s.end]), ...shownFocus.flatMap((b) => [b.start, b.end]), ...(isToday ? [Date.now()] : [])];
  const [fromHour, toHour] = timelineHours(day0, d.timeline.length || shownFocus.length ? times : []);
  const t0 = day0 + fromHour * HOUR_MS;
  const span = (toHour - fromHour) * HOUR_MS;
  const clock = (hour: number) => `${String(hour).padStart(2, "0")}:00`;
  const strip = h("div", { class: "timeline", role: "img", "aria-label": `Activity on ${$("#today-date").textContent} from ${clock(fromHour)} to ${clock(toHour)}` });
  for (const seg of d.timeline) {
    const el = h("span", { class: `seg ${seg.category}` });
    el.style.left = `${((seg.start - t0) / span) * 100}%`;
    el.style.width = `max(calc(${((seg.end - seg.start) / span) * 100}% - 2px), 3px)`;
    tip(el, () => `<strong>${esc(seg.label)}</strong><div>${dotHtml(seg.category)}${LABELS[seg.category]}</div><div class="muted">${fmtClock(seg.start)}–${fmtClock(seg.end)} · ${fmtDur(seg.end - seg.start)}</div>`);
    strip.append(el);
  }
  if (isToday) {
    const now = h("span", { class: "now", "aria-hidden": "true" });
    now.style.left = `${((Date.now() - t0) / span) * 100}%`;
    strip.append(now);
  }

  // A mark every hour on a short day, every two or three on a longer one.
  const hours = toHour - fromHour;
  const step = hours <= 8 ? 1 : hours <= 14 ? 2 : 3;
  const axis = h("div", { class: "axis", "aria-hidden": "true" });
  for (let hour = fromHour; hour <= toHour; hour += step) {
    const mark = h("span", {}, clock(hour));
    mark.style.left = `${((hour - fromHour) / hours) * 100}%`;
    axis.append(mark);
  }
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
  if (!d.timeline.length) parts.push(h("p", { class: "empty" }, isToday ? "Nothing tracked yet today. Chrono records in the background while Kairo runs." : "Nothing was tracked this day."));
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
      const row = h(
        "div",
        { class: "usage" },
        h("div", { class: "usage-top" }, h("span", { class: "usage-label" }, appIcon(u.key, u.label), u.label), h("span", { class: "usage-value" }, fmtDur(u.ms))),
        line,
      );
      tip(row, () => `<strong>${esc(u.label)}</strong><div>${dotHtml(u.category)}${LABELS[u.category]}<b>${fmtDur(u.ms)}</b></div>`);
      return row;
    }),
  );
}

/// The project open in each projects block, by the block's id.
const openProjects = new Map<string, number>();

/// Time per project in `#<id>`, its block hidden when no project had any. Clicking one opens where
/// its time came from over the same `span` holding `date`.
function renderProjectTimes(id: string, times: ProjectTime[], span: Span, date?: string) {
  $(`#${id}-box`).hidden = !times.length;
  const max = times[0]?.ms ?? 1;
  $(`#${id}`).replaceChildren(
    ...times.map((p) => {
      const line = h("div", { class: "line project" });
      line.style.width = `${Math.max((p.ms / max) * 100, 1)}%`;
      const open = openProjects.get(id) === p.id;
      const row = h(
        "button",
        { class: "usage project-toggle", "aria-expanded": String(open), title: open ? "Hide where this time came from" : "Show where this time came from" },
        h("div", { class: "usage-top" }, h("span", { class: "usage-label" }, p.name), h("span", { class: "usage-value" }, fmtDur(p.ms))),
        line,
      );
      row.addEventListener("click", () => {
        if (open) openProjects.delete(id);
        else openProjects.set(id, p.id);
        renderProjectTimes(id, times, span, date);
      });
      const item = h("div", { class: "project-item" }, row);
      if (open) {
        const box = h("div", { class: "project-detail" }, h("p", { class: "muted" }, "Loading…"));
        item.append(box);
        api
          .projectDetail(p.id, date, span)
          .then((d) => paintProjectDetail(box, d, span))
          .catch((err) => box.replaceChildren(h("p", { class: "empty" }, `Couldn't load it: ${err}`)));
      }
      return item;
    }),
  );
}

/// A bar an hour or a day, then the apps and sites and the window titles behind the time.
function paintProjectDetail(box: HTMLElement, d: ProjectDetail, span: Span) {
  const max = Math.max(...d.bars.map((b) => b.ms), 1);
  const label = (b: { startMs: number }, i: number) => {
    const at = new Date(b.startMs);
    if (span === "day") return i % 6 === 0 ? at.toLocaleTimeString([], { hour: "numeric" }) : "";
    if (span === "week") return at.toLocaleDateString([], { weekday: "short" });
    return at.getDate() % 7 === 1 ? String(at.getDate()) : "";
  };
  const name = (b: { startMs: number }) => {
    const at = new Date(b.startMs);
    return span === "day"
      ? `${at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}–${new Date(b.startMs + HOUR_MS).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`
      : at.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
  };
  const bars = h(
    "div",
    { class: "pd-bars", style: `--bars: ${d.bars.length}` },
    ...d.bars.map((b, i) => {
      const fill = h("span", { class: "pd-fill" });
      fill.style.height = `${(b.ms / max) * 100}%`;
      const col = h("div", { class: "pd-bar", "aria-label": `${name(b)}: ${fmtDur(b.ms)}` }, h("span", { class: "pd-track" }, fill), h("span", { class: "pd-label" }, label(b, i)));
      tip(col, () => `<strong>${esc(name(b))}</strong><div class="muted">${b.ms ? fmtDur(b.ms) : "Nothing"}</div>`);
      return col;
    }),
  );
  const sources = h("div");
  renderUsage(sources, d.sources, "Nothing in this stretch.");
  const top = d.titles[0]?.ms ?? 1;
  const titles = d.titles.length
    ? d.titles.map((t) => {
        const fill = h("div", { class: "line project" });
        fill.style.width = `${Math.max((t.ms / top) * 100, 1)}%`;
        return h(
          "div",
          { class: "usage" },
          h("div", { class: "usage-top" }, h("span", { class: "usage-label pd-title", title: t.title }, t.title), h("span", { class: "usage-value" }, fmtDur(t.ms))),
          fill,
        );
      })
    : [h("p", { class: "empty" }, "No window titles: they're hidden by a privacy rule, or the time matched by site.")];
  box.replaceChildren(
    bars,
    h("div", { class: "pd-columns" }, h("section", {}, h("h3", {}, "Apps and sites"), sources), h("section", {}, h("h3", {}, "Window titles"), ...titles)),
  );
}

/// Settings' projects, each editable in place: a change saves once the field is left.
function renderProjects(projects: Project[]) {
  const status = $("#project-status");
  const save = async (run: () => Promise<Project[]>) => {
    try {
      renderProjects(await run());
      status.textContent = "";
    } catch (err) {
      status.textContent = String(err);
    }
  };
  $("#project-list").replaceChildren(
    ...projects.map((p) => {
      const name = h("input", { class: "filter project-name", value: p.name, maxlength: "60", "aria-label": "Project name" }) as HTMLInputElement;
      const words = h("input", { class: "filter project-keywords", value: p.keywords.join(", "), "aria-label": `Keywords for ${p.name}` }) as HTMLInputElement;
      for (const field of [name, words]) field.addEventListener("change", () => save(() => api.saveProject(p.id, name.value, words.value)));
      const target = minutesSelect(PROJECT_TARGET_CHOICES, p.weekTargetMin, `Weekly target for ${p.name}`, "No weekly target");
      target.addEventListener("change", () => save(() => api.setProjectTarget(p.id, Number(target.value))));
      const remove = h("button", { class: "link-btn" }, "Remove");
      remove.addEventListener("click", () => save(() => api.removeProject(p.id)));
      target.classList.add("project-target");
      return h("div", { class: "project-row" }, name, words, target, remove);
    }),
  );
}

$("#project-add").addEventListener("click", async () => {
  const name = $<HTMLInputElement>("#project-name");
  const words = $<HTMLInputElement>("#project-keywords");
  try {
    renderProjects(await api.saveProject(null, name.value, words.value));
    name.value = "";
    words.value = "";
    $("#project-status").textContent = "";
  } catch (err) {
    $("#project-status").textContent = String(err);
  }
});

/// Goal amounts read "0m" rather than "<1m" when nothing has been used yet.
const fmtGoal = (ms: number) => (ms < 30_000 ? "0m" : fmtDur(ms));

function renderGoals(g: DayGoals, isToday: boolean) {
  // The streak runs up to yesterday, so it only reads naturally next to today.
  renderGoalBox("goals", g, isToday && g.streak ? `${g.streak}-day streak` : "");
}

const AWAY_KINDS: [string, string][] = [
  ["meeting", "Meeting"],
  ["call", "Call"],
  ["break", "Break"],
  ["other", "Something else"],
];

/// Long stretches away from the computer, each asking what it was; labelled, it counts as time.
function renderAway(stretches: AwayStretch[]) {
  $("#away").hidden = !stretches.length;
  $("#away-list").replaceChildren(
    ...stretches.map((s) => {
      const say = async (run: () => Promise<void>) => {
        try {
          await run();
          load();
        } catch (err) {
          $("#away-status").textContent = String(err);
        }
      };
      const buttons = AWAY_KINDS.map(([kind, name]) => {
        const b = h("button", { class: "btn" }, name);
        b.addEventListener("click", () => say(() => api.labelAway(s.startMs, s.endMs, kind)));
        return b;
      });
      const skip = h("button", { class: "link-btn" }, "Dismiss");
      skip.addEventListener("click", () => say(() => api.dismissAway(s.startMs)));
      return h(
        "div",
        { class: "away-row" },
        h("span", { class: "away-time" }, `${fmtClock(s.startMs)}–${fmtClock(s.endMs)}`, h("span", { class: "muted" }, ` · ${fmtDur(s.endMs - s.startMs)}`)),
        h("div", { class: "row-buttons" }, ...buttons, skip),
      );
    }),
  );
}

/// The weekly goals on Week; their streak counts the weeks before, so it shows on this week only.
function renderWeekGoals(g: DayGoals) {
  renderGoalBox("week-goals", g, selectedWeek === null && g.streak ? `${g.streak}-week streak` : "");
}

/// A goals block: `#<id>` holding `#<id>-streak` and `#<id>-list`, hidden with no goals set.
function renderGoalBox(id: string, g: DayGoals, streak: string) {
  const box = $(`#${id}`);
  box.hidden = !g.items.length;
  if (!g.items.length) return;
  $(`#${id}-streak`).textContent = streak;
  $(`#${id}-list`).replaceChildren(
    ...g.items.map((p) => {
      const over = !p.met && p.kind !== "productive";
      const tone = p.kind === "productive" ? "productive" : over ? "distracting" : "neutral";
      const fill = h("div", { class: `line ${tone}` });
      fill.style.width = `${p.targetMs ? Math.min(p.usedMs / p.targetMs, 1) * 100 : 0}%`;
      const value = over ? `${fmtGoal(p.usedMs)} · ${fmtDur(p.usedMs - p.targetMs)} over` : `${fmtGoal(p.usedMs)} of ${fmtDur(p.targetMs)}`;
      return h(
        "div",
        { class: `usage goal${over ? " over" : ""}` },
        h(
          "div",
          { class: "usage-top" },
          // Limits are for one app or site and get its icon; the day-wide goals don't.
          h("span", { class: "usage-label" }, ...(p.kind === "limit" ? [appIcon(p.key, p.label)] : []), p.label),
          h("span", { class: "usage-value" }, value),
        ),
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
/// Week shows a week or, switched over, a month or a year.
let span: "week" | "month" | "year" = "week";
/// The 1st of the month shown; null follows the current month.
let selectedMonth: string | null = null;
/// The 1 January of the year shown; null follows the current year.
let selectedYear: string | null = null;

const mondayOf = (iso: string) => shiftDate(iso, -weekdayIndex(iso));
const shownWeek = () => selectedWeek ?? mondayOf(todayIso());
const shownMonth = () => selectedMonth ?? monthStart(todayIso());
const yearStart = (iso: string) => `${iso.slice(0, 4)}-01-01`;
const shownYear = () => selectedYear ?? yearStart(todayIso());
const TOTAL_KEYS = { productive: "productiveMs", neutral: "neutralMs", distracting: "distractingMs" } as const;

/// "+12%" against the week, month or year before, or nothing when that had no time to compare with.
function vsBefore(current: number, previous: number): string {
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

function goToMonth(first: string | null) {
  const current = monthStart(todayIso());
  if (first && firstDay && first < monthStart(firstDay)) first = monthStart(firstDay);
  selectedMonth = !first || first >= current ? null : first;
  load("week");
}

function goToYear(first: string | null) {
  const current = yearStart(todayIso());
  if (first && firstDay && first < yearStart(firstDay)) first = yearStart(firstDay);
  selectedYear = !first || first >= current ? null : first;
  load("week");
}

/// Steps the week shown, or the month or year, `n` back or on.
function stepPeriod(n: number) {
  if (span === "year") goToYear(shiftMonthKeepDay(shownYear(), 12 * n));
  else if (span === "month") goToMonth(shiftMonthKeepDay(shownMonth(), n));
  else goToWeek(shiftDate(shownWeek(), 7 * n));
}

const backToNow = () => ({ week: goToWeek, month: goToMonth, year: goToYear })[span](null);

function setSpan(next: "week" | "month" | "year") {
  span = next;
  for (const b of document.querySelectorAll<HTMLButtonElement>("#span button")) b.setAttribute("aria-pressed", String(b.dataset.span === next));
}

/// Opens a day from the week chart on the Today view.
function openDay(iso: string) {
  selectedDay = iso >= todayIso() ? null : iso;
  $<HTMLButtonElement>('[data-view="today"]').click();
}

function renderPeriod(w: Period) {
  const month = span === "month";
  const year = span === "year";
  const isNow = { week: selectedWeek, month: selectedMonth, year: selectedYear }[span] === null;
  const start = parseDate(w.start);
  const end = parseDate(w.end);
  const otherYear = end.getFullYear() !== new Date().getFullYear();
  const from = start.toLocaleDateString([], start.getMonth() === end.getMonth() ? { day: "numeric" } : { day: "numeric", month: "long" });
  $("#week-range").textContent = `${from} – ${end.toLocaleDateString([], { day: "numeric", month: "long", ...(otherYear ? { year: "numeric" } : {}) })}`;
  $("#week-title").textContent = year
    ? isNow
      ? "This year so far"
      : String(start.getFullYear())
    : month
    ? isNow
      ? "This month so far"
      : start.toLocaleDateString([], { month: "long", ...(otherYear ? { year: "numeric" } : {}) })
    : isNow
      ? "This week so far"
      : `Week of ${start.toLocaleDateString([], { day: "numeric", month: "long" })}`;
  const unit = span;
  const prev = $<HTMLButtonElement>("#week-prev");
  const next = $<HTMLButtonElement>("#week-next");
  prev.disabled = !firstDay || w.start <= (year ? yearStart(firstDay) : month ? monthStart(firstDay) : mondayOf(firstDay));
  next.disabled = isNow;
  prev.setAttribute("aria-label", `Previous ${unit}`);
  prev.title = `Previous ${unit} (←)`;
  next.setAttribute("aria-label", `Next ${unit}`);
  next.title = `Next ${unit} (→)`;
  const back = $("#week-this");
  back.hidden = isNow;
  back.textContent = `This ${unit}`;
  back.title = `Back to this ${unit} (T)`;

  const [t, p] = [w.totals, w.previous];
  $("#week-stats").replaceChildren(
    stat("Productive", fmtGoal(t.productiveMs), vsBefore(t.productiveMs, p.productiveMs)),
    stat("Distracting", fmtGoal(t.distractingMs), vsBefore(t.distractingMs, p.distractingMs)),
    stat("On screen", fmtGoal(t.activeMs), vsBefore(t.activeMs, p.activeMs)),
    stat("Goals met", w.goalDays ? `${w.goalDaysMet} of ${w.goalDays} days` : "—"),
    ...(w.goalWeeks ? [stat("Weekly goals", `${w.goalWeeksMet} of ${w.goalWeeks} weeks`)] : []),
    year
      ? stat("Best month", bestMonth(w) ? parseDate(bestMonth(w)!).toLocaleDateString([], { month: "long" }) : "—")
      : stat("Best day", w.bestDay ? parseDate(w.bestDay).toLocaleDateString([], month ? { weekday: "long", day: "numeric" } : { weekday: "long" }) : "—"),
    stat("Focus sessions", String(w.focusSessions), w.focusSessions ? fmtDur(w.focusMs) : ""),
  );
  const before = parseDate(year ? shiftMonthKeepDay(w.start, -12) : month ? shiftMonthKeepDay(w.start, -1) : shiftDate(w.start, -7));
  const beforeEnd = parseDate(shiftDate(w.start, -1));
  $("#week-compare").textContent = p.activeMs
    ? `Changes compare with ${before.toLocaleDateString([], { day: "numeric", month: "long", ...(year ? { year: "numeric" } : {}) })} – ${beforeEnd.toLocaleDateString([], { day: "numeric", month: "long", ...(year ? { year: "numeric" } : {}) })}.`
    : "";
  $("#week-chart-title").textContent = year ? "Each month" : "Each day";
  if (year) renderYearChart(w);
  else renderWeekChart(w);
  renderWeekApps(w);
  renderProjectTimes("week-projects", w.topProjects, span, w.start);

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

/// The week's notes in their fields, unless one is being typed in.
function renderWeekNotes(n: WeekNote) {
  for (const [id, text] of [["#week-note-well", n.well], ["#week-note-change", n.change]] as const) {
    const field = $<HTMLTextAreaElement>(id);
    if (document.activeElement !== field) field.value = text;
  }
  $("#week-notes-status").textContent = "";
}

/// A month's or year's weeks that have notes, each under its dates.
function renderPeriodNotes(notes: (WeekNote & { monday: string })[]) {
  $("#period-notes-box").hidden = !notes.length;
  $("#period-notes").replaceChildren(
    ...notes.map((n) => {
      const start = parseDate(n.monday);
      const week = h("button", { class: "link-btn" }, `Week of ${start.toLocaleDateString([], { day: "numeric", month: "long" })}`);
      week.addEventListener("click", () => {
        setSpan("week");
        goToWeek(n.monday);
      });
      return h(
        "article",
        { class: "period-note" },
        h("h3", {}, week),
        ...(n.well ? [h("p", {}, h("span", { class: "muted" }, "Went well · "), n.well)] : []),
        ...(n.change ? [h("p", {}, h("span", { class: "muted" }, "To change · "), n.change)] : []),
      );
    }),
  );
}

/// The year's days added up by month, from January.
function months(w: Period) {
  const out: { first: string; totals: Period["totals"]; met: number; tracked: number }[] = [];
  for (const d of w.days) {
    const first = monthStart(d.date);
    let m = out[out.length - 1];
    if (!m || m.first !== first) out.push((m = { first, totals: { activeMs: 0, productiveMs: 0, neutralMs: 0, distractingMs: 0 }, met: 0, tracked: 0 }));
    for (const k of ["activeMs", "productiveMs", "neutralMs", "distractingMs"] as const) m.totals[k] += d.totals[k];
    if (d.goalsMet !== null) {
      m.tracked += 1;
      m.met += Number(d.goalsMet);
    }
  }
  return out;
}

/// The 1st of the month with the most productive time, or null with none.
function bestMonth(w: Period): string | null {
  const best = months(w).reduce<{ first: string; ms: number } | null>((b, m) => (m.totals.productiveMs > (b?.ms ?? 0) ? { first: m.first, ms: m.totals.productiveMs } : b), null);
  return best?.first ?? null;
}

/// A bar a month; clicking one opens that month.
function renderYearChart(w: Period) {
  const list = months(w);
  const max = Math.max(HOUR_MS, ...list.map((m) => m.totals.activeMs));
  const thisMonth = monthStart(todayIso());
  const chart = $("#week-chart");
  chart.classList.remove("month");
  chart.style.setProperty("--days", "12");
  chart.classList.add("year");
  chart.replaceChildren(
    ...list.map((m) => {
      const date = parseDate(m.first);
      const name = date.toLocaleDateString([], { month: "long", year: "numeric" });
      const bar = h("div", { class: "wk-bar" });
      for (const c of ["distracting", "neutral", "productive"] as const) {
        const ms = m.totals[TOTAL_KEYS[c]];
        if (!ms) continue;
        const part = h("span", { class: `wk-part ${c}` });
        part.style.height = `${(ms / max) * 100}%`;
        bar.append(part);
      }
      const goals = m.tracked ? `${m.met}/${m.tracked}` : "";
      const col = h(
        "button",
        { class: `wk-day${m.first === thisMonth ? " today" : ""}`, "aria-label": `${name}: ${fmtGoal(m.totals.activeMs)} on screen` },
        h("span", { class: "wk-value" }, m.totals.activeMs ? fmtHours(m.totals.activeMs) : ""),
        bar,
        h("span", { class: "wk-label" }, date.toLocaleDateString([], { month: "short" })),
        h("span", { class: "wk-goal", title: goals ? `Goals met on ${m.met} of ${m.tracked} days` : "" }, goals),
      ) as HTMLButtonElement;
      col.disabled = m.first > thisMonth;
      tip(col, () => `<strong>${esc(name)}</strong>${CATEGORIES.map((c) => `<div>${dotHtml(c)}${LABELS[c]}<b>${fmtGoal(m.totals[TOTAL_KEYS[c]])}</b></div>`).join("")}`);
      col.addEventListener("click", () => {
        setSpan("month");
        goToMonth(m.first);
      });
      return col;
    }),
  );
}

/// Whole hours for a month's bar, which has no room for minutes: "142h".
const fmtHours = (ms: number) => `${Math.round(ms / HOUR_MS)}h`;

/// A bar per day; a month's are narrow, so they leave out the hours above and, on a small
/// window, most of the dates below.
function renderWeekChart(w: Period) {
  const max = Math.max(HOUR_MS, ...w.days.map((d) => d.totals.activeMs));
  const today = todayIso();
  const month = w.days.length > 7;
  const chart = $("#week-chart");
  chart.classList.remove("year");
  chart.classList.toggle("month", month);
  chart.style.setProperty("--days", String(w.days.length));
  chart.replaceChildren(
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
        h("span", { class: "wk-value" }, d.totals.activeMs && !month ? fmtDur(d.totals.activeMs) : ""),
        bar,
        month
          ? h("span", { class: `wk-label${date.getDate() % 7 === 1 ? "" : " minor"}` }, String(date.getDate()))
          : h("span", { class: "wk-label" }, date.toLocaleDateString([], { weekday: "short" })),
        h("span", { class: `wk-goal${d.goalsMet === false ? " missed" : ""}`, title: d.goalsMet === null ? "" : d.goalsMet ? "Goals met" : "Goals missed" }, goal),
      ) as HTMLButtonElement;
      col.disabled = d.date > today;
      tip(col, () => `<strong>${esc(name)}</strong>${CATEGORIES.map((c) => `<div>${dotHtml(c)}${LABELS[c]}<b>${fmtGoal(d.totals[TOTAL_KEYS[c]])}</b></div>`).join("")}`);
      col.addEventListener("click", () => openDay(d.date));
      return col;
    }),
  );
}

function renderWeekApps(w: Period) {
  const root = $("#week-apps");
  const unit = w.days.length > 31 ? "year" : w.days.length > 7 ? "month" : "week";
  if (!w.topApps.length) {
    root.replaceChildren(h("p", { class: "empty" }, `Nothing tracked this ${unit}.`));
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
          h("span", { class: "usage-label" }, appIcon(a.key, a.label), a.label),
          h("span", { class: "usage-delta", title: `Change from last ${unit}` }, delta),
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
  const s = p.summary;
  $("#patterns-stats").replaceChildren(
    stat("On screen, per day", fmtGoal(s.avgActiveMs)),
    stat("Productive", `${pct(s.avgProductiveMs, s.avgActiveMs)}%`, fmtGoal(s.avgProductiveMs)),
    stat("Distracted, per day", fmtGoal(s.avgDistractingMs)),
    stat("Focus sessions", String(s.focusSessions), s.focusSessions ? `${fmtGoal(s.avgFocusMs)}/day` : ""),
  );
  renderUsage($("#patterns-apps"), p.topApps, "No app activity in this range.");
  renderUsage($("#patterns-sites"), p.topSites, "No recognised sites in this range.");
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

/// Everything the table could show; the filter box narrows it without asking Kairo again.
let allCategories: CategoryEntry[] = [];

function renderCategories(entries: CategoryEntry[]) {
  allCategories = entries;
  paintCategories();
}

function paintCategories() {
  const root = $("#categories");
  const query = $<HTMLInputElement>("#category-filter").value.trim().toLowerCase();
  const entries = query ? allCategories.filter((e) => e.label.toLowerCase().includes(query) || e.key.toLowerCase().includes(query)) : allCategories;
  $("#category-count").textContent = query ? `${entries.length} of ${allCategories.length}` : "";
  if (!entries.length) {
    root.replaceChildren(h("p", { class: "empty" }, query ? `No apps or sites match “${query}”.` : "Nothing tracked in the last 30 days yet."));
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
    // During focus: "Auto" follows the category, so its wording shows what that means right now.
    const focus = h("select", { "aria-label": `During focus sessions, ${e.label} is` }) as HTMLSelectElement;
    const autoText = (c: Category) => (c === "distracting" ? "Auto · blocked" : "Auto · allowed");
    for (const [rule, text] of [["auto", autoText(e.category)], ["block", "Always block"], ["allow", "Never block"]] as const) {
      const opt = h("option", { value: rule }, text) as HTMLOptionElement;
      opt.selected = rule === e.focus;
      focus.append(opt);
    }
    focus.addEventListener("change", () => api.setFocusRule(e.key, focus.value as FocusRule));

    select.addEventListener("change", async () => {
      const c = select.value as Category;
      await api.setCategory(e.key, c);
      dot.className = `dot ${c}`;
      focus.options[0].textContent = autoText(c);
    });

    const recording = h("select", { "aria-label": `What Chrono records for ${e.label}` }) as HTMLSelectElement;
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
    // Once there's a limit, it can block for the rest of the day instead of only notifying.
    const hard = h("input", { type: "checkbox", "aria-label": `Block ${e.label} once its limit is passed` }) as HTMLInputElement;
    hard.checked = e.limitHard;
    const hardToggle = h("label", { class: "hard-toggle", title: "Block it for the rest of the day once the limit is passed (games are never cut short)" }, hard, "Block");
    hardToggle.hidden = !e.limitMin;
    hard.addEventListener("change", () => api.setLimitHard(e.key, hard.checked));
    limit.addEventListener("change", async () => {
      await api.setLimit(e.key, Number(limit.value));
      hardToggle.hidden = limit.value === "0";
      if (limit.value === "0") hard.checked = false;
    });
    return h(
      "tr",
      {},
      h("td", {}, h("span", { class: "entry-name" }, dot, appIcon(e.key, e.label), e.label)),
      h("td", { class: "kind" }, e.kind),
      h("td", { class: "num" }, fmtDur(e.ms)),
      h("td", {}, select),
      // In a span, so the cell stays a table cell and lines up with the rest of its row.
      h("td", {}, h("span", { class: "limit-cell" }, limit, hardToggle)),
      h("td", {}, focus),
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
        h("tr", {}, h("th", {}, "Name"), h("th", {}, "Type"), h("th", { class: "num" }, "Time"), h("th", {}, "Category"), h("th", {}, "Daily limit"), h("th", {}, "During focus"), h("th", {}, "Recording")),
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
const WEEK_PRODUCTIVE_CHOICES = [0, 300, 600, 900, 1200, 1500, 1800, 2400];
/// A project's weekly target: an hour up to a full working week.
const PROJECT_TARGET_CHOICES = [0, 60, 120, 180, 300, 420, 600, 900, 1200, 1800, 2400];
const WEEK_DISTRACTING_CHOICES = [0, 120, 180, 300, 420, 600, 840];

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
    ["#goal-week-productive", WEEK_PRODUCTIVE_CHOICES, s.weekProductiveMin, "week-productive"],
    ["#goal-week-distracting", WEEK_DISTRACTING_CHOICES, s.weekDistractingMaxMin, "week-distracting"],
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
    $("#privacy-notice-text").textContent = `Chrono won't save window titles from ${e.label} from now on.`;
    btn.textContent = "Also clear titles already saved";
  } else if (mode === "ignore") {
    $("#privacy-notice-text").textContent = `Chrono won't record ${e.label} from now on.`;
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

/// Time left in a session; a pause holds it still.
function focusLeft(s: FocusSession, now = Date.now()): number {
  const pausedNow = s.pausedAt === null ? 0 : now - s.pausedAt;
  return s.startMs + s.plannedMs + s.pausedMs + pausedNow - now;
}

/// Kairo closes a finished session within seconds; until the next poll, treat it as over here.
const runningFocus = () => (focusSession && (focusSession.pausedAt !== null || focusLeft(focusSession) > 0) ? focusSession : null);

/// The sidebar's focus box: start buttons when idle, a countdown while a session runs.
function paintFocus() {
  const running = runningFocus();
  $("#focus-idle").hidden = !!running;
  $("#focus-active").hidden = !running;
  if (running) {
    const left = focusLeft(running);
    const paused = running.pausedAt !== null;
    // "Paused" rides in the small label so the countdown stays on one line.
    $("#focus-active-label").textContent = paused ? "Focus session · paused" : "Focus session";
    $("#focus-left").textContent = paused ? fmtElapsed(left) : `${fmtElapsed(left)} left`;
    $("#focus-progress").style.width = `${(1 - left / running.plannedMs) * 100}%`;
  }
  if (view === "focus") paintTimer();
}

/// After any change to the session: repaint, and reload the views that list sessions.
function focusChanged(s: FocusSession | null) {
  focusSession = s;
  paintFocus();
  if (view === "today" && selectedDay === null) load("today");
  if (view === "focus") load("focus");
}

const startFocus = async (minutes: number, label?: string, task?: string) => focusChanged(await api.startFocus(minutes, label, task));
const controlFocus = async (action: FocusAction, minutes?: number) => focusChanged((await api.controlFocus(action, minutes)).session);

/// How long a finished strict-mode wait stays good for its confirming click (matches Kairo's).
const STRICT_CONFIRM_MS = 2 * 60_000;

/// Strict mode answers "end early" with a time to come back. The button counts down to it, then
/// asks for the confirming click, then goes back to normal if that never comes.
function holdButton(btn: HTMLButtonElement, readyAt: number) {
  const label = btn.dataset.label ?? btn.textContent ?? "";
  btn.dataset.label = label;
  window.clearInterval(Number(btn.dataset.timer));
  const tick = () => {
    const left = readyAt - Date.now();
    if (left > 0) {
      btn.disabled = true;
      btn.textContent = `Strict mode: wait ${Math.ceil(left / 1000)}s`;
    } else if (left > -STRICT_CONFIRM_MS) {
      btn.disabled = false;
      btn.textContent = `Confirm: ${label.toLowerCase()}`;
    } else {
      releaseButton(btn);
    }
  };
  btn.dataset.timer = String(window.setInterval(tick, 500));
  tick();
}

function releaseButton(btn: HTMLButtonElement) {
  window.clearInterval(Number(btn.dataset.timer));
  if (btn.dataset.label !== undefined) btn.textContent = btn.dataset.label;
  delete btn.dataset.label;
  btn.disabled = false;
}

async function endEarly(btn: HTMLButtonElement) {
  const result = await api.controlFocus("stop");
  if (result.waitUntil) {
    holdButton(btn, result.waitUntil);
    return;
  }
  releaseButton(btn);
  focusChanged(result.session);
}

for (const btn of document.querySelectorAll<HTMLButtonElement>("[data-focus]")) {
  btn.addEventListener("click", () => startFocus(Number(btn.dataset.focus)));
}
$<HTMLButtonElement>("#focus-stop").addEventListener("click", (e) => endEarly(e.currentTarget as HTMLButtonElement));
$("#focus-left").addEventListener("click", () => $<HTMLButtonElement>('[data-view="focus"]').click());

// ---------- focus page ----------

const TIMER_STEP_MIN = 5;
const TIMER_MAX_MIN = 240;
const TIMER_KEY = "kairo.timerMinutes";

/// The length picked for the next session, remembered between visits when storage allows.
let timerMinutes = (() => {
  try {
    const saved = Number(localStorage.getItem(TIMER_KEY));
    return saved >= TIMER_STEP_MIN && saved <= TIMER_MAX_MIN ? saved : 25;
  } catch {
    return 25;
  }
})();

function setTimerMinutes(minutes: number) {
  timerMinutes = Math.min(TIMER_MAX_MIN, Math.max(TIMER_STEP_MIN, minutes));
  try {
    localStorage.setItem(TIMER_KEY, String(timerMinutes));
  } catch {
    // Not remembered this time; nothing else depends on it.
  }
  paintTimer();
}

/// The big timer: the chosen length while idle, the countdown and controls while running.
function paintTimer() {
  const running = runningFocus();
  $("#timer-setup").hidden = !!running;
  $("#timer-run").hidden = !running;
  const time = $("#timer-time");
  if (!running) {
    $("#timer-eyebrow").textContent = "Focus";
    $("#timer-title").textContent = "Start a focus session";
    time.textContent = fmtElapsed(timerMinutes * 60_000);
    time.classList.remove("paused");
    $("#timer-sub").textContent = "";
    $("#timer-minutes").textContent = `${timerMinutes} min`;
    $("#timer-progress").style.width = "0%";
    for (const b of document.querySelectorAll<HTMLButtonElement>("[data-minutes]")) b.setAttribute("aria-pressed", String(Number(b.dataset.minutes) === timerMinutes));
    $<HTMLButtonElement>("#timer-less").disabled = timerMinutes <= TIMER_STEP_MIN;
    $<HTMLButtonElement>("#timer-more").disabled = timerMinutes >= TIMER_MAX_MIN;
    return;
  }
  const left = focusLeft(running);
  const paused = running.pausedAt !== null;
  $("#timer-eyebrow").textContent = paused ? "Focus · paused" : "Focus · in progress";
  $("#timer-title").textContent = running.label ?? "Focusing";
  time.textContent = fmtElapsed(left);
  time.classList.toggle("paused", paused);
  $("#timer-sub").textContent = paused ? "Paused. Distraction nudges are off until you resume." : `Ends at ${fmtClock(Date.now() + left)}`;
  $("#timer-progress").style.width = `${(1 - left / running.plannedMs) * 100}%`;
  $("#timer-pause").textContent = paused ? "Resume" : "Pause";
  $<HTMLButtonElement>("#timer-extend").disabled = running.plannedMs >= TIMER_MAX_MIN * 60_000;
}

const focusedMs = (b: FocusBlock) => Math.max(0, b.end - b.start - b.pausedMs);

// ---------- scheduled focus ----------

const SCHEDULE_LENGTHS = [30, 45, 60, 90, 120, 180, 240];

/// "Weekdays", "Every day", "Weekends" or the days themselves.
function daysText(days: number): string {
  if (days === 0x7f) return "Every day";
  if (days === 0x1f) return "Weekdays";
  if (days === 0x60) return "Weekends";
  return WEEKDAYS.filter((_, i) => days & (1 << i)).join(", ");
}

/// Minutes after midnight as "09:00"; past midnight wraps to the next day's time.
const clockOf = (min: number) => `${String(Math.floor(min / 60) % 24).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;

/// The slot whose block list is open for changing, if any.
let editingSlot: number | null = null;
/// Apps and sites Kairo has seen, to pick block lists from; fetched when first needed.
let blockChoices: { key: string; label: string }[] | null = null;
/// The saved block lists, as last shown.
let savedLists: BlockList[] = [];

const blockLabel = (key: string) => blockChoices?.find((c) => c.key === key)?.label ?? key.slice(key.indexOf(":") + 1);

async function loadBlockChoices() {
  blockChoices ??= (await api.categories().catch(() => [])).map((c) => ({ key: c.key, label: c.label }));
}

/// Apps and sites as removable tags, with a field to add one Kairo has seen; `save` gets the list
/// after each change, and `say` reports a name it can't find.
function keyPicker(keys: string[], save: (keys: string[]) => void, say: (text: string) => void, id: string) {
  const chips = keys.map((key) => {
    const x = h("button", { class: "link-btn", "aria-label": `Stop blocking ${blockLabel(key)}` }, "×");
    x.addEventListener("click", () => save(keys.filter((k) => k !== key)));
    return h("span", { class: "slot-chip" }, blockLabel(key), x);
  });
  const listId = `block-choices-${id}`;
  const input = h("input", { class: "filter", list: listId, placeholder: "Add an app or site", "aria-label": "Add an app or site", autocomplete: "off" }) as HTMLInputElement;
  const options = h(
    "datalist",
    { id: listId },
    ...(blockChoices ?? []).filter((c) => !keys.includes(c.key)).map((c) => h("option", { value: c.label }, c.key.startsWith("site:") ? "Site" : "App")),
  );
  input.addEventListener("change", () => {
    const text = input.value.trim().toLowerCase();
    const found = (blockChoices ?? []).find((c) => c.label.toLowerCase() === text || c.key === text);
    if (!found) {
      say(text ? "Pick an app or site Kairo has seen." : "");
      return;
    }
    save([...keys, found.key]);
  });
  return h("div", { class: "slot-chips" }, ...chips, input, options);
}

function renderSchedule(slots: FocusSlot[]) {
  const root = $("#schedule-list");
  if (!slots.length) {
    root.replaceChildren();
    return;
  }
  root.replaceChildren(
    ...slots.map((s) => {
      const remove = h("button", { class: "link-btn" }, "Remove");
      remove.addEventListener("click", async () => renderSchedule(await api.removeFocusSlot(s.id)));
      const change = h("button", { class: "link-btn", "aria-expanded": String(editingSlot === s.id) }, editingSlot === s.id ? "Done" : "Change");
      change.addEventListener("click", async () => {
        editingSlot = editingSlot === s.id ? null : s.id;
        await loadBlockChoices();
        renderSchedule(await api.focusSchedule());
      });
      const blocks = s.list
        ? `Blocks your “${s.list}” list`
        : s.blocks
          ? `Blocks only ${s.blocks.map(blockLabel).join(", ")}`
          : "Blocks distracting apps and sites";
      return h(
        "div",
        { class: "history-row schedule-row" },
        h("span", { class: "history-time" }, `${clockOf(s.startMin)}–${clockOf(s.startMin + s.minutes)}`),
        h(
          "span",
          { class: "history-label" },
          h("span", { class: s.label ? "" : "muted" }, s.label ?? "Scheduled focus"),
          h("span", { class: "schedule-blocks muted" }, `${blocks} · `, change),
        ),
        h("span", { class: "history-len" }, daysText(s.days)),
        remove,
        ...(editingSlot === s.id ? [slotBlocksEditor(s)] : []),
      );
    }),
  );
}

/// Under a slot: whether it blocks as usual, a saved list or its own, each change saved.
function slotBlocksEditor(s: FocusSlot) {
  const status = h("span", { class: "muted" });
  const run = async (change: () => Promise<FocusSlot[]>) => {
    try {
      renderSchedule(await change());
    } catch (err) {
      status.textContent = String(err);
    }
  };
  // "Only these" with nothing on the list yet blocks as usual until something's added.
  const only = s.blocks !== null || s.list !== null || pendingOnly === s.id;
  const own = s.blocks ?? [];
  const usual = h("button", { "aria-pressed": String(!only) }, "Distracting apps and sites");
  const mine = h("button", { "aria-pressed": String(only) }, "Only these");
  usual.addEventListener("click", () => {
    pendingOnly = null;
    run(async () => (await api.setSlotList(s.id, null), api.setSlotBlocks(s.id, null)));
  });
  mine.addEventListener("click", async () => {
    if (only) return;
    pendingOnly = s.id;
    renderSchedule(await api.focusSchedule());
  });
  const box = h("div", { class: "slot-blocks" }, h("div", { class: "range", role: "group", "aria-label": "What it blocks" }, usual, mine));
  if (only) {
    // A saved list, or one of its own.
    if (savedLists.length) {
      const which = h("select", { "aria-label": "Which list" }, h("option", { value: "" }, "Its own list"), ...savedLists.map((l) => h("option", { value: l.name }, l.name))) as HTMLSelectElement;
      which.value = s.list ?? "";
      which.addEventListener("change", () => {
        pendingOnly = which.value ? null : s.id;
        run(() => api.setSlotList(s.id, which.value || null));
      });
      box.append(h("label", { class: "field" }, "Use", which));
    }
    if (s.list) {
      box.append(h("p", { class: "muted" }, `Blocks what your “${s.list}” list holds; change it under Block lists below.`));
    } else {
      const save = (keys: string[]) => {
        pendingOnly = keys.length ? null : s.id;
        run(() => api.setSlotBlocks(s.id, keys.length ? keys : null));
      };
      box.append(
        keyPicker(own, save, (t) => (status.textContent = t), `slot-${s.id}`),
        h("p", { class: "muted" }, own.length ? "Only these are blocked while this slot runs; everything else is let through." : "Add what this slot should block. Until then it blocks as usual."),
      );
    }
  }
  box.append(status);
  return box;
}

/// The slot just switched to "Only these", before anything is on its list.
let pendingOnly: number | null = null;

/// The saved block lists, each renamed or changed in place, and the list sessions started by hand
/// block.
function renderBlockLists(b: BlockLists) {
  savedLists = b.lists;
  const status = $("#block-list-status");
  const run = async (change: () => Promise<BlockLists>) => {
    try {
      renderBlockLists(await change());
      status.textContent = "";
      // A slot's summary names its list, which may have been renamed or removed.
      renderSchedule(await api.focusSchedule());
    } catch (err) {
      status.textContent = String(err);
    }
  };
  const fallback = h("select", { id: "default-block-list", "aria-label": "Sessions you start block" }, h("option", { value: "" }, "Distracting apps and sites"), ...b.lists.map((l) => h("option", { value: l.name }, `Your “${l.name}” list`))) as HTMLSelectElement;
  fallback.value = b.default ?? "";
  // Beside Start focus, say when a session will block a list rather than everything distracting.
  const note = $("#focus-blocks-note");
  note.hidden = !b.default;
  note.textContent = b.default ? `Blocks your “${b.default}” list. Choose another under Block lists, or name one in the label.` : "";
  fallback.addEventListener("change", () => run(() => api.setDefaultBlockList(fallback.value || null)));
  $("#default-block-list").replaceWith(fallback);
  $("#block-lists").replaceChildren(
    ...b.lists.map((l, i) => {
      const name = h("input", { class: "filter block-list-name", value: l.name, maxlength: "40", "aria-label": "List name" }) as HTMLInputElement;
      name.addEventListener("change", () => run(() => api.saveBlockList(l.name, name.value, l.keys)));
      const remove = h("button", { class: "link-btn" }, "Remove");
      remove.addEventListener("click", () => run(() => api.removeBlockList(l.name)));
      const picker = keyPicker(l.keys, (keys) => run(() => api.saveBlockList(l.name, l.name, keys)), (t) => (status.textContent = t), `list-${i}`);
      return h(
        "div",
        { class: "block-list" },
        h("div", { class: "row-buttons" }, name, remove),
        picker,
        ...(l.keys.length ? [] : [h("p", { class: "muted" }, "Empty, so anything using it blocks as usual.")]),
      );
    }),
  );
}

$("#block-list-add").addEventListener("click", async () => {
  const name = $<HTMLInputElement>("#block-list-name");
  try {
    renderBlockLists(await api.saveBlockList(null, name.value, []));
    name.value = "";
    $("#block-list-status").textContent = "";
  } catch (err) {
    $("#block-list-status").textContent = String(err);
  }
});

/// The add row: day toggles (weekdays to start), and the lengths to choose from.
function setUpScheduleForm() {
  $("#schedule-days").replaceChildren(
    ...WEEKDAYS.map((day, i) => {
      const button = h("button", { class: "day-toggle", "aria-pressed": String(i < 5), title: day }, day.slice(0, 2));
      button.addEventListener("click", () => button.setAttribute("aria-pressed", String(button.getAttribute("aria-pressed") !== "true")));
      return button;
    }),
  );
  const length = $<HTMLSelectElement>("#schedule-length");
  length.replaceChildren(...SCHEDULE_LENGTHS.map((m) => h("option", { value: String(m) }, fmtDur(m * 60_000))));
  length.value = "120";
  $("#schedule-add").addEventListener("click", async () => {
    const days = [...$("#schedule-days").children].reduce((bits, b, i) => (b.getAttribute("aria-pressed") === "true" ? bits | (1 << i) : bits), 0);
    const [h24, m] = $<HTMLInputElement>("#schedule-start").value.split(":").map(Number);
    const label = $<HTMLInputElement>("#schedule-label").value.trim() || null;
    try {
      renderSchedule(await api.addFocusSlot(days, (h24 || 0) * 60 + (m || 0), Number(length.value), label));
      $<HTMLInputElement>("#schedule-label").value = "";
      $("#schedule-status").textContent = "";
    } catch (err) {
      $("#schedule-status").textContent = String(err);
    }
  });
}
setUpScheduleForm();

function renderFocusPage(o: FocusOverview) {
  const finished = o.all.sessions ? `${Math.round((o.all.completed / o.all.sessions) * 100)}%` : "—";
  $("#focus-stats").replaceChildren(
    stat("Today", String(o.today.sessions), o.today.sessions ? fmtDur(o.today.focusedMs) : ""),
    stat("This week", String(o.week.sessions), o.week.sessions ? fmtDur(o.week.focusedMs) : ""),
    stat("All time", String(o.all.sessions), o.all.sessions ? fmtDur(o.all.focusedMs) : ""),
    stat("Finished", finished, o.all.sessions ? `${o.all.completed} of ${o.all.sessions}` : ""),
  );

  const root = $("#focus-history");
  if (!o.recent.length) {
    root.replaceChildren(h("p", { class: "empty" }, "No focus sessions yet. Pick a length above and press Start focus."));
    return;
  }
  const today = todayIso();
  const days = new Map<string, FocusBlock[]>();
  for (const b of o.recent) {
    const day = isoDate(new Date(b.start));
    days.set(day, [...(days.get(day) ?? []), b]);
  }
  const live = runningFocus();
  root.replaceChildren(
    ...[...days].map(([day, blocks]) =>
      h(
        "div",
        { class: "history-day" },
        h("h3", {}, day === today ? "Today" : day === shiftDate(today, -1) ? "Yesterday" : longDate(day)),
        ...blocks.map((b) => {
          const inProgress = !!live && b.start === live.startMs;
          const state = inProgress ? "In progress" : b.completed ? "Finished" : "Ended early";
          return h(
            "div",
            { class: "history-row" },
            h("span", { class: "history-time" }, `${fmtClock(b.start)}–${inProgress ? "now" : fmtClock(b.end)}`),
            h("span", { class: `history-label${b.label ? "" : " muted"}` }, b.label ?? "Focus session"),
            h("span", { class: "history-len" }, fmtGoal(focusedMs(b))),
            h("span", { class: `history-state ${inProgress ? "live" : b.completed ? "done" : "cut"}` }, state),
          );
        }),
      ),
    ),
  );
}

for (const b of document.querySelectorAll<HTMLButtonElement>("[data-minutes]")) {
  b.addEventListener("click", () => setTimerMinutes(Number(b.dataset.minutes)));
}
$("#timer-less").addEventListener("click", () => setTimerMinutes(timerMinutes - TIMER_STEP_MIN));
$("#timer-more").addEventListener("click", () => setTimerMinutes(timerMinutes + TIMER_STEP_MIN));
/// The Focus page's Tasks block; each change redraws it with the list the change returns.
async function drawTasks(tasks?: Task[]) {
  renderTasks($("#focus-tasks"), tasks ?? (await api.tasks()), runningFocus()?.task ?? null, taskActions);
}

const taskActions: TaskActions = {
  add: async (title) => drawTasks(await api.addTask(title)),
  rename: async (id, title) => drawTasks(await api.renameTask(id, title)),
  done: async (id, done) => drawTasks(await api.setTaskDone(id, done)),
  remove: async (id) => drawTasks(await api.removeTask(id)),
  // For the length picked on the timer, like Start focus; the page then reloads, tasks and all.
  start: (id) => startFocus(timerMinutes, undefined, id),
};

$("#timer-start").addEventListener("click", async () => {
  const label = $<HTMLInputElement>("#timer-label");
  await startFocus(timerMinutes, label.value.trim() || undefined);
  label.value = "";
});
$<HTMLInputElement>("#timer-label").addEventListener("keydown", (e) => {
  if (e.key === "Enter") $<HTMLButtonElement>("#timer-start").click();
});
$("#timer-pause").addEventListener("click", () => controlFocus(runningFocus()?.pausedAt != null ? "resume" : "pause"));
$("#timer-extend").addEventListener("click", () => controlFocus("extend", TIMER_STEP_MIN));
$<HTMLButtonElement>("#timer-stop").addEventListener("click", (e) => endEarly(e.currentTarget as HTMLButtonElement));

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

if (IS_MAC) {
  $("#block-explainer").textContent =
    "Blocked apps are hidden behind a Chrono window, and with the browser extension, blocked sites show a Chrono page instead. Pausing a session pauses blocking. Choose what's blocked for each app or site in Categories. Closing the launchers stops new games from starting.";
  // A Mac lets Kairo ask Safari, Chrome and the like which page they show, so the extension is
  // only needed for blocking sites.
  $("#ext-intro").textContent =
    "Chrono asks Safari, Chrome, Brave, Edge, Vivaldi and Arc which site is in front, and reads it from Firefox's and Opera's address bar, so sites are tracked without the extension; macOS asks once per browser, and once for Accessibility, whether that's OK. The extension adds blocking distracting sites during focus sessions. It sends only the domain, never the page or what you type, and only to Kairo on this computer.";
}
if (IS_LINUX) {
  // Linux browsers don't describe their address bar to other apps unless a screen reader is on.
  $("#ext-intro").textContent =
    "Without it, Chrono guesses sites from window titles, which misses most pages in Brave and Chrome. The extension tells Chrono the exact site instead, and blocks distracting sites during focus sessions. It sends only the domain, never the page or what you type, and only to Kairo on this computer.";
  // Kairo sees other apps through X11 or, on Wayland, through GNOME or KDE Plasma themselves.
  $("#block-explainer").textContent =
    "Blocked apps are minimized behind a Chrono window, and with the browser extension, blocked sites show a Chrono page instead. Pausing a session pauses blocking. Choose what's blocked for each app or site in Categories. On Wayland desktops other than GNOME and KDE Plasma, Kairo sees and hides only apps running through XWayland, as most games do.";
}

type View = "today" | "week" | "focus" | "patterns" | "categories" | "settings";
/// In sidebar order, for Ctrl+1 to Ctrl+5.
const VIEWS: View[] = ["today", "week", "focus", "patterns", "categories", "settings"];
let view: View = "today";

async function load(v: View = view) {
  try {
    if (v === "today") {
      firstDay = await api.firstDay();
      // Before anything is recorded, Today is a welcome rather than an empty dashboard.
      const firstRun = firstDay === null;
      $("#view-today").classList.toggle("first-run", firstRun);
      $("#welcome").hidden = !firstRun;
      if (firstRun) return;
      const date = selectedDay ?? undefined;
      const [day, goals, focusBlocks, away, projectTimes] = await Promise.all([api.day(date), api.goals(date), api.focusBlocks(date), api.away(date), api.projectTimes(date)]);
      shownFocus = focusBlocks;
      renderToday(day);
      renderGoals(goals, selectedDay === null);
      renderAway(away);
      renderProjectTimes("today-projects", projectTimes, "day", date);
    }
    if (v === "week") {
      firstDay ??= await api.firstDay();
      $("#week-notes-box").hidden = span !== "week";
      if (span !== "week") {
        const first = span === "year" ? selectedYear : selectedMonth;
        renderPeriod(await (span === "year" ? api.year(first ?? undefined) : api.month(first ?? undefined)));
        // A month or year counts its weeks in the stats; the box is for one week's goals.
        $("#week-goals").hidden = true;
        renderPeriodNotes(await api.notesBetween(first ?? undefined, span));
      } else {
        renderPeriod(await api.week(selectedWeek ?? undefined));
        renderWeekGoals(await api.weekGoals(selectedWeek ?? undefined));
        $("#period-notes-box").hidden = true;
        renderWeekNotes(await api.weekNotes(selectedWeek ?? undefined));
      }
    }
    if (v === "patterns") renderPatterns(await api.patterns(rangeDays));
    if (v === "categories") renderCategories(await api.categories());
    if (v === "focus") {
      paintTimer();
      renderFocusPage(await api.focusOverview());
      await drawTasks();
      // Slots name the apps, sites and saved lists they block, so those come first.
      await loadBlockChoices();
      renderBlockLists(await api.blockLists());
      renderSchedule(await api.focusSchedule());
    }
    if (v === "settings") {
      renderBrowsers(await api.browsers());
      renderRetention(await api.retention());
      renderGoalSettings(await api.goalSettings());
      renderCalendar(await api.calendar());
      renderProjects(await api.projects());
      await refreshExtension();
      const blocking = await api.focusBlocking();
      $<HTMLInputElement>("#block-enabled").checked = blocking.enabled;
      $<HTMLInputElement>("#block-strict").checked = blocking.strict;
      $<HTMLInputElement>("#block-launchers").checked = blocking.launchers;
      const breaks = $<HTMLSelectElement>("#break-every");
      breaks.replaceChildren(...minutesSelect(BREAK_CHOICES, await api.breakMinutes(), "", "Never").children);
      breaks.onchange = () => api.setBreakMinutes(Number(breaks.value));
      $<HTMLInputElement>("#nudges-enabled").checked = await api.nudgesEnabled();
      const evening = await api.eveningSummary();
      $<HTMLInputElement>("#evening-enabled").checked = evening.enabled;
      $<HTMLInputElement>("#evening-time").value = `${String(Math.floor(evening.minute / 60)).padStart(2, "0")}:${String(evening.minute % 60).padStart(2, "0")}`;
    }
  } catch (e) {
    const target = { today: "#today-timeline", week: "#week-chart", focus: "#focus-history", patterns: "#insights", categories: "#categories", settings: "#settings-status" }[v];
    $(target).replaceChildren(h("p", { class: "empty" }, `Couldn't load data: ${e}`));
  }
}

/// Switches to a view and loads it; resolves once it's filled.
function showView(next: View): Promise<void> {
  view = next;
  for (const t of document.querySelectorAll<HTMLButtonElement>("[data-view]")) t.setAttribute("aria-selected", String(t.dataset.view === next));
  for (const sec of document.querySelectorAll<HTMLElement>(".view")) sec.hidden = sec.id !== `view-${next}`;
  // Each view starts at its top, not wherever the last one was scrolled to.
  window.scrollTo(0, 0);
  return load();
}

for (const tab of document.querySelectorAll<HTMLButtonElement>("[data-view]")) {
  tab.addEventListener("click", () => showView(tab.dataset.view as View));
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

/// Reports come every 30 seconds while the browser runs, so a couple of minutes' silence means
/// the browser is closed or the extension isn't installed.
const EXTENSION_QUIET_MS = 2 * 60_000;

async function refreshExtension() {
  const el = $("#ext-status");
  try {
    const s = await api.extensionStatus();
    const ago = s.lastReportMs === null ? null : Date.now() - s.lastReportMs;
    const connected = ago !== null && ago < EXTENSION_QUIET_MS;
    el.className = `ext-status${connected ? " connected" : ""}`;
    el.textContent = !s.listening
      ? "Chrono can't listen for the extension: another program is using port 47613."
      : connected
        ? `Connected. Last heard ${fmtAgo(ago!)}.`
        : ago === null
          ? "Not connected yet. Once it's installed, open the browser and this turns green."
          : `Last heard ${fmtAgo(ago)}. Is the browser open?`;
  } catch (err) {
    el.textContent = `Couldn't check the extension: ${err}`;
  }
}

const fmtAgo = (ms: number) => (ms < 60_000 ? `${Math.max(1, Math.round(ms / 1000))}s ago` : `${fmtDur(ms)} ago`);

/// One button per installed browser: it opens the extensions page and copies the folder's location.
function renderBrowsers(browsers: { id: string; name: string }[]) {
  const note = $("#ext-note");
  if (!browsers.length) {
    $("#ext-browsers").replaceChildren(h("span", { class: "muted" }, "No Brave, Chrome, Edge or Vivaldi found. Use the folder below with any Chromium browser."));
    return;
  }
  $("#ext-browsers").replaceChildren(
    ...browsers.map((b) => {
      const btn = h("button", { class: "btn" }, `Open ${b.name} extensions`);
      btn.addEventListener("click", async () => {
        try {
          const folder = await api.openExtensionsPage(b.id);
          try {
            await navigator.clipboard.writeText(folder);
            note.textContent = `Opened ${b.name}. The folder location is copied: click Load unpacked and press ${IS_MAC ? "⌘V" : "Ctrl+V"}.`;
          } catch {
            note.textContent = `Opened ${b.name}. Click Load unpacked and choose ${folder}`;
          }
        } catch (err) {
          note.textContent = `Couldn't open ${b.name}: ${err}`;
        }
      });
      return btn;
    }),
  );
}

$("#ext-open").addEventListener("click", async () => {
  try {
    await api.openExtensionFolder();
    $("#ext-note").textContent = "";
  } catch (err) {
    $("#ext-note").textContent = `Couldn't open it: ${err}`;
  }
});

$("#category-filter").addEventListener("input", paintCategories);

// ---------- command palette ----------

/// Shows the view, and for Today or Week the date, the command palette asked for, and for
/// Settings the control to land on. The request
/// waits in Kairo until taken: a window opened for it is still loading when it's made.
async function followNavigation() {
  const target = await api.takeNavigation().catch(() => null);
  if (!target) return;
  const { view: next, date, target: control } = target;
  if (next === "today") selectedDay = date && date < todayIso() ? date : null;
  if (next === "week") {
    setSpan("week");
    selectedWeek = date && mondayOf(date) < mondayOf(todayIso()) ? mondayOf(date) : null;
  }
  // Waits for the view's own load, so the control exists and is filled before the jump.
  await showView(next as View);
  if (control) jumpTo(control);
}

listen("kairo:navigate", () => followNavigation());
// Kairo's settings window can change what Chrono's Settings shows too.
listen("kairo:settings", () => view === "settings" && load());

for (const [id, which] of [["#block-enabled", "enabled"], ["#block-launchers", "launchers"], ["#block-strict", "strict"]] as const) {
  $<HTMLInputElement>(id).addEventListener("change", async (e) => {
    const box = e.target as HTMLInputElement;
    try {
      await api.setFocusBlocking(which, box.checked);
      $("#block-note").textContent = "";
    } catch (err) {
      // Strict mode keeps these as they are mid-session.
      box.checked = !box.checked;
      $("#block-note").textContent = String(err);
    }
  });
}

/// Welcome links open another view and bring the relevant control into sight.
for (const link of document.querySelectorAll<HTMLButtonElement>("[data-goto]")) {
  link.addEventListener("click", async () => {
    $<HTMLButtonElement>(`[data-view="${link.dataset.goto}"]`).click();
    await load(link.dataset.goto as View);
    const found = document.querySelector<HTMLElement>(link.dataset.gotoTarget ?? "");
    const target = found && shownControl(found);
    target?.scrollIntoView({ block: "center" });
    target?.focus();
  });
}

$("#welcome-demo").addEventListener("click", async () => {
  await api.seedDemo();
  load("today");
});

$("#week-prev").addEventListener("click", () => stepPeriod(-1));
$("#week-next").addEventListener("click", () => stepPeriod(1));
$("#week-this").addEventListener("click", backToNow);

/// Switching keeps roughly the same time in view: the month of the week shown, or the week of
/// the month's 1st.
for (const btn of document.querySelectorAll<HTMLButtonElement>("#span button")) {
  btn.addEventListener("click", () => {
    const next = btn.dataset.span === "month" ? "month" : btn.dataset.span === "year" ? "year" : "week";
    if (next === span) return;
    // The first day of what's shown now, or null for the present.
    const from = { week: selectedWeek, month: selectedMonth, year: selectedYear }[span];
    setSpan(next);
    if (next === "year") goToYear(from && yearStart(from));
    else if (next === "month") goToMonth(from && monthStart(from));
    else goToWeek(from && mondayOf(from));
  });
}

/// The week's notes save when a field is left; the week shown is the one they belong to.
for (const id of ["#week-note-well", "#week-note-change"]) {
  $(id).addEventListener("change", async () => {
    const status = $("#week-notes-status");
    try {
      await api.setWeekNotes(shownWeek(), $<HTMLTextAreaElement>("#week-note-well").value, $<HTMLTextAreaElement>("#week-note-change").value);
      status.textContent = "Saved";
    } catch (err) {
      status.textContent = `Couldn't save: ${err}`;
    }
  });
}

/// ←/→ step a day on Today and a week or month on Week; T jumps back to the present.
document.addEventListener("keydown", (e) => {
  if ((view !== "today" && view !== "week") || e.altKey || e.ctrlKey || e.metaKey) return;
  if ((e.target as HTMLElement).closest("input, select, textarea")) return;
  const step = (n: number) => (view === "today" ? goToDay(shiftDate(shownDay(), n)) : stepPeriod(n));
  if (e.key === "ArrowLeft") step(-1);
  else if (e.key === "ArrowRight") step(1);
  else if (e.key === "t" || e.key === "T") (view === "today" ? goToDay(null) : backToNow());
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

/// The evening summary's toggle and time save together, whichever changed.
for (const id of ["#evening-enabled", "#evening-time"]) {
  $(id).addEventListener("change", async () => {
    const [h24, m] = $<HTMLInputElement>("#evening-time").value.split(":").map(Number);
    const out = $("#evening-status");
    try {
      await api.setEveningSummary($<HTMLInputElement>("#evening-enabled").checked, (h24 || 0) * 60 + (m || 0));
      out.textContent = "";
    } catch (e) {
      out.textContent = `Couldn't save: ${e}`;
    }
  });
}

$("#evening-test").addEventListener("click", async () => {
  const out = $("#evening-status");
  try {
    await api.testEveningSummary();
    out.textContent = "Sent. Check your notifications.";
  } catch (e) {
    out.textContent = `Couldn't send: ${e}`;
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

function renderCalendar(c: CalendarStatus) {
  const box = $<HTMLTextAreaElement>("#calendar-urls");
  if (document.activeElement !== box) box.value = c.urls.join("\n");
  const read = c.readMs ? `Read at ${fmtClock(c.readMs)}: ${c.events} ${c.events === 1 ? "meeting" : "meetings"} in these four weeks.` : "";
  $("#calendar-status").textContent = !c.urls.length ? "" : [read, c.error ?? ""].filter(Boolean).join(" ") || "Not read yet.";
}

$("#calendar-save").addEventListener("click", async () => {
  const out = $("#calendar-status");
  out.textContent = "Reading…";
  try {
    renderCalendar(await api.setCalendar($<HTMLTextAreaElement>("#calendar-urls").value));
  } catch (err) {
    out.textContent = String(err);
  }
});

/// The sidebar's choice of whose time to show, while more than one computer is combined.
function renderComputerPick(sync: SyncState) {
  const others = sync.computers.filter((c) => !c.thisOne);
  $("#computer-pick").hidden = sync.folder === null || others.length === 0;
  const pick = $<HTMLSelectElement>("#shown-computer");
  pick.replaceChildren(
    h("option", { value: "all" }, "All computers"),
    h("option", { value: "this" }, `This one (${sync.name})`),
    ...others.map((c) => h("option", { value: c.id }, c.name)),
  );
  pick.value = sync.shown;
  // A computer that left the folder falls back to everyone's time.
  if (pick.value !== sync.shown) pick.value = "all";
}

$<HTMLSelectElement>("#shown-computer").addEventListener("change", async (e) => {
  await api.setShownComputer((e.target as HTMLSelectElement).value);
  load();
});
api.sync().then(renderComputerPick, () => {});
// Kairo's settings can change who is combined while Chrono is open in the background, and other
// computers can join the folder.
window.addEventListener("focus", () => api.sync().then(renderComputerPick, () => {}));
listen("kairo:sync", () => {
  api.sync().then(renderComputerPick, () => {});
  load();
});
$("#open-kairo-settings").addEventListener("click", () => api.openDashboard("settings"));

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
// Settings shows whether the extension is talking; keep that current while it's open.
setInterval(() => view === "settings" && refreshExtension(), STATUS_POLL_MS);
// The elapsed counter ticks locally between polls.
setInterval(paintStatus, 1000);

renderLegends();
refreshStatus();
load();
followNavigation();
followTheme();
useDropdowns();
