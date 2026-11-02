import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type Task } from "./api";
import { renderDetail, renderTaskList, type LedgerActions, type RunningSession } from "./ledger-tasks";
import { $, followTheme } from "./ui";

/// Chrono's timer keeps the length picked for the next session here; Start focus uses it too.
const TIMER_KEY = "kairo.timerMinutes";

function sessionMinutes(): number {
  try {
    const saved = Number(localStorage.getItem(TIMER_KEY));
    return saved >= 5 && saved <= 240 ? saved : 25;
  } catch {
    return 25;
  }
}

let tasks: Task[] = [];
let session: RunningSession = null;
let selected: string | null = null;

function draw(list?: Task[]) {
  if (list) tasks = list;
  renderTaskList($("#task-list"), tasks, session, actions);
  const panel = $("#task-detail");
  const t = tasks.find((x) => x.id === selected);
  // Typing in the panel isn't undone by a redraw from elsewhere.
  if (t && !panel.contains(document.activeElement)) renderDetail(panel, t, actions);
  panel.hidden = !t;
  if (!t) selected = null;
}

async function refresh() {
  const [list, running] = await Promise.all([api.tasks(), api.focus().catch(() => null)]);
  session = running ? { task: running.task } : null;
  draw(list);
}

const actions: LedgerActions = {
  add: async (text) => draw(await api.addTask(text)),
  preview: async (text) => (await api.parseTask(text)).shown,
  done: async (id, done) => draw(await api.setTaskDone(id, done)),
  start: async (id) => {
    await api.startFocus(sessionMinutes(), undefined, id);
    await refresh();
  },
  select: (id) => {
    selected = selected === id ? null : id;
    draw();
  },
  rename: async (id, title) => draw(await api.renameTask(id, title)),
  plan: async (id, due, priority, remind) => {
    const list = await api.setTaskPlan(id, due, priority, remind);
    // The panel shows the new plan, though it has the focus.
    selected = id;
    tasks = list;
    const t = list.find((x) => x.id === id);
    if (t) renderDetail($("#task-detail"), t, actions);
    draw();
  },
  remove: async (id) => {
    if (selected === id) selected = null;
    draw(await api.removeTask(id));
  },
};

type View = "tasks" | "settings";

function show(view: View) {
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-view]")) b.setAttribute("aria-selected", String(b.dataset.view === view));
  $("#view-tasks").hidden = view !== "tasks";
  $("#view-settings").hidden = view !== "settings";
  $("#ledger-title").textContent = view === "tasks" ? "Tasks" : "Settings";
  if (view === "settings") void loadSettings();
}

async function loadSettings() {
  const s = await api.ledgerSettings();
  $<HTMLInputElement>("#ledger-reminders").checked = s.reminders;
  $<HTMLInputElement>("#ledger-morning").value = s.morning;
}

async function saveSettings() {
  const note = $("#ledger-settings-note");
  try {
    const s = await api.setLedgerSettings($<HTMLInputElement>("#ledger-reminders").checked, $<HTMLInputElement>("#ledger-morning").value);
    $<HTMLInputElement>("#ledger-morning").value = s.morning;
    note.textContent = "";
  } catch (e) {
    note.textContent = String(e);
  }
}

followTheme();
$("#ledger-date").textContent = new Date().toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
for (const b of document.querySelectorAll<HTMLButtonElement>("[data-view]")) b.addEventListener("click", () => show(b.dataset.view as View));
$("#ledger-reminders").addEventListener("change", saveSettings);
$("#ledger-morning").addEventListener("change", saveSettings);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && selected) {
    selected = null;
    draw();
  }
});
void refresh();
// Tasks change from Chrono, the palette and other computers too; and sessions start and end.
listen("kairo:tasks", () => refresh());
listen("kairo:sync", () => refresh());
setInterval(() => !document.hidden && refresh(), 10_000);
document.addEventListener("visibilitychange", () => !document.hidden && refresh());
