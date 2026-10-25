// The command palette on macOS and Linux: opened by a global shortcut over whatever you're doing,
// it opens apps, jumps around Kairo and runs its actions. Kairo finds what a query means
// (src-tauri/src/palette), the same as Windows' own palette; this page draws it. Everything here
// stays on this computer.

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type Appearance, type PaletteItem, type PaletteResults } from "./api";

const MIN_MS = 60_000;
/// The window's width, and the room around the panel for its shadow (`.palette-page`).
const WIDTH = 680;
const ROOM_ABOVE = 16;
const ROOM_BELOW = 40;

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

function strokeIcon(name: string, tone: string): HTMLElement {
  const box = h("span", { class: `palette-glyph ${tone}`, "aria-hidden": "true" });
  box.innerHTML = `<svg class="icon" viewBox="0 0 24 24">${ICONS[name] ?? ""}</svg>`;
  return box;
}

/// The title with the characters at `at` marked, as the search matched them.
function highlighted(text: string, at: number[]): Node[] {
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
  // By character, as Kairo counts them.
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

// ---------- icons ----------

const icons = new Map<string, string | null>();

function appIcon(path: string, name: string): HTMLElement {
  const el = h("span", { class: "app-icon palette-app", "aria-hidden": "true", "data-icon-path": path, "data-icon-label": name });
  paintIcon(el);
  return el;
}

function paintIcon(el: HTMLElement) {
  const url = icons.get(el.dataset.iconPath ?? "");
  el.classList.toggle("letter", !url);
  if (url) el.replaceChildren(h("img", { src: url, alt: "" }));
  else el.textContent = [...(el.dataset.iconLabel ?? "?")][0]?.toUpperCase() ?? "?";
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

// ---------- rendering ----------

const input = $<HTMLInputElement>("#palette-input");
let results: PaletteResults = { seq: 0, rows: [], tiles: [], chips: [] };
let shown: PaletteItem[] = [];
let selected = 0;
/// Bumped by each query and day preview, so a slow answer can't replace a newer one.
let queryToken = 0;
let previewToken = 0;

function row(item: PaletteItem, index: number, kind: "row" | "tile" | "chip"): HTMLElement {
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
    const icon = item.icon.kind === "app" ? appIcon(item.icon.path, item.title) : strokeIcon(item.icon.glyph, item.icon.tone);
    el.append(icon, text);
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

/// Lays out the results: rows grouped under their section, then recent apps as tiles and places
/// as chips when nothing is typed.
function render(r: PaletteResults) {
  results = r;
  shown = [...r.rows, ...r.tiles, ...r.chips];
  selected = Math.min(selected, Math.max(shown.length - 1, 0));
  const out: Node[] = [];
  let index = 0;
  let section = "";
  const typed = !!input.value.trim();
  for (const item of r.rows) {
    // When typing, the best match leads under its own heading.
    const heading = typed && index === 0 ? "Top hit" : item.section;
    if (heading !== section) out.push(label(heading));
    section = heading;
    out.push(row(item, index++, "row"));
  }
  if (r.tiles.length) {
    out.push(label("Recent apps"));
    out.push(h("div", { class: "palette-tiles" }, ...r.tiles.map((t) => row(t, index++, "tile"))));
  }
  if (r.chips.length) {
    out.push(label("Go to"));
    out.push(h("div", { class: "palette-chips" }, ...r.chips.map((c) => row(c, index++, "chip"))));
  }
  if (!shown.length && typed) out.push(h("p", { class: "palette-empty" }, "Nothing matches. Try an app’s name, a day like “last tue”, or “focus 50”."));
  $("#palette-results").replaceChildren(...out);
  input.setAttribute("aria-activedescendant", shown.length ? `palette-item-${selected}` : "");
  loadIcons();
  renderPreview();
  fit();
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
  input.setAttribute("aria-activedescendant", `palette-item-${selected}`);
  document.getElementById(`palette-item-${selected}`)?.scrollIntoView({ block: "nearest" });
  renderPreview();
}

/// A selected day gets a glance at how it went, above the results.
async function renderPreview() {
  const box = $("#palette-preview");
  const iso = shown[selected]?.preview;
  const token = ++previewToken;
  if (!iso) {
    if (!box.hidden) {
      box.hidden = true;
      fit();
    }
    return;
  }
  const day = await api.paletteDay(iso).catch(() => null);
  if (token !== previewToken || !day) return;
  const stat = (name: string, value: string) => h("div", { class: "palette-stat" }, h("span", {}, name), h("strong", {}, value));
  const strip = h("div", { class: "palette-strip", role: "img", "aria-label": "The day’s activity from midnight to midnight" });
  for (const seg of day.segments) {
    const el = h("span", { class: `seg ${seg.category}` });
    el.style.left = `${seg.from * 100}%`;
    el.style.width = `max(${(seg.to - seg.from) * 100}%, 2px)`;
    strip.append(el);
  }
  const focus = day.focusCount ? `${day.focusCount} · ${fmtDur(day.focusMs)}` : "0";
  box.replaceChildren(h("div", { class: "palette-stats" }, stat("On screen", fmtDur(day.activeMs)), stat("Productive", `${day.productivePct}%`), stat("Focus sessions", focus)), strip);
  box.hidden = false;
  fit();
}

/// Fits the window to the panel, so no more of it than the shadow is see-through: a click there
/// would land on the palette rather than what's behind it, and some Linux desktops can't show
/// see-through windows at all.
function fit() {
  const height = Math.ceil($("#palette").getBoundingClientRect().height) + ROOM_ABOVE + ROOM_BELOW;
  getCurrentWindow()
    .setSize(new LogicalSize(WIDTH, height))
    .catch(() => {});
}

async function run(index: number) {
  if (!shown[index]) return;
  try {
    await api.paletteRun(results.seq, index);
  } catch (err) {
    $("#palette-results").prepend(h("p", { class: "palette-error" }, String(err)));
    fit();
  }
}

async function refresh() {
  const token = ++queryToken;
  const r = await api.paletteQuery(input.value).catch(() => null);
  if (r && token === queryToken) render(r);
}

// ---------- wiring ----------

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
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    run(selected);
  }
});

// The window keeps a margin around the panel for its shadow; a click there closes it like a
// click anywhere else would.
document.body.addEventListener("mousedown", (e) => {
  if (!(e.target as HTMLElement).closest("#palette")) api.paletteHide();
});

const prefersLight = window.matchMedia("(prefers-color-scheme: light)");

function applyTheme(mode: Appearance) {
  document.documentElement.dataset.theme = mode === "system" ? (prefersLight.matches ? "light" : "dark") : mode;
}

/// Every opening starts fresh: an empty box, today's suggestions, the chosen theme.
async function open() {
  input.value = "";
  selected = 0;
  input.focus();
  try {
    const opening = await api.paletteOpen();
    applyTheme(opening.appearance);
    $("#palette-today").textContent = opening.footer;
  } catch {
    // The last theme and footer stay.
  }
  await refresh();
}

listen("palette:open", open);
open();
