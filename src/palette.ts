// The command palette: opened by a global shortcut over whatever you're doing, it opens apps,
// jumps around Kairo and runs its actions. Everything here stays on this computer.

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type FocusSession, type PaletteApp } from "./api";

const MIN_MS = 60_000;
const HOUR_MS = 60 * MIN_MS;
const MAX_APPS = 6;
const RECENT_APPS = 3;

// ---------- helpers ----------

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;

function h(tag: string, attrs: Record<string, string> = {}, ...children: (Node | string)[]): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  el.append(...children);
  return el;
}

function fmtDur(ms: number): string {
  const mins = Math.round(ms / MIN_MS);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}m`;
}

const pad2 = (n: number) => String(n).padStart(2, "0");
const isoDate = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
const parseIso = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
};
const daysAgo = (n: number) => {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - n);
  return d;
};
const longDate = (d: Date) => d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });

/// Stroke icons, the same family as the sidebar's.
const ICONS: Record<string, string> = {
  focus: '<circle cx="12" cy="13" r="8" /><path d="M12 9v4l2.5 2.5M9.5 2h5" />',
  day: '<rect x="3" y="5" width="18" height="16" rx="2" /><path d="M3 10h18M8 3v4M16 3v4" />',
  chart: '<path d="M3 21h18M6 17v-5M11 17V6M16 17v-8M21 17v-3" />',
  tag: '<path d="M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9z" /><circle cx="7.5" cy="7.5" r="1.5" />',
  settings: '<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0" /><circle cx="16" cy="6" r="2" /><circle cx="10" cy="12" r="2" /><circle cx="18" cy="18" r="2" />',
  today: '<circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />',
  alert: '<path d="M12 8v5M12 16.5v.5" /><circle cx="12" cy="12" r="9" />',
  update: '<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5" />',
};

function strokeIcon(name: string, tone = ""): HTMLElement {
  const box = h("span", { class: `palette-glyph ${tone}`, "aria-hidden": "true" });
  box.innerHTML = `<svg class="icon" viewBox="0 0 24 24">${ICONS[name]}</svg>`;
  return box;
}

// ---------- matching ----------

interface Match {
  score: number;
  /// Positions in the text that matched, for underlining.
  at: number[];
}

const startsWord = (t: string, i: number) => i === 0 || /[\s\-_.(]/.test(t[i - 1]) || (t[i - 1] === t[i - 1].toLowerCase() && t[i] !== t[i].toLowerCase());

/// The query's characters in order within `text`, each one either carrying on from the last
/// match or starting a word, so "vsc" finds Visual Studio Code but stray letters don't match.
/// Starting the text, and longer runs, score higher; null when it doesn't match.
function fuzzy(query: string, text: string): Match | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  const t = text.toLowerCase();
  if (!q) return { score: 0, at: [] };
  const whole = t.indexOf(query.toLowerCase().trim());
  if (whole >= 0 && startsWord(text, whole)) {
    const n = query.trim().length;
    return { score: 100 + (whole === 0 ? 60 : 30) - t.length / 10, at: Array.from({ length: n }, (_, i) => whole + i) };
  }
  // Greedy, trying each word start for the next character when it can't simply carry on.
  const at: number[] = [];
  let score = 0;
  let next = 0;
  for (const c of q) {
    let i = at.length && t[next] === c ? next : -1;
    if (i < 0) {
      for (let j = next; j < t.length; j++) {
        if (t[j] === c && startsWord(text, j)) {
          i = j;
          break;
        }
      }
    }
    if (i < 0) return null;
    score += i === next && at.length ? 6 : 10;
    at.push(i);
    next = i + 1;
  }
  return { score: score - t.length / 20, at };
}

function highlighted(text: string, at: number[] = []): Node[] {
  if (!at.length) return [document.createTextNode(text)];
  const set = new Set(at);
  const out: Node[] = [];
  let run = "";
  let marked = false;
  const flush = () => {
    if (!run) return;
    out.push(marked ? h("mark", {}, run) : document.createTextNode(run));
    run = "";
  };
  [...text].forEach((c, i) => {
    if (set.has(i) !== marked) {
      flush();
      marked = set.has(i);
    }
    run += c;
  });
  flush();
  return out;
}

// ---------- understanding days ----------

const WEEKDAY_NAMES = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
const MONTH_NAMES = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

const weekday = (word: string) => WEEKDAY_NAMES.findIndex((n) => word.length >= 3 && n.startsWith(word));
const month = (word: string) => MONTH_NAMES.findIndex((n) => word.length >= 3 && n.startsWith(word));

/// A past day named the way people say it: "yesterday", "3 days ago", "tue", "last tue",
/// "14 oct", "oct 14". Null when the words aren't a day.
function parseDay(query: string): Date | null {
  const q = query.toLowerCase().trim().replace(/\s+/g, " ");
  if (q === "today") return daysAgo(0);
  if (q === "yesterday") return daysAgo(1);
  const ago = q.match(/^(\d{1,3}) ?(d|days?) ago$/);
  if (ago) return daysAgo(Number(ago[1]));
  const named = q.match(/^(last )?([a-z]+)$/);
  if (named && weekday(named[2]) >= 0) {
    const back = (new Date().getDay() - weekday(named[2]) + 7) % 7;
    // "tue" on a Tuesday is today; "last tue" is the one before.
    return daysAgo(back === 0 && named[1] ? 7 : back);
  }
  const dated = q.match(/^(\d{1,2}) ([a-z]+)$/) ?? q.match(/^([a-z]+) (\d{1,2})$/);
  if (dated) {
    const [dayText, monthText] = /^\d/.test(dated[1]) ? [dated[1], dated[2]] : [dated[2], dated[1]];
    const m = month(monthText);
    if (m < 0) return null;
    const d = new Date(new Date().getFullYear(), m, Number(dayText));
    if (d.getMonth() !== m) return null;
    // A date still ahead this year means last year's.
    if (d > daysAgo(0)) d.setFullYear(d.getFullYear() - 1);
    return d;
  }
  return null;
}

// ---------- items ----------

interface Item {
  title: string;
  at?: number[];
  sub?: string;
  /// Right-hand detail, like today's time in an app.
  detail?: string;
  icon: () => Node;
  score: number;
  section: string;
  run: () => Promise<unknown>;
  /// A day to preview above the results while this item is selected.
  previewDay?: string;
}

let apps: PaletteApp[] = [];
let session: FocusSession | null = null;
let suggestionsOn = true;
const icons = new Map<string, string | null>();

function appIcon(app: PaletteApp): Node {
  const el = h("span", { class: "app-icon palette-app", "aria-hidden": "true", "data-icon-path": app.path, "data-icon-label": app.name });
  paintIcon(el);
  return el;
}

function paintIcon(el: HTMLElement) {
  const url = icons.get(el.dataset.iconPath ?? "");
  el.classList.toggle("letter", !url);
  if (url) el.replaceChildren(h("img", { src: url, alt: "" }));
  else el.textContent = (el.dataset.iconLabel ?? "?").charAt(0).toUpperCase();
}

/// Fetches the icons of apps on screen that haven't been asked for yet, then fills them in.
async function loadIcons() {
  const wanted = [...document.querySelectorAll<HTMLElement>("[data-icon-path]")].map((el) => el.dataset.iconPath!).filter((p) => !icons.has(p));
  if (!wanted.length) return;
  for (const p of wanted) icons.set(p, null);
  try {
    const found = await api.paletteIcons(wanted);
    for (const [p, url] of Object.entries(found)) icons.set(p, url);
  } catch {
    // Letters stay.
  }
  for (const el of document.querySelectorAll<HTMLElement>("[data-icon-path]")) paintIcon(el);
}

const launch = (app: PaletteApp) => () => api.paletteLaunch(app.path);
const go = (view: string, date?: string) => () => api.paletteGo(view, date);

async function startFocus(minutes: number, label?: string) {
  await api.startFocus(minutes, label);
  await api.paletteHide();
}

const PAGES: { words: string; title: string; view: string; icon: string; date?: () => string }[] = [
  { words: "today", title: "Today", view: "today", icon: "today" },
  { words: "yesterday", title: "Yesterday", view: "today", icon: "day", date: () => isoDate(daysAgo(1)) },
  { words: "week this week", title: "This week", view: "week", icon: "day" },
  { words: "last week", title: "Last week", view: "week", icon: "day", date: () => isoDate(daysAgo(7)) },
  { words: "focus timer sessions", title: "Focus", view: "focus", icon: "focus" },
  { words: "patterns insights", title: "Patterns", view: "patterns", icon: "chart" },
  { words: "categories apps sites limits", title: "Categories", view: "categories", icon: "tag" },
  { words: "settings preferences options", title: "Settings", view: "settings", icon: "settings" },
];

function dayItem(d: Date, score: number): Item {
  const iso = isoDate(d);
  const back = Math.round((daysAgo(0).getTime() - d.getTime()) / (24 * HOUR_MS));
  return {
    title: longDate(d),
    sub: back === 0 ? "Today" : back === 1 ? "Yesterday" : `${back} days ago`,
    icon: () => strokeIcon("day"),
    score,
    section: "Go to",
    run: go("today", iso),
    previewDay: iso,
  };
}

/// What a typed query could mean: apps, days, pages and actions, best first.
function search(query: string): Item[] {
  const items: Item[] = [];
  const q = query.trim();

  // "focus 50 essay": a session of that length, with the rest as its label.
  const focusCmd = q.match(/^focus\s+(\d{1,3})\s*(?:m|min|mins|minutes)?(?:\s+(.+))?$/i);
  if (focusCmd) {
    const minutes = Math.min(240, Math.max(1, Number(focusCmd[1])));
    const label = focusCmd[2]?.trim();
    items.push({
      title: `Start a ${minutes}-minute focus session`,
      sub: label ? `Labelled “${label}”` : "No label",
      icon: () => strokeIcon("focus", "accent"),
      score: 1000,
      section: "Focus",
      run: () => startFocus(minutes, label),
    });
  }

  const day = parseDay(q);
  if (day) items.push(dayItem(day, 900));

  for (const app of apps) {
    const m = fuzzy(q, app.name);
    if (!m) continue;
    // Apps you actually use rise above ones you don't.
    const boost = Math.min(app.usedMs / HOUR_MS, 40);
    items.push({
      title: app.name,
      at: m.at,
      detail: app.usedMs ? `${fmtDur(app.usedMs)} this month` : undefined,
      icon: () => appIcon(app),
      score: m.score + boost,
      section: "Apps",
      run: launch(app),
    });
  }

  for (const page of PAGES) {
    const m = fuzzy(q, page.title) ?? fuzzy(q, page.words);
    if (!m) continue;
    items.push({
      title: page.title,
      at: fuzzy(q, page.title)?.at,
      sub: "Kairo",
      icon: () => strokeIcon(page.icon),
      score: m.score - 5,
      section: "Go to",
      run: go(page.view, page.date?.()),
    });
  }

  const actions: { words: string; title: string; icon: string; run: () => Promise<unknown>; when?: boolean }[] = [
    { words: "focus start session pomodoro 25", title: "Start a 25-minute focus session", icon: "focus", run: () => startFocus(25), when: !session },
    { words: "focus start session 50", title: "Start a 50-minute focus session", icon: "focus", run: () => startFocus(50), when: !session },
    { words: "focus start session 90", title: "Start a 90-minute focus session", icon: "focus", run: () => startFocus(90), when: !session },
    { words: "focus end stop session", title: "End the focus session", icon: "focus", run: go("focus"), when: !!session },
    { words: "check for updates update version", title: "Check for updates", icon: "update", run: async () => (await api.checkForUpdates(), api.paletteHide()) },
    { words: "open kairo show window", title: "Open Kairo", icon: "today", run: go("today") },
  ];
  for (const a of actions) {
    if (a.when === false) continue;
    const m = fuzzy(q, a.title) ?? fuzzy(q, a.words);
    if (!m) continue;
    items.push({ title: a.title, at: fuzzy(q, a.title)?.at, icon: () => strokeIcon(a.icon, "accent"), score: m.score - 8, section: "Actions", run: a.run });
  }

  items.sort((a, b) => b.score - a.score);
  // Plenty of apps match one or two letters; a handful is enough.
  let appCount = 0;
  const kept = items.filter((i) => i.section !== "Apps" || ++appCount <= MAX_APPS);
  // The best match leads; the rest stay together by section, sections in order of their best.
  const [top, ...rest] = kept;
  const sections = [...new Set(rest.map((i) => i.section))];
  return top ? [top, ...sections.flatMap((s) => rest.filter((i) => i.section === s))] : [];
}

/// What shows before anything is typed: a focus nudge, limits passed, recent apps, places.
async function suggestions(): Promise<{ items: Item[]; tiles: Item[]; chips: Item[] }> {
  const items: Item[] = [];
  const tiles: Item[] = [];
  const chips: Item[] = PAGES.filter((p) => ["Today", "Yesterday", "This week", "Focus", "Settings"].includes(p.title)).map((p) => ({
    title: p.title,
    icon: () => strokeIcon(p.icon),
    score: 0,
    section: "Go to",
    run: go(p.view, p.date?.()),
  }));
  if (!suggestionsOn) return { items, tiles, chips };

  const [day, goals] = await Promise.all([api.day().catch(() => null), api.goals().catch(() => null)]);
  if (session) {
    items.push({
      title: "Focus session in progress",
      sub: session.label ? `“${session.label}”` : "Open the timer",
      icon: () => strokeIcon("focus", "accent"),
      score: 0,
      section: "Suggested",
      run: go("focus"),
    });
  } else {
    items.push({
      title: "Start a 25-minute focus session",
      sub: "Or type “focus 50 essay” for any length and a label",
      icon: () => strokeIcon("focus", "accent"),
      score: 0,
      section: "Suggested",
      run: () => startFocus(25),
    });
  }
  for (const g of goals?.items ?? []) {
    if (g.kind === "productive" || g.met) continue;
    items.push({
      title: `${g.label}: ${fmtDur(g.usedMs - g.targetMs)} over today’s limit`,
      sub: `${fmtDur(g.usedMs)} of ${fmtDur(g.targetMs)}`,
      icon: () => strokeIcon("alert", "warm"),
      score: 0,
      section: "Suggested",
      run: go("today"),
    });
  }
  // Today's busiest apps, where the Start menu has a way to open them.
  for (const used of day?.topApps ?? []) {
    const app = apps.find((a) => a.name.toLowerCase() === used.label.toLowerCase());
    if (!app) continue;
    tiles.push({ title: app.name, detail: fmtDur(used.ms), icon: () => appIcon(app), score: 0, section: "Recent apps", run: launch(app) });
    if (tiles.length === RECENT_APPS) break;
  }
  if (day) $("#palette-today").textContent = `${fmtDur(day.activeMs)} today · ${day.activeMs ? Math.round((day.productiveMs / day.activeMs) * 100) : 0}% productive`;
  return { items, tiles, chips };
}

// ---------- rendering ----------

let shown: Item[] = [];
let selected = 0;
let previewToken = 0;

function row(item: Item, index: number, kind: "row" | "tile" | "chip"): HTMLElement {
  const isSelected = index === selected;
  const el = h("div", {
    class: `palette-${kind}${isSelected ? " selected" : ""}`,
    role: "option",
    id: `palette-item-${index}`,
    "aria-selected": String(isSelected),
  });
  if (kind === "chip") {
    el.append(item.title);
  } else {
    const text = h("span", { class: "palette-text" }, h("span", { class: "palette-title" }, ...highlighted(item.title, item.at)));
    if (item.sub && kind === "row") text.append(h("span", { class: "palette-sub" }, item.sub));
    el.append(item.icon(), text);
    if (item.detail) el.append(h("span", { class: "palette-detail" }, item.detail));
    if (isSelected && kind === "row") el.append(h("kbd", {}, "Enter"));
  }
  el.addEventListener("mousemove", () => {
    if (selected !== index) select(index);
  });
  el.addEventListener("click", () => run(index));
  return el;
}

function label(text: string): HTMLElement {
  return h("p", { class: "palette-label", role: "presentation" }, text);
}

/// Lays out the current items: rows grouped under their section, recent apps as tiles and
/// places as chips when nothing is typed.
function render(groups: { items: Item[]; tiles?: Item[]; chips?: Item[] }) {
  shown = [...groups.items, ...(groups.tiles ?? []), ...(groups.chips ?? [])];
  selected = Math.min(selected, Math.max(shown.length - 1, 0));
  const root = $("#palette-results");
  const out: Node[] = [];
  let index = 0;
  let section = "";
  const typed = !!$<HTMLInputElement>("#palette-input").value.trim();
  for (const item of groups.items) {
    // When typing, the best match leads under its own heading.
    const heading = typed && index === 0 ? "Top hit" : item.section;
    if (heading !== section) out.push(label(heading));
    section = heading;
    out.push(row(item, index++, "row"));
  }
  if (groups.tiles?.length) {
    out.push(label("Recent apps"));
    out.push(h("div", { class: "palette-tiles" }, ...groups.tiles.map((t) => row(t, index++, "tile"))));
  }
  if (groups.chips?.length) {
    out.push(label("Go to"));
    out.push(h("div", { class: "palette-chips" }, ...groups.chips.map((c) => row(c, index++, "chip"))));
  }
  if (!shown.length) out.push(h("p", { class: "palette-empty" }, "Nothing matches. Try an app’s name, a day like “last tue”, or “focus 50”."));
  root.replaceChildren(...out);
  $("#palette-input").setAttribute("aria-activedescendant", shown.length ? `palette-item-${selected}` : "");
  loadIcons();
  renderPreview();
}

function select(index: number) {
  selected = (index + shown.length) % Math.max(shown.length, 1);
  for (const el of document.querySelectorAll<HTMLElement>("[role=option]")) {
    const on = el.id === `palette-item-${selected}`;
    el.classList.toggle("selected", on);
    el.setAttribute("aria-selected", String(on));
    el.querySelector("kbd")?.remove();
    if (on && el.classList.contains("palette-row")) el.append(h("kbd", {}, "Enter"));
  }
  $("#palette-input").setAttribute("aria-activedescendant", `palette-item-${selected}`);
  document.getElementById(`palette-item-${selected}`)?.scrollIntoView({ block: "nearest" });
  renderPreview();
}

/// A selected day gets a glance at how it went, above the results.
async function renderPreview() {
  const box = $("#palette-preview");
  const iso = shown[selected]?.previewDay;
  const token = ++previewToken;
  if (!iso) {
    box.hidden = true;
    return;
  }
  const [day, focus] = await Promise.all([api.day(iso).catch(() => null), api.focusBlocks(iso).catch(() => [])]);
  if (token !== previewToken || !day) return;
  const productive = day.activeMs ? Math.round((day.productiveMs / day.activeMs) * 100) : 0;
  const focusMs = focus.reduce((ms, b) => ms + Math.max(0, b.end - b.start - b.pausedMs), 0);
  const stat = (name: string, value: string) => h("div", { class: "palette-stat" }, h("span", {}, name), h("strong", {}, value));
  const strip = h("div", { class: "palette-strip", role: "img", "aria-label": "The day’s activity from midnight to midnight" });
  const t0 = parseIso(iso).getTime();
  for (const seg of day.timeline) {
    const el = h("span", { class: `seg ${seg.category}` });
    el.style.left = `${((seg.start - t0) / (24 * HOUR_MS)) * 100}%`;
    el.style.width = `max(${((seg.end - seg.start) / (24 * HOUR_MS)) * 100}%, 2px)`;
    strip.append(el);
  }
  box.replaceChildren(
    h("div", { class: "palette-stats" }, stat("On screen", fmtDur(day.activeMs)), stat("Productive", `${productive}%`), stat("Focus sessions", focus.length ? `${focus.length} · ${fmtDur(focusMs)}` : "0")),
    strip,
  );
  box.hidden = false;
}

async function run(index: number) {
  const item = shown[index];
  if (!item) return;
  try {
    await item.run();
  } catch (err) {
    $("#palette-results").prepend(h("p", { class: "palette-error" }, String(err)));
  }
}

async function refresh() {
  const query = $<HTMLInputElement>("#palette-input").value;
  if (query.trim()) {
    render({ items: search(query) });
  } else {
    render(await suggestions());
  }
}

// ---------- wiring ----------

const input = $<HTMLInputElement>("#palette-input");

input.addEventListener("input", () => {
  selected = 0;
  refresh();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    api.paletteHide();
  } else if (e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey)) {
    e.preventDefault();
    select(selected + 1);
  } else if (e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey)) {
    e.preventDefault();
    select(selected - 1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    run(selected);
  }
});

// The window is a little larger than the panel; a click on the empty part closes it like a click
// anywhere else would.
document.body.addEventListener("mousedown", (e) => {
  if (!(e.target as HTMLElement).closest("#palette")) api.paletteHide();
});

/// Every opening starts fresh: an empty box, today's suggestions, the theme the main window
/// last chose.
async function open() {
  try {
    const t = localStorage.getItem("kairo.theme");
    if (t) document.documentElement.dataset.theme = t;
  } catch {
    // The theme from the page's first load stays.
  }
  input.value = "";
  selected = 0;
  input.focus();
  const [list, focus, settings] = await Promise.all([api.paletteApps().catch(() => apps), api.focus().catch(() => null), api.paletteSettings().catch(() => null)]);
  apps = list;
  session = focus;
  suggestionsOn = settings?.suggestions ?? true;
  await refresh();
}

listen("palette:open", open);
open();
