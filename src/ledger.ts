import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api, type Task } from "./api";
import { useDropdowns } from "./dropdown";
import { mountNotes, type NotesView } from "./ledger-notes";
import type { NoteChange } from "./note-popout";
import { renderDetail, renderTaskList, taskSummary, type LedgerActions, type NoteName, type RunningSession } from "./ledger-tasks";
import { logo } from "./logos";
import { replaceTimeInputs } from "./time-select";
import { $, followTheme } from "./ui";
import { mountUndo } from "./undo";

// Times of day as Kairo's dropdowns rather than the system's time fields; first, before anything
// takes hold of the fields it replaces.
replaceTimeInputs();

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

/// The panel opened on `id`: level with its card, so it opens in view wherever the task is in
/// the list, and the keyboard in its title.
function openedPanel(id: string) {
  const panel = $("#task-detail");
  const card = document.querySelector<HTMLElement>(`#task-list [data-task="${CSS.escape(id)}"]`);
  const room = $("#view-tasks").clientHeight - panel.offsetHeight;
  panel.style.top = card ? `${Math.max(0, Math.min(card.offsetTop - 8, room))}px` : "0px";
  panel.querySelector<HTMLInputElement>(".detail-title")?.focus();
}

/// Closes the panel, the keyboard back on the task it was open on.
function closePanel() {
  const was = selected;
  selected = null;
  draw();
  if (was) document.querySelector<HTMLElement>(`#task-list [data-task="${CSS.escape(was)}"] .task-title`)?.focus();
}

/// A task deleted a moment ago: gone from the list at once, deleted for good when the next
/// delete comes, the view is left or the window closes. Until then Undo (or Ctrl+Z) brings it back.
const taskUndo = mountUndo($("#task-undo"));
let deletingTask: string | null = null;

/// The tasks as shown: one waiting on its Undo is left out.
const shownTasks = () => tasks.filter((t) => t.id !== deletingTask);

const titleOf = (id: string) => document.querySelector<HTMLElement>(`#task-list [data-task="${CSS.escape(id)}"] .task-title`);

/// A line saying what didn't work, until the next thing does.
function say(text: string) {
  $("#task-status").textContent = text;
}

/// Runs a task action, saying so if it fails rather than leaving it looking done.
async function tried(what: string, run: () => Promise<void>) {
  try {
    await run();
    say("");
  } catch {
    say(`Couldn't ${what}. Try again in a moment.`);
    void refresh();
  }
}

function draw(list?: Task[]) {
  if (list) tasks = list;
  const visible = shownTasks();
  renderTaskList($("#task-list"), visible, session, actions, selected);
  const panel = $("#task-detail");
  const t = visible.find((x) => x.id === selected);
  // Typing in the panel isn't undone by a redraw from elsewhere.
  if (t && !panel.contains(document.activeElement)) renderDetail(panel, t, actions, noteNames);
  panel.hidden = !t;
  // The panel slides over the list, which keeps its width.
  $("#view-tasks").classList.toggle("detail-open", !!t);
  if (!t) selected = null;
  $("#count-tasks").textContent = String(visible.filter((x) => x.doneMs === null).length || "");
  if (shown === "tasks") $("#ledger-summary").textContent = taskSummary(visible);
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
  // A failure is the add field's to say: it keeps what was typed.
  add: async (text) => {
    draw(await api.addTask(text));
    say("");
  },
  preview: async (text) => (await api.parseTask(text)).shown,
  done: (id, done) => tried(done ? "tick it off" : "reopen it", async () => draw(await api.setTaskDone(id, done))),
  start: (id) =>
    tried("start the focus session", async () => {
      await api.startFocus(sessionMinutes(), undefined, id);
      await refresh();
    }),
  select: (id) => {
    if (selected === id) return closePanel();
    selected = id;
    draw();
    openedPanel(id);
  },
  rename: (id, title) => tried("rename it", async () => draw(await api.renameTask(id, title))),
  plan: (id, due, priority, remind) =>
    tried("change it", async () => {
      const list = await api.setTaskPlan(id, due, priority, remind);
      // The panel shows the new plan, though it has the focus.
      selected = id;
      tasks = list;
      const t = list.find((x) => x.id === id);
      if (t) renderDetail($("#task-detail"), t, actions, noteNames);
      draw();
    }),
  remove: (id) => {
    if (selected === id) selected = null;
    const visible = shownTasks();
    const at = visible.findIndex((x) => x.id === id);
    // The keyboard's next place once it's gone for good: the task after it, or before it.
    const neighbour = visible[at + 1] ?? visible[at - 1];
    taskUndo.offer({
      what: visible[at]?.title ?? "the task",
      finish: () => {
        deletingTask = null;
        void api.removeTask(id).then(
          (list) => draw(list),
          () => {
            say("Couldn't delete it, so it's back in the list.");
            void refresh();
          },
        );
      },
      undo: () => {
        deletingTask = null;
        draw();
        titleOf(id)?.focus();
      },
      focusAfter: () => (neighbour && titleOf(neighbour.id)) || document.querySelector<HTMLElement>(".task-add"),
    });
    deletingTask = id;
    draw();
  },
  focusMinutes: () => sessionMinutes(),
  openNote: (id) => void openNote(id),
  attach: (task, note) =>
    tried(note ? "attach the note" : "detach the note", async () => {
      const list = await api.setTaskNote(task, note);
      selected = task;
      tasks = list;
      const t = list.find((x) => x.id === task);
      if (t) renderDetail($("#task-detail"), t, actions, noteNames);
      draw();
    }),
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
    setPaper: (id, paper) => api.setNotePaper(id, paper),
    remove: (id) => api.removeNote(id),
    openTask: (id) => openTask(id),
    popOut: (id) => void api.popOutNote(id),
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
  // Kairo's App settings: Ledger's own.
  if (kind === "settings") show("settings");
  // The ring's New task: the Tasks view, ready to type.
  if (kind === "add") {
    show("tasks");
    document.querySelector<HTMLInputElement>(".task-add")?.focus();
  }
}

type View = "tasks" | "notes" | "settings";
const VIEWS: View[] = ["tasks", "notes", "settings"];

/// The tabs as one Tab stop, the chosen one; the arrows move between them.
function markTabs() {
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-view]")) {
    const on = b.dataset.view === shown;
    b.setAttribute("aria-selected", String(on));
    b.tabIndex = on ? 0 : -1;
  }
}

function show(view: View) {
  // Leaving a view makes a delete waiting on its Undo there final.
  if (view !== "tasks") taskUndo.finish();
  if (view !== "notes") notesView?.finishDelete();
  shown = view;
  markTabs();
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
  morningFollowsReminders();
}

/// The morning reminder's time means nothing with reminders off.
function morningFollowsReminders() {
  $<HTMLInputElement>("#ledger-morning").disabled = !$<HTMLInputElement>("#ledger-reminders").checked;
}

async function saveSettings() {
  const note = $("#ledger-settings-note");
  try {
    const s = await api.setLedgerSettings($<HTMLInputElement>("#ledger-reminders").checked, $<HTMLInputElement>("#ledger-morning").value);
    $<HTMLInputElement>("#ledger-morning").value = s.morning;
    note.textContent = "";
  } catch (e) {
    note.textContent = `Couldn't save this: ${e}`;
  }
}

followTheme();
// Kairo's own dropdowns in place of the system's lists.
useDropdowns();
document.querySelector(".wordmark")?.insertAdjacentHTML("afterbegin", logo("ledger", 30));
// Kairo is one window: this goes back to its dashboard.
$("#back-to-kairo").addEventListener("click", () => api.openDashboard());
$("#ledger-date").textContent = new Date().toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
for (const b of document.querySelectorAll<HTMLButtonElement>("[data-view]")) b.addEventListener("click", () => show(b.dataset.view as View));
$("#ledger-reminders").addEventListener("change", () => {
  morningFollowsReminders();
  void saveSettings();
});
$("#ledger-morning").addEventListener("change", saveSettings);
markTabs();
// The tabs: the arrows move along them, as tabs do; Home and End go to the ends.
$(".tabs").addEventListener("keydown", (e) => {
  const at = VIEWS.indexOf(shown);
  const to = ({ ArrowDown: at + 1, ArrowRight: at + 1, ArrowUp: at - 1, ArrowLeft: at - 1, Home: 0, End: VIEWS.length - 1 } as Record<string, number>)[e.key];
  if (to === undefined) return;
  e.preventDefault();
  const view = VIEWS[(to + VIEWS.length) % VIEWS.length];
  show(view);
  document.querySelector<HTMLElement>(`[data-view="${view}"]`)?.focus();
});

/// Whether a key press belongs to something being typed in.
const typingIn = (t: EventTarget | null) => t instanceof HTMLElement && !!t.closest("input, textarea, select, [contenteditable]");

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && selected) return closePanel();
  const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
  // Ctrl+1 to 3: the views.
  if (ctrl && ["1", "2", "3"].includes(e.key)) {
    e.preventDefault();
    return show(VIEWS[Number(e.key) - 1]);
  }
  // Ctrl+N: a new note in Notes; elsewhere, the add field, ready to type a task.
  if (ctrl && e.key.toLowerCase() === "n") {
    e.preventDefault();
    if (shown === "notes") return void notesView?.create();
    show("tasks");
    return document.querySelector<HTMLInputElement>(".task-add")?.focus();
  }
  // "/": the notes' search in Notes, the add field in Tasks.
  if (e.key === "/" && !ctrl && !typingIn(e.target)) {
    e.preventDefault();
    if (shown === "notes") document.querySelector<HTMLInputElement>(".notes-search")?.focus();
    else if (shown === "tasks") document.querySelector<HTMLInputElement>(".task-add")?.focus();
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
// A note changed in its pop-out window: taken unless it's being typed in here.
listen<NoteChange>("kairo:note", (e) => {
  const { id, from, gone } = e.payload;
  if (from === "kairo") return;
  // Notes are read again only while they show (opening the view reads them anyway), so the
  // pop-out's every save doesn't reload Ledger.
  if (shown === "notes") void notesView?.refresh();
  // The sidebar's count and the task panel's note names: only a note made or deleted elsewhere
  // changes them.
  if (gone || !noteNames.some((n) => n.id === id)) void refresh();
});
setInterval(() => !document.hidden && refresh(), 10_000);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) void notesView?.flush();
  else void refresh();
});
// Typed here is in the pop-out before you can type there; closing the window keeps it too.
window.addEventListener("blur", () => void notesView?.flush());
window.addEventListener("beforeunload", () => void notesView?.flush());
window.addEventListener("pagehide", () => void notesView?.flush());
// Closing the window makes a delete waiting on its Undo final.
window.addEventListener("pagehide", () => {
  taskUndo.finish();
  notesView?.finishDelete();
});
