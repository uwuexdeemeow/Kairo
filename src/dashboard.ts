import { jumpTo } from "./jump";
import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type Task } from "./api";
import { cancelShortcutRecording, loadKairoSettings, shortcutKeys, wireKairoSettings } from "./kairo-settings";
import { useDropdowns } from "./dropdown";
import { ledgerLine } from "./ledger-tasks";
import { shown } from "./ring-model";
import { dialSvg, greeting, logo } from "./logos";
import { glyph } from "./glyphs";
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
  /// Today's line on its tile: words, or what to draw (keys as keys, a Resume button).
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
      if (!status.tracking) {
        // Paused says how to stop being paused, right there.
        const resume = h("button", { class: "link-btn tile-resume" }, "Resume");
        resume.addEventListener("click", async () => {
          await api.resumeTracking().catch(() => {});
          void refreshLines();
        });
        return [document.createTextNode("Paused · "), resume];
      }
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
    line: async () => ledgerLine(await tasksNow()),
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
      const s = await api.paletteSettings();
      // The keycap shows the keys; the line, one thing to try in it, a different one each day.
      document.querySelector('[data-app="palette"] .keycap')?.replaceChildren(...shortcutKeys(s.shortcut));
      return tryToday(new Date());
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
      return [h("span", { class: "shortcut-keys" }, ...shortcutKeys(ring.shortcut)), document.createTextNode(` · ${ring.items.length} items`)];
    },
    object: () => h("span", { class: "app-arc" }, ringArc(7)),
    open: () => api.openRing(),
    settings: { covers: "Shortcut, and what's on it", show: "ring-record" },
  },
];

/// Things to type in the palette, one shown a day on its tile.
const TRIES = ["yesterday", "speed test", "scan", "12*7", "this week", "yt lofi beats", "windows"];

/// "Try “focus 25”": the palette's tile line for a day, the same all day.
export function tryToday(d: Date): string {
  const day = Math.floor(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 86_400_000);
  return `Try “${TRIES[((day % TRIES.length) + TRIES.length) % TRIES.length]}”`;
}

/// Where the keyboard was put on opening Home, until Today has drawn.
let startedOn: HTMLElement | null = null;

/// Today's tasks, read once per refresh for both Ledger's tile and the Today list.
let tasksRead: Promise<Task[]> | null = null;
const tasksNow = () => (tasksRead ??= api.tasks());

/// Home's Today: the open tasks due today or overdue, each opening in Ledger, and the next
/// reminder still to come today.
async function drawToday() {
  const list = $("#today-list");
  const next = $("#today-next");
  let tasks: Task[];
  try {
    tasks = await tasksNow();
  } catch {
    // What's shown stays; the next refresh tries again.
    return;
  }
  const due = tasks.filter((t) => t.group === "overdue" || t.group === "today");
  const shown = due.slice(0, 5);
  list.replaceChildren(
    ...shown.map((t) => {
      // Ticked off right here, the cheapest thing Home can do; or opened in Ledger.
      const tick = h("button", { class: "today-tick", "aria-label": `Mark “${t.title}” done`, title: "Mark done" }, glyph("check")) as HTMLButtonElement;
      tick.addEventListener("click", async () => {
        tick.disabled = true;
        try {
          await api.setTaskDone(t.id, true);
        } catch {
          tick.disabled = false;
          return;
        }
        // The keyboard moves on to the next task, or the tiles when none is left.
        const at = shown.indexOf(t);
        await refreshLines();
        const ticks = [...list.querySelectorAll<HTMLElement>(".today-tick")];
        (ticks[Math.min(at, ticks.length - 1)] ?? document.querySelector<HTMLElement>(".app-open"))?.focus();
      });
      const open = h(
        "button",
        { class: "today-task", "aria-label": `Open “${t.title}” in Ledger${t.group === "overdue" ? ", overdue" : ""}` },
        h("span", { class: "today-task-title" }, t.title),
        t.group === "overdue" ? h("span", { class: "today-late" }, "Overdue") : "",
      );
      open.addEventListener("click", () => api.openLedger(`task:${t.id}`));
      return h("li", { class: "today-item" }, tick, open);
    }),
  );
  // The greeting's aside says how the day stands, once there's something to say.
  const overdue = tasks.filter((t) => t.group === "overdue").length;
  const aside = document.querySelector("#dash-title em");
  // Late at night it stays "Go easy."
  const hour = new Date().getHours();
  if (aside && current === "home" && hour >= 5 && hour < 22) {
    aside.textContent = overdue ? `${overdue} overdue.` : !due.length ? "all clear." : greeting(hour).aside;
  }
  // Opened on Home with the keyboard on the first tile: today's first task comes before it.
  if (startedOn && document.activeElement === startedOn && shown.length) list.querySelector<HTMLElement>(".today-task")?.focus();
  startedOn = null;
  if (due.length > shown.length) {
    const more = h("button", { class: "link-btn today-more" }, `${due.length - shown.length} more in Ledger`);
    more.addEventListener("click", () => api.openLedger());
    list.append(h("li", {}, more));
  }
  if (!due.length) {
    const add = h("button", { class: "link-btn" }, "Add a task");
    add.addEventListener("click", () => api.openLedger("add"));
    list.replaceChildren(h("li", { class: "today-none muted" }, "Nothing due today. ", add));
  }
  // The next reminder still to come today, if any.
  const now = Date.now();
  const end = new Date();
  end.setHours(23, 59, 59, 999);
  const reminder = tasks
    .filter((t) => t.group !== "done" && t.remindMs !== null && t.remindMs > now && t.remindMs <= end.getTime())
    .sort((a, b) => a.remindMs! - b.remindMs!)[0];
  next.textContent = reminder ? `Next reminder at ${new Date(reminder.remindMs!).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}: ${reminder.title}` : "";
}

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
      // The whole tile opens the app: its name is the button, stretched over the tile, so the
      // drawing, the name and the line are one target, and the name is read once.
      const object = h("span", { class: "app-object", "aria-hidden": "true" }, app.object());
      const open = h("button", { class: "app-open" }, h("span", { class: "sr-only" }, "Open "), app.name);
      open.addEventListener("click", app.open);
      return h(
        "article",
        { class: "app-tile", "data-app": app.id },
        object,
        h("h2", {}, open),
        h("p", { class: "muted app-tile-what" }, app.what),
        // Not a live region: refreshed every 30 seconds, it would talk over whatever is read.
        h("p", { class: "app-tile-line", "data-line": app.id }),
      );
    }),
  );
}

function drawAppSettings() {
  $("#app-settings").replaceChildren(
    // Only apps whose settings are on their own screen: the palette's and the ring's are sections
    // of this page, under Palette and ring, and a row only pointing down the page repeats them.
    ...APPS.filter((app) => app.settings && "open" in app.settings).map((app) => {
      const s = app.settings!;
      // Said as where it takes you: into the app.
      const go = h("button", { class: "link-btn", "data-settings": app.id, "aria-label": `Open ${app.name}'s settings` }, `Open in ${app.name}`);
      go.addEventListener("click", () => "open" in s && s.open());
      return h("li", {}, h("div", {}, h("strong", {}, app.name), h("p", { class: "muted" }, s.covers)), go);
    }),
  );
}

/// Asks each app for today's line; one that can't answer keeps what it showed.
async function refreshLines() {
  tasksRead = null;
  await Promise.all([
    drawToday(),
    ...APPS.map(async (app) => {
      const el = $(`[data-line="${app.id}"]`);
      try {
        const line = await app.line();
        const next = typeof line === "string" ? [document.createTextNode(line)] : line;
        // Only a line that changed is redrawn.
        if (el.innerHTML !== next.map((n) => (n instanceof Element ? n.outerHTML : n.textContent)).join("")) el.replaceChildren(...next);
      } catch {
        // A line already shown is left as it was, the next refresh trying again; a tile with
        // none yet says so rather than stay blank.
        if (!el.textContent) el.textContent = `Couldn't read ${app.name} just now`;
      }
    }),
  ]);
}

/// Resolves once the tab has what it shows.
async function showTab(tab: Tab): Promise<void> {
  if (tab === current) return;
  current = tab;
  // Tabs: the one shown is selected and the Tab stop; the arrows reach the other.
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
    b.setAttribute("aria-selected", String(b.dataset.tab === tab));
    b.tabIndex = b.dataset.tab === tab ? 0 : -1;
  }
  $("#tab-home").hidden = tab !== "home";
  $("#tab-settings").hidden = tab !== "settings";
  $("#dash-date").hidden = tab !== "home";
  $("#settings-version").hidden = tab !== "settings";
  if (tab === "home") drawGreeting();
  else $("#dash-title").textContent = "Settings";
  // The keyboard starts on the tab's first control, not nowhere.
  const into = tab === "home" ? document.querySelector<HTMLElement>(".app-open") : document.querySelector<HTMLElement>("#settings-find");
  if (into && (!document.activeElement || document.activeElement === document.body)) {
    into.focus({ preventScroll: true });
    // Today's first task takes it once drawn, if the keyboard is still here.
    if (tab === "home") startedOn = into;
  }
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
/// Shows a tab from the keyboard, the keyboard on its tab.
const goTab = (tab: Tab) => {
  void showTab(tab);
  document.querySelector<HTMLElement>(`[data-tab="${tab}"]`)?.focus();
};
// The tabs: the arrows move along them, as tabs do.
$(".dash-tabs").addEventListener("keydown", (e) => {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
  e.preventDefault();
  goTab(e.key === "ArrowLeft" || e.key === "Home" ? "home" : "settings");
});
// Ctrl+1 Home, Ctrl+2 or Ctrl+, Settings, as Ledger's views and most apps' settings.
document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
  const tab = ({ "1": "home", "2": "settings", ",": "settings" } as Record<string, Tab>)[e.key];
  if (!tab) return;
  e.preventDefault();
  goTab(tab);
});
followTab();
listen("kairo:dashboard", () => followTab());

// Tile lines stay current while the window is open.
setInterval(() => current === "home" && !document.hidden && refreshLines(), 30_000);
document.addEventListener("visibilitychange", () => current === "home" && !document.hidden && refreshLines());
