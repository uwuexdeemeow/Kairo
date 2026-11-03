import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type Task } from "./api";
import { mountNotes, type NotesView } from "./ledger-notes";
import { renderDetail, renderTaskList, taskSummary, type LedgerActions, type NoteName, type RunningSession } from "./ledger-tasks";
import { logo } from "./logos";
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
/// The view showing: the summary line is the Tasks view's.
let shown: "tasks" | "notes" | "settings" = "tasks";

/// The notes' names, for the task panel's Note row.
let noteNames: NoteName[] = [];

function draw(list?: Task[]) {
  if (list) tasks = list;
  renderTaskList($("#task-list"), tasks, session, actions);
  const panel = $("#task-detail");
  const t = tasks.find((x) => x.id === selected);
  // Typing in the panel isn't undone by a redraw from elsewhere.
  if (t && !panel.contains(document.activeElement)) renderDetail(panel, t, actions, noteNames);
  panel.hidden = !t;
  // The panel slides over the list, which keeps its width.
  $("#view-tasks").classList.toggle("detail-open", !!t);
  if (!t) selected = null;
  $("#count-tasks").textContent = String(tasks.filter((x) => x.doneMs === null).length || "");
  if (shown === "tasks") $("#ledger-summary").textContent = taskSummary(tasks);
}

async function refresh() {
  const [list, running, notes] = await Promise.all([api.tasks(), api.focus().catch(() => null), api.notes().catch(() => null)]);
  session = running ? { task: running.task } : null;
  if (notes) {
    noteNames = notes.map((n) => ({ id: n.id, title: n.title }));
    $("#count-notes").textContent = String(notes.length || "");
  }
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
    if (t) renderDetail($("#task-detail"), t, actions, noteNames);
    draw();
  },
  remove: async (id) => {
    if (selected === id) selected = null;
    draw(await api.removeTask(id));
  },
  openNote: (id) => void openNote(id),
  attach: async (task, note) => {
    const list = await api.setTaskNote(task, note);
    selected = task;
    tasks = list;
    const t = list.find((x) => x.id === task);
    if (t) renderDetail($("#task-detail"), t, actions, noteNames);
    draw();
  },
  newNoteFor: async (task, title) => {
    const n = await api.newNote(`# ${title}

`);
    noteNames = [{ id: n.id, title: n.title }, ...noteNames];
    await actions.attach(task, n.id);
    await openNote(n.id);
  },
};

let notesView: NotesView | null = null;

/// The Notes view, made the first time it's shown.
function notes(): NotesView {
  notesView ??= mountNotes($("#view-notes"), {
    search: (q) => api.notes(q),
    create: () => api.newNote(""),
    save: (id, body) => api.saveNote(id, body),
    pin: (id, pinned) => api.pinNote(id, pinned),
    remove: (id) => api.removeNote(id),
    openTask: (id) => openTask(id),
    syncSoon: () => void api.syncSoon(),
  });
  return notesView;
}

async function openNote(id: string) {
  show("notes");
  await notes().open(id);
}

function openTask(id: string) {
  show("tasks");
  selected = id;
  void refresh();
}

/// Goes where whoever opened the window asked: a note or a task.
async function followOpen() {
  const asked = await api.takeLedgerOpen().catch(() => null);
  const [kind, id] = (asked ?? "").split(":");
  if (kind === "note" && id) await openNote(id);
  if (kind === "task" && id) openTask(id);
  // The ring's New task: the Tasks view, ready to type.
  if (kind === "add") {
    show("tasks");
    document.querySelector<HTMLInputElement>(".task-add")?.focus();
  }
}

type View = "tasks" | "notes" | "settings";

function show(view: View) {
  shown = view;
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-view]")) b.setAttribute("aria-selected", String(b.dataset.view === view));
  $("#view-tasks").hidden = view !== "tasks";
  $("#view-notes").hidden = view !== "notes";
  $("#view-settings").hidden = view !== "settings";
  $("#ledger-title").textContent = { tasks: "Tasks", notes: "Notes", settings: "Settings" }[view];
  $("#ledger-summary").textContent = view === "tasks" ? taskSummary(tasks) : "";
  // A note typed in is kept before another view takes the window.
  if (view !== "notes") void notesView?.flush();
  if (view === "notes") void notes().refresh();
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
document.querySelector(".wordmark")?.insertAdjacentHTML("afterbegin", logo("ledger", 30));
// Kairo is one window: this goes back to its dashboard.
$("#back-to-kairo").addEventListener("click", () => api.openDashboard());
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
void refresh().then(followOpen);
// Tasks change from Chrono, the palette and other computers too; and sessions start and end.
listen("kairo:tasks", () => refresh());
listen("kairo:sync", () => {
  void refresh();
  void notesView?.refresh();
});
listen("kairo:ledger", () => followOpen());
setInterval(() => !document.hidden && refresh(), 10_000);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) void notesView?.flush();
  else void refresh();
});
// Closing the window keeps what was typed.
window.addEventListener("beforeunload", () => void notesView?.flush());
window.addEventListener("pagehide", () => void notesView?.flush());
