import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { api, type Category, type CategoryEntry, type DaySummary, type Patterns, type Usage } from "./api";

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
  const picker = $<HTMLInputElement>("#day-picker");
  picker.value = day;
  picker.max = today;
  picker.min = firstDay ?? today;
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

async function renderHeadsUp() {
  const box = $("#heads-up");
  const h = await api.headsUp().catch(() => null);
  box.hidden = !h;
  if (!h) return;
  $("#heads-up-time").textContent = new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  $("#heads-up-title").textContent = h.title;
  $("#heads-up-detail").textContent = h.detail;
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
    return h("tr", {}, h("td", {}, dot, e.label), h("td", { class: "kind" }, e.kind), h("td", { class: "num" }, fmtDur(e.ms)), h("td", {}, select));
  });
  root.replaceChildren(
    h("table", { class: "table" }, h("thead", {}, h("tr", {}, h("th", {}, "Name"), h("th", {}, "Type"), h("th", { class: "num" }, "Time"), h("th", {}, "Category"))), h("tbody", {}, ...rows)),
  );
}

// ---------- status ----------

let status: { app: string | null; sinceMs: number | null; tracking: boolean } | null = null;

async function refreshStatus() {
  try {
    status = await api.status();
  } catch {
    status = null;
  }
  paintStatus();
}

function paintStatus() {
  const el = $("#status");
  if (!status) {
    el.textContent = "";
    el.className = "status";
    return;
  }
  const live = status.tracking && status.app && status.sinceMs;
  el.className = live ? "status live" : "status";
  el.textContent = !status.tracking ? "Paused" : live ? `${status.app} · ${fmtElapsed(Date.now() - status.sinceMs!)}` : "Idle";
}

// ---------- wiring ----------

type View = "today" | "patterns" | "categories";
let view: View = "today";

async function load(v: View = view) {
  try {
    if (v === "today") {
      firstDay = await api.firstDay();
      renderToday(await api.day(selectedDay ?? undefined));
    }
    if (v === "patterns") renderPatterns(await api.patterns(rangeDays));
    if (v === "categories") {
      renderCategories(await api.categories());
      $<HTMLInputElement>("#nudges-enabled").checked = await api.nudgesEnabled();
      const auto = await api.autostart();
      const box = $<HTMLInputElement>("#autostart-enabled");
      box.checked = auto.enabled;
      box.disabled = !auto.available;
      if (!auto.available) $("#autostart-note").textContent = "Only available in the installed app, not in development builds.";
    }
  } catch (e) {
    const target = v === "today" ? "#today-timeline" : v === "patterns" ? "#insights" : "#categories";
    $(target).replaceChildren(h("p", { class: "empty" }, `Couldn't load data: ${e}`));
  }
}

for (const tab of document.querySelectorAll<HTMLButtonElement>("[data-view]")) {
  tab.addEventListener("click", () => {
    view = tab.dataset.view as View;
    for (const t of document.querySelectorAll<HTMLButtonElement>("[data-view]")) t.setAttribute("aria-selected", String(t === tab));
    for (const sec of document.querySelectorAll<HTMLElement>(".view")) sec.hidden = sec.id !== `view-${view}`;
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
$("#today-date").addEventListener("click", () => {
  const picker = $<HTMLInputElement>("#day-picker");
  try {
    picker.showPicker();
  } catch {
    picker.focus();
  }
});
$<HTMLInputElement>("#day-picker").addEventListener("change", (e) => {
  const value = (e.target as HTMLInputElement).value;
  if (value) goToDay(value);
});

document.addEventListener("keydown", (e) => {
  if (view !== "today" || e.altKey || e.ctrlKey || e.metaKey) return;
  if ((e.target as HTMLElement).closest("input, select, textarea")) return;
  if (e.key === "ArrowLeft") goToDay(shiftDate(shownDay(), -1));
  else if (e.key === "ArrowRight") goToDay(shiftDate(shownDay(), 1));
  else if (e.key === "t" || e.key === "T") goToDay(null);
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
