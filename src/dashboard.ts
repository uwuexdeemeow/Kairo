import { jumpTo } from "./jump";
import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { cancelShortcutRecording, loadKairoSettings, shortcutKeys, wireKairoSettings } from "./kairo-settings";
import { useDropdowns } from "./dropdown";
import { ledgerLine } from "./ledger-tasks";
import { shown } from "./ring-model";
import { dialSvg, greeting, logo } from "./logos";
import { $, fmtDur, followTheme, h, pct } from "./ui";

// Kairo's dashboard: a tile for each app on Home, and the settings that belong to all of Kairo.

/// An app in Kairo. Home draws a tile for each, and App settings (on the Settings tab) a row
/// for each that has settings, so a new app is one more entry here: its settings are either on
/// its own screen (`open`, an Open button) or a section of Kairo's settings (`show`, the id of
/// the control a Show button brings into view).
interface KairoApp {
  id: string;
  name: string;
  what: string;
  /// Today's line on its tile: words, or keys to show as keys.
  line: () => Promise<string | Node[]>;
  /** The app drawn as what it is; Chrono's dial redraws with its line. */
  object: () => HTMLElement;
  open: () => void;
  settings?: { covers: string; open: () => void } | { covers: string; show: string };
}

/// Today's local date, as Kairo's commands take it ("2026-10-29").
function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const APPS: KairoApp[] = [
  {
    id: "chrono",
    name: "Chrono",
    what: "Screen time tracker",
    // Only today's two totals: the full day summary (timeline, apps, sites) is far more work to
    // build every 30 seconds.
    line: async () => {
      const [status, [day]] = await Promise.all([api.status(), api.dayTotals(today(), today())]);
      drawDial(day);
      if (!status.tracking) return "Paused";
      if (!day?.activeMs) return "Nothing tracked yet today";
      return `${fmtDur(day.activeMs)} on screen · ${pct(day.productiveMs, day.activeMs)}% productive`;
    },
    object: () => h("span", { class: "app-dial" }),
    open: () => api.openChrono(),
    settings: {
      covers: "Goals, privacy, projects, calendar, browser extension, nudges, evening summary, focus blocking, breaks and demo data",
      open: () => api.openChrono("settings"),
    },
  },
  {
    id: "ledger",
    name: "Ledger",
    what: "Tasks and notes",
    line: async () => ledgerLine(await api.tasks()),
    object: () => {
      const book = h("span", { class: "app-book" });
      book.innerHTML = logo("ledger", 112);
      return book;
    },
    open: () => api.openLedger(),
    settings: {
      covers: "Reminders and the morning reminder",
      open: () => api.openLedger("settings"),
    },
  },
  {
    id: "palette",
    name: "Command palette",
    what: "Apps, days and commands, from anywhere",
    line: async () => {
      const shortcut = (await api.paletteSettings()).shortcut;
      // The keycap shows the keys; the line says where they work.
      document.querySelector('[data-app="palette"] .keycap')?.replaceChildren(...shortcutKeys(shortcut));
      return "Press it anywhere";
    },
    object: () => h("span", { class: "keycap" }),
    open: () => api.openPalette(),
    settings: { covers: "Shortcut, suggestions and files", show: "palette-record" },
  },
  {
    id: "ring",
    name: "App ring",
    what: "Kairo's apps at the pointer",
    line: async () => {
      const ring = await api.ringSettings();
      document.querySelector('[data-app="ring"] .app-arc')?.replaceChildren(ringArc(ring.items.length));
      return [...shortcutKeys(ring.shortcut), document.createTextNode(` · ${ring.items.length} items`)];
    },
    object: () => h("span", { class: "app-arc" }, ringArc(7)),
    open: () => api.openRing(),
    settings: { covers: "Shortcut, and what's on it", show: "ring-record" },
  },
];

/// The ring as its tile's picture: an arc of segments, the middle one lit.
function ringArc(count: number): HTMLElement {
  const box = h("span", { class: "app-arc-drawing", "aria-hidden": "true" });
  // The places the ring itself shows, by the ring's own rule.
  const offsets = shown(count);
  const at = (r: number, deg: number) => {
    const a = (deg * Math.PI) / 180;
    return `${(60 + r * Math.sin(a)).toFixed(1)} ${(118 - r * Math.cos(a)).toFixed(1)}`;
  };
  const band = (k: number) => {
    const [from, to] = [k * 26 - 12, k * 26 + 12];
    const outer = k === 0 ? 104 : 96;
    return `<path class="arc-seg${k === 0 ? " lit" : ""}" d="M${at(outer, from)} A${outer} ${outer} 0 0 1 ${at(outer, to)} L${at(64, to)} A64 64 0 0 0 ${at(64, from)} Z"/>`;
  };
  box.innerHTML = `<svg viewBox="0 0 120 70" width="112" height="66">${offsets.sort((a, b) => Math.abs(b) - Math.abs(a)).map(band).join("")}</svg>`;
  return box;
}

/// "2h 05" for the dial's middle.
function dialTime(ms: number): string {
  const mins = Math.round(ms / 60_000);
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}`;
}

/// Chrono's dial: today's active time by what it went on.
function drawDial(day: { activeMs: number; productiveMs: number; distractingMs?: number } | undefined) {
  const el = document.querySelector('[data-app="chrono"] .app-dial');
  if (!el) return;
  const active = day?.activeMs ?? 0;
  const productive = day?.productiveMs ?? 0;
  const distracting = day?.distractingMs ?? 0;
  el.innerHTML = dialSvg({ productive, distracting, neutral: Math.max(0, active - productive - distracting) }, dialTime(active), "today");
}

/// The greeting for this hour, its aside in italic.
function drawGreeting() {
  const g = greeting(new Date().getHours());
  $("#dash-title").replaceChildren(`${g.lead} `, h("em", {}, g.aside));
}

type Tab = "home" | "settings";
let current: Tab | null = null;
let settingsWired = false;

function drawTiles() {
  $("#app-tiles").replaceChildren(
    ...APPS.map((app) => {
      const object = h("button", { class: "app-object", "aria-label": `Open ${app.name}` }, app.object());
      object.addEventListener("click", app.open);
      return h(
        "article",
        { class: "app-tile", "data-app": app.id, "aria-label": app.name },
        object,
        h("h2", {}, app.name),
        h("p", { class: "muted app-tile-what" }, app.what),
        h("p", { class: "app-tile-line", "data-line": app.id, "aria-live": "polite" }),
      );
    }),
  );
}

function drawAppSettings() {
  $("#app-settings").replaceChildren(
    ...APPS.filter((app) => app.settings).map((app) => {
      const s = app.settings!;
      // On its own screen, Open; a section of this page, Show.
      const here = "show" in s;
      const go = h("button", { class: "link-btn", "data-settings": app.id }, here ? "Show" : "Open");
      go.setAttribute("aria-label", `${here ? "Show" : "Open"} ${app.name}'s settings`);
      go.addEventListener("click", () => ("show" in s ? jumpTo(s.show) : s.open()));
      return h("li", {}, h("div", {}, h("strong", {}, app.name), h("p", { class: "muted" }, s.covers)), go);
    }),
  );
}

/// Asks each app for today's line; one that can't answer keeps what it showed.
async function refreshLines() {
  await Promise.all(
    APPS.map(async (app) => {
      const el = $(`[data-line="${app.id}"]`);
      try {
        const line = await app.line();
        if (typeof line === "string") el.replaceChildren(line);
        else el.replaceChildren(h("span", { class: "shortcut-keys" }, ...line));
      } catch {
        // Left as it was; the next refresh tries again.
      }
    }),
  );
}

/// Resolves once the tab has what it shows.
async function showTab(tab: Tab): Promise<void> {
  if (tab === current) return;
  current = tab;
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-tab]")) b.setAttribute("aria-pressed", String(b.dataset.tab === tab));
  $("#tab-home").hidden = tab !== "home";
  $("#tab-settings").hidden = tab !== "settings";
  $("#dash-date").hidden = tab !== "home";
  $("#settings-version").hidden = tab !== "settings";
  if (tab === "home") drawGreeting();
  else $("#dash-title").textContent = "Settings";
  if (tab === "home") {
    cancelShortcutRecording();
    refreshLines();
  } else {
    if (!settingsWired) {
      wireKairoSettings();
      settingsWired = true;
    }
    // Read again each time, since Chrono's window can change some of these too.
    await loadKairoSettings().catch(() => {});
  }
}

/// Shows the tab whoever opened the window asked for; with no ask, the current tab stays (Home
/// the first time). An ask of `settings#id` also lands on that control once the tab is filled.
async function followTab() {
  let asked: string | null = null;
  try {
    asked = await api.takeDashboardTab();
  } catch {
    // Nothing asked.
  }
  const [tab, control] = (asked ?? "").split("#");
  await showTab(tab === "settings" ? "settings" : tab === "home" ? "home" : (current ?? "home"));
  if (control && tab === "settings") jumpTo(control);
}

followTheme();
useDropdowns();
$("#dash-date").textContent = new Date().toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
drawTiles();
drawAppSettings();
for (const b of document.querySelectorAll<HTMLButtonElement>("[data-tab]")) b.addEventListener("click", () => showTab(b.dataset.tab as Tab));
followTab();
listen("kairo:dashboard", () => followTab());

// Tile lines stay current while the window is open.
setInterval(() => current === "home" && !document.hidden && refreshLines(), 30_000);
document.addEventListener("visibilitychange", () => current === "home" && !document.hidden && refreshLines());
