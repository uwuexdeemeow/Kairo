import { jumpTo } from "./jump";
import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { cancelShortcutRecording, loadKairoSettings, shortcutKeys, wireKairoSettings } from "./kairo-settings";
import { useDropdowns } from "./dropdown";
import { ledgerLine } from "./ledger-tasks";
import { dialSvg, greeting, logo } from "./logos";
import { $, fmtDur, followTheme, h, pct } from "./ui";

// Kairo's dashboard: a tile for each app on Home, and the settings that belong to all of Kairo.

/// An app in Kairo. Home draws a tile for each, and App settings (on the Settings tab) a row
/// for each that has settings of its own, so a new app is one more entry here.
interface KairoApp {
  id: string;
  name: string;
  what: string;
  /// Today's line on its tile: words, or keys to show as keys.
  line: () => Promise<string | Node[]>;
  /** The app drawn as what it is; Chrono's dial redraws with its line. */
  object: () => HTMLElement;
  open: () => void;
  settings?: { covers: string; open: () => void };
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
  },
];

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
      const open = h("button", { class: "link-btn" }, "Open");
      open.setAttribute("aria-label", `Open ${app.name}'s settings`);
      open.addEventListener("click", app.settings!.open);
      return h("li", {}, h("div", {}, h("strong", {}, app.name), h("p", { class: "muted" }, app.settings!.covers)), open);
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
