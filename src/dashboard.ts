import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { cancelShortcutRecording, loadKairoSettings, shortcutKeys, wireKairoSettings } from "./kairo-settings";
import { useDropdowns } from "./dropdown";
import { $, fmtDur, followTheme, h, pct } from "./ui";

// Kairo's dashboard: a tile for each app on Home, and the settings that belong to all of Kairo.

/// An app in Kairo. Home draws a tile for each, and App settings a row for each that has
/// settings of its own, so a new app is one more entry here.
interface KairoApp {
  id: string;
  name: string;
  what: string;
  /// Today's line on its tile: words, or keys to show as keys.
  line: () => Promise<string | Node[]>;
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
      if (!status.tracking) return "Paused";
      if (!day?.activeMs) return "Nothing tracked yet today";
      return `${fmtDur(day.activeMs)} on screen · ${pct(day.productiveMs, day.activeMs)}% productive`;
    },
    open: () => api.openChrono(),
    settings: {
      covers: "Goals, privacy, projects, calendar, browser extension, nudges, evening summary, focus blocking, breaks and demo data",
      open: () => api.openChrono("settings"),
    },
  },
  {
    id: "palette",
    name: "Command palette",
    what: "Apps, days and commands, from anywhere",
    line: async () => shortcutKeys((await api.paletteSettings()).shortcut),
    open: () => api.openPalette(),
  },
];

type Tab = "home" | "settings";
let current: Tab | null = null;
let settingsWired = false;

function drawTiles() {
  $("#app-tiles").replaceChildren(
    ...APPS.map((app) => {
      const actions = h("div", { class: "row-buttons app-tile-actions" });
      const open = h("button", { class: "btn" }, "Open");
      open.addEventListener("click", app.open);
      actions.append(open);
      if (app.settings) {
        const settings = h("button", { class: "link-btn" }, "Settings");
        settings.addEventListener("click", app.settings.open);
        actions.append(settings);
      }
      return h(
        "article",
        { class: "app-tile", "data-app": app.id, "aria-label": app.name },
        h("h2", {}, app.name),
        h("p", { class: "muted app-tile-what" }, app.what),
        h("p", { class: "app-tile-line", "data-line": app.id, "aria-live": "polite" }),
        actions,
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

function showTab(tab: Tab) {
  if (tab === current) return;
  current = tab;
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-tab]")) b.setAttribute("aria-pressed", String(b.dataset.tab === tab));
  $("#tab-home").hidden = tab !== "home";
  $("#tab-settings").hidden = tab !== "settings";
  $("#dash-date").hidden = tab !== "home";
  $("#settings-version").hidden = tab !== "settings";
  $("#dash-title").textContent = tab === "home" ? "Your apps" : "Settings";
  if (tab === "home") {
    cancelShortcutRecording();
    refreshLines();
  } else {
    if (!settingsWired) {
      wireKairoSettings();
      settingsWired = true;
    }
    // Read again each time, since Chrono's window can change some of these too.
    loadKairoSettings().catch(() => {});
  }
}

/// Shows the tab whoever opened the window asked for; with no ask, the current tab stays (Home
/// the first time).
async function followTab() {
  let asked: string | null = null;
  try {
    asked = await api.takeDashboardTab();
  } catch {
    // Nothing asked.
  }
  showTab(asked === "settings" ? "settings" : asked === "home" ? "home" : (current ?? "home"));
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
