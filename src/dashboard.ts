import { jumpTo } from "./jump";
import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type FocusSession, type Task } from "./api";
import { cancelShortcutRecording, loadKairoSettings, shortcutKeys, wireKairoSettings } from "./kairo-settings";
import { useDropdowns } from "./dropdown";
import { loadLine, plannedMinutes } from "./ledger-cost";
import { ledgerLine } from "./ledger-tasks";
import { greeting } from "./logos";
import { glyph } from "./glyphs";
import { $, fmtDur, followTheme, h, savedTimerMinutes } from "./ui";

// Kairo's dashboard: the day on Home, with a chip for each app that keeps count, and the
// settings that belong to all of Kairo.

/// An app in Kairo. Home draws a chip for each (one that opens by shortcut, `hint`, is a line of
/// words under them instead), and App settings (on the Settings tab) a row for each that has
/// settings, so a new app is one more entry here: its settings are either on its own screen
/// (`open`, an Open button) or a section of Kairo's settings (`show`, the id of the control a
/// Show button brings into view).
interface KairoApp {
  id: string;
  name: string;
  /// Today's line beside its name: words, or what to draw (keys as keys, a Resume button).
  line: () => Promise<string | Node[]>;
  /// Opened by its shortcut: Home names it in a line of hints, not a chip.
  hint?: true;
  open: () => void;
  settings?: { covers: string; open: () => void; finds?: Find[] } | { covers: string; show: string };
}

/// A setting kept in an app's own window, for "Find a setting" to find: its name as the app
/// words it, other words it may be looked for by, and how to open the app on it.
interface Find {
  name: string;
  words: string;
  open: () => void;
}

/// Chrono's settings and where each is: the view it's on and the control to land on.
const CHRONO_FINDS: Find[] = (
  [
    ["Daily and weekly goals", "goal target productive distracting cap week", "settings", "goal-productive"],
    ["Daily focus target", "goal focus streak", "settings", "goal-focus"],
    ["Projects", "project keywords weekly target", "settings", "project-name"],
    ["Calendar", "meetings events ics address", "settings", "calendar-urls"],
    ["Browser extension", "sites chrome brave edge firefox", "settings", "ext-open"],
    ["Focus blocking", "block blocked distracting apps sites launchers", "settings", "block-enabled"],
    ["Strict mode", "end early wait blocking", "settings", "block-strict"],
    ["Breaks", "break reminder rest", "settings", "break-every"],
    ["Focus nudges", "nudge notification distracted", "settings", "nudges-enabled"],
    ["Evening summary", "notification daily recap", "settings", "evening-enabled"],
    ["Keep history for", "retention privacy older days forever", "settings", "retention"],
    ["Erase tracked time", "delete history clear remove privacy", "settings", "delete-range"],
    ["Demo data", "sample made-up try", "settings", "demo-load"],
    ["Keyboard", "arrow keys shortcuts days weeks", "settings", "day-keys"],
    ["Rounds and breaks", "pomodoro long break rounds", "focus", "pomodoro-break-min"],
    ["Focus schedule", "scheduled sessions weekly automatic", "focus", "schedule-start"],
    ["Block lists", "blocking list apps sites", "focus", "block-list-name"],
    ["Categories and limits", "category productive neutral distracting daily limit privacy hide title ignore", "categories", undefined],
  ] as const
).map(([name, words, view, target]) => ({ name, words, open: () => api.openChrono(view, target) }));

/// Today's local date, as Kairo's commands take it ("2026-10-29").
function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const APPS: KairoApp[] = [
  {
    id: "chrono",
    name: "Chrono",
    // Only today's two totals: the full day summary (timeline, apps, sites) is far more work to
    // build every 30 seconds.
    line: async () => {
      const [status, [day]] = await Promise.all([api.status(), api.dayTotals(today(), today())]);
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
      return `${fmtDur(day.activeMs)} today`;
    },
    open: () => api.openChrono(),
    settings: {
      covers: "Goals, privacy, projects, calendar, browser extension, nudges, evening summary, focus blocking, breaks and demo data",
      open: () => api.openChrono("settings"),
      finds: CHRONO_FINDS,
    },
  },
  {
    id: "ledger",
    name: "Ledger",
    line: async () => ledgerLine(await tasksNow()),
    open: () => api.openLedger(),
    settings: {
      covers: "Reminders and the morning reminder",
      open: () => api.openLedger("settings"),
    },
  },
  {
    id: "palette",
    name: "Command palette",
    hint: true,
    // Its keys, and one thing to try in it, a different one each day.
    line: async () => {
      const s = await api.paletteSettings();
      return [h("span", { class: "shortcut-keys" }, ...shortcutKeys(s.shortcut)), h("span", {}, tryToday(new Date()))];
    },
    open: () => api.openPalette(),
    settings: { covers: "Shortcut, suggestions and files", show: "palette-record" },
  },
  {
    id: "ring",
    name: "App ring",
    hint: true,
    line: async () => [h("span", { class: "shortcut-keys" }, ...shortcutKeys((await api.ringSettings()).shortcut))],
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

/// The focus session now running, if any, read once per refresh for the strip and the task rows.
type Running = Promise<FocusSession | null>;

const COUNT_WORDS = ["Nothing", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"];

/// The day in one line: what is left, or that nothing is.
export function dayTitle(left: number): string {
  if (left === 0) return "Nothing due today.";
  return `${left <= 10 ? COUNT_WORDS[left] : left} ${left === 1 ? "thing" : "things"} left today.`;
}

/// The title Home had last, put back at once on returning from Settings.
let homeTitle = "";

/// Names the focus session running now above the day, with a way to end it; nothing at all when
/// none is.
async function drawFocus(running: Running) {
  const strip = $("#focus-strip");
  let session: FocusSession | null;
  let tasks: Task[] = [];
  try {
    session = await running;
    if (session) tasks = await tasksNow();
  } catch {
    // What's shown stays; the next refresh tries again.
    return;
  }
  strip.hidden = !session;
  if (!session) return;
  const what = tasks.find((t) => t.id === session.task)?.title ?? session.label;
  const left = session.plannedMs - ((session.pausedAt ?? Date.now()) - session.startMs - session.pausedMs);
  const state = session.pausedAt ? " · paused" : session.plannedMs > 0 ? ` · ${Math.max(1, Math.ceil(left / 60_000))} min left` : "";
  $("#focus-text").textContent = `Focusing${what ? ` on ${what}` : ""}${state}`;
}

/// Home's Today: the open tasks due today or overdue, each opening in Ledger, and the next
/// reminder still to come today.
async function drawToday(running: Running) {
  const list = $("#today-list");
  const next = $("#today-next");
  let tasks: Task[];
  let busy: FocusSession | null;
  let usual: number | null;
  try {
    [tasks, busy, usual] = await Promise.all([tasksNow(), running, api.focusUsual().catch(() => null)]);
  } catch {
    // What's shown stays; the next refresh tries again. A title still blank gets the greeting.
    if (current === "home" && !homeTitle) drawGreeting();
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
      // Focus on it from here, until a session is already running.
      const go = busy
        ? ""
        : h("button", { class: "link-btn today-focus", "aria-label": `Focus on “${t.title}”`, title: `Focus ${savedTimerMinutes()} min` }, "Focus");
      if (go instanceof HTMLElement)
        go.addEventListener("click", async () => {
          await api.startFocus(savedTimerMinutes(), undefined, t.id).catch(() => {});
          void refreshLines();
        });
      return h("li", { class: "today-item" }, tick, open, go);
    }),
  );
  // The title says how the day stands.
  homeTitle = dayTitle(due.length);
  if (current === "home") $("#dash-title").textContent = homeTitle;
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
  const nextReminder = reminder ? `Next reminder at ${new Date(reminder.remindMs!).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}: ${reminder.title}` : "";
  // The day's estimates against a usual day's focus, when any task due has one.
  next.textContent = [loadLine(plannedMinutes(tasks), usual), nextReminder].filter(Boolean).join(" · ");
}

/// The greeting for this hour, for a day that can't be read.
function drawGreeting() {
  const g = greeting(new Date().getHours());
  $("#dash-title").replaceChildren(`${g.lead} `, h("em", {}, g.aside));
}

type Tab = "home" | "settings";
let current: Tab | null = null;
let settingsWired = false;

function drawHome() {
  // A chip for each app that keeps count: its name is the button, stretched over the chip, so the
  // dot, the name and the line are one target, and the name is read once. The line is not a live
  // region: refreshed every 30 seconds, it would talk over whatever is read.
  $("#app-tiles").replaceChildren(
    ...APPS.filter((app) => !app.hint).map((app) => {
      const open = h("button", { class: "app-open" }, h("span", { class: "app-dot", "aria-hidden": "true" }), h("span", { class: "sr-only" }, "Open "), app.name);
      open.addEventListener("click", app.open);
      return h("div", { class: "app-chip", "data-app": app.id }, open, h("span", { class: "app-chip-line", "data-line": app.id }));
    }),
  );
  // The ones opened by their keys are a line of words beneath: each named, with its keys.
  $("#dash-hint").replaceChildren(
    ...APPS.filter((app) => app.hint).map((app) => {
      const open = h("button", { class: "link-btn hint-open" }, app.name);
      open.addEventListener("click", app.open);
      return h("span", { class: "hint-item", "data-app": app.id }, open, h("span", { class: "hint-line", "data-line": app.id }));
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
      // What "Find a setting" turns up inside the app, each opening the app on it; shown only
      // while it matches what's typed (kairo-settings' find).
      const finds = h(
        "ul",
        { class: "app-finds", "aria-label": `Found in ${app.name}`, hidden: "" },
        ...("finds" in s && s.finds ? s.finds : []).map((f) => {
          const open = h("button", { class: "link-btn" }, f.name);
          open.addEventListener("click", f.open);
          return h("li", { "data-finds": `${f.name} ${f.words}`.toLowerCase(), hidden: "" }, open);
        }),
      );
      return h("li", {}, h("div", {}, h("strong", {}, app.name), h("p", { class: "muted" }, s.covers), finds), go);
    }),
  );
}

/// Asks each app for today's line; one that can't answer keeps what it showed.
async function refreshLines() {
  tasksRead = null;
  // Read once for the strip and the task rows; one that can't be read counts as none running.
  const running: Running = api.focus().catch(() => null);
  await Promise.all([
    drawToday(running),
    drawFocus(running),
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
  $("#dash-title").textContent = tab === "home" ? homeTitle : "Settings";
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
drawHome();
drawAppSettings();
// Focus rings stay out of the way until a key is pressed: the keyboard starts on Home's first
// control, which no one reached by keyboard yet, and a ring there reads as a stray highlight.
document.addEventListener("keydown", () => document.body.classList.add("keys"), { capture: true, once: true });
$("#focus-stop").addEventListener("click", async () => {
  const r = await api.controlFocus("stop").catch(() => null);
  // Strict mode holds an early end back a little while: say until when.
  if (r?.waitUntil) $("#focus-text").textContent = `Strict mode: you can end this at ${new Date(r.waitUntil).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  else void refreshLines();
});
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
