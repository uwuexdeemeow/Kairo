import { invoke } from "@tauri-apps/api/core";
import type { RingSettings } from "./ring-editor";

export type Category = "productive" | "neutral" | "distracting";

/** Combining computers: how (a folder, or the relay), and the computers combined. */
export interface SyncState {
  mode: "off" | "folder" | "relay";
  folder: string | null;
  computers: { id: string; name: string; updatedMs: number; thisOne: boolean }[];
  name: string;
  /** Whose time the views show: "all", "this" or a computer's id. */
  shown: string;
  /** When this computer last synced through the relay. */
  syncedMs: number | null;
  /** What went wrong with the relay since, or "unlinked" when another computer started over. */
  error: string | null;
}

/** A code Settings shows for another computer to type, and how it's going. */
export type Linking = { code: string; expiresMs: number } & (
  | { state: "waiting" | "expired" | "mismatch" }
  | { state: "linked"; name: string }
  | { state: "failed"; message: string }
);

export interface Usage {
  /** `app:<exe>` or `site:<name>`, for the icon. */
  key: string;
  label: string;
  category: Category;
  ms: number;
}

export interface Segment {
  start: number;
  end: number;
  category: Category;
  label: string;
}

export interface DaySummary {
  activeMs: number;
  productiveMs: number;
  neutralMs: number;
  distractingMs: number;
  switches: number;
  longestFocusMs: number;
  deepBlocks: number;
  topApps: Usage[];
  topSites: Usage[];
  timeline: Segment[];
}

export interface DayTotal {
  date: string;
  activeMs: number;
  productiveMs: number;
  distractingMs: number;
}

export interface HourStat {
  hour: number;
  activeMs: number;
  productiveMs: number;
  neutralMs: number;
  distractingMs: number;
  switches: number;
}

export interface Cell {
  activeMs: number;
  productiveMs: number;
  distractingMs: number;
}

export interface Insight {
  kind: "warning" | "positive" | "info";
  title: string;
  detail: string;
  metric: string;
  unit: string;
}

export interface Patterns {
  daysWithData: number;
  /** Averages per day with meaningful activity. */
  summary: {
    avgActiveMs: number;
    avgProductiveMs: number;
    avgDistractingMs: number;
    focusSessions: number;
    avgFocusMs: number;
  };
  hourly: HourStat[];
  heatmap: Cell[][];
  topApps: Usage[];
  topSites: Usage[];
  insights: Insight[];
}

export interface FocusControl {
  /** The session as it now stands; null once it has ended. */
  session: FocusSession | null;
  /** Set when strict mode held "end early" back: ask again from this time to confirm. */
  waitUntil: number | null;
}

export interface Status {
  tracking: boolean;
  app: string | null;
  sinceMs: number | null;
}

export interface HeadsUp {
  title: string;
  detail: string;
}

export interface Autostart {
  available: boolean;
  enabled: boolean;
}

export type PrivacyMode = "track" | "hide_title" | "ignore";

export interface CategoryEntry {
  key: string;
  label: string;
  kind: "app" | "site";
  category: Category;
  privacy: PrivacyMode;
  /** Daily limit in minutes; 0 means none. */
  limitMin: number;
  /** Whether passing the limit blocks it for the rest of the day. */
  limitHard: boolean;
  /** During focus sessions: "auto" blocks it when its category is distracting. */
  focus: FocusRule;
  ms: number;
}

export type FocusRule = "auto" | "block" | "allow";

export interface PaletteItem {
  title: string;
  /** Characters of the title to underline. */
  at: number[];
  sub: string | null;
  detail: string | null;
  icon:
    | { kind: "app"; path: string }
    | { kind: "file"; path: string; fileKind: "folder" | "image" | "document" | "other" }
    | { kind: "glyph"; glyph: string; tone: "" | "accent" | "warm" };
  section: string;
  /** The label of what Ctrl+Enter (Command+Enter on a Mac) does instead. */
  second: string | null;
  /** Whether it has a card to show while it's selected (`paletteCard`). */
  preview: boolean;
  /** For a card that waits for Enter (a time chart): the row's second line once it's showing. */
  again?: string | null;
  /** Whether it only says something ("Not a sum yet"): drawn as a note, never chosen. */
  note?: boolean;
}

/** File rows found for the results numbered `seq`, sent as `palette:files` after them; they
 *  follow the rows, so the first is item `rows.length`. */
export interface PaletteFiles {
  seq: number;
  rows: PaletteItem[];
  /** In Files mode they take the place of the rows (its "Looking for files…" row). */
  replace?: boolean;
}

/** A mode the palette is in, drawn as a bubble at the left of the input; its `key` goes back with
 *  each query typed after it. */
export interface PaletteMode {
  key: string;
  name: string;
  glyph: string;
  /** The input's hint while it's on. */
  hint: string;
}

/** Items are numbered across rows, tiles and chips, in that order. */
export interface PaletteResults {
  seq: number;
  rows: PaletteItem[];
  tiles: PaletteItem[];
  chips: PaletteItem[];
  /** The mode these were found in; none for an ordinary search. */
  mode?: PaletteMode | null;
  /** What the input should hold now, when a short word typed at its start became the bubble. */
  text?: string | null;
}

/** The card above the results for a selected item: how a day went, time in an app or site over
 *  a period, or what a focus session would do. */
export type PaletteCard =
  | {
      kind: "day";
      activeMs: number;
      productivePct: number;
      focusCount: number;
      focusMs: number;
      /** Stretches of the day as shares of it from midnight. */
      segments: { from: number; to: number; category: Category }[];
    }
  | {
      kind: "answer";
      /** "THE FINALS this week". */
      title: string;
      /** What to draw its icon from, as for an app in the results. */
      icon: string | null;
      /** "Mon 19 – Sun 25 October · distracting". */
      range: string;
      category: Category;
      totalMs: number;
      /** "−1h 30m vs last week". */
      change: string | null;
      /** Whether the change is for the better. */
      better: boolean | null;
      /** A bar a day, or an hour for a single day. */
      bars: { label: string; ms: number; over: boolean }[];
      /** The daily limit, when the bars are days and there is one. */
      limitMs: number | null;
      note: string;
      text: string;
    }
  | {
      kind: "focus";
      title: string;
      /** "Labelled “essay” · ends at 4:20 pm". */
      sub: string;
      blocking: string;
      launchers: string;
      strict: string;
      /** A session is running, which this one would end. */
      replaces: boolean;
    };

/** "system" follows Windows' light or dark setting. */
/** The palette as it opens: everything, or only its search bar. */
export type PaletteLook = "full" | "compact";

/** "system", or a ground's id (src/themes.ts). */
export type Appearance = string;

export interface GoalProgress {
  key: string;
  label: string;
  /** "productive" and "focus" are targets to reach; the others are caps not to pass. */
  kind: "productive" | "distracting" | "limit" | "focus";
  usedMs: number;
  targetMs: number;
  met: boolean;
  /** The focus target's own streak, in days before this one; null for the rest. */
  streak: number | null;
}

export interface DayGoals {
  items: GoalProgress[];
  /** Completed days in a row, before this one, with every goal met. */
  streak: number;
}

export interface WeekTotals {
  activeMs: number;
  productiveMs: number;
  neutralMs: number;
  distractingMs: number;
}

/** A week or a month. */
export interface Period {
  /** The first day (a Monday, or the 1st), as a local `YYYY-MM-DD`. */
  start: string;
  /** The last day. */
  end: string;
  totals: WeekTotals;
  /** Last week, or last month: as much of it as this one has had, while this one is under way. */
  previous: WeekTotals;
  /** The last day of that stretch counted (to the same time of day), when it isn't all of it. */
  previousEnd: string | null;
  days: { date: string; totals: WeekTotals; goalsMet: boolean | null }[];
  bestDay: string | null;
  goalDaysMet: number;
  goalDays: number;
  /** For a month, its finished weeks that met every weekly goal, of those tracked; 0 for a week. */
  goalWeeksMet: number;
  goalWeeks: number;
  topApps: { key: string; label: string; category: Category; ms: number; previousMs: number }[];
  topProjects: ProjectTime[];
  limitsOver: { label: string; overMs: number; daysOver: number }[];
  focusSessions: number;
  focusMs: number;
}

export interface FocusSession {
  startMs: number;
  plannedMs: number;
  /** Time paused so far, not counting a pause still in progress. */
  pausedMs: number;
  /** Set while paused. */
  pausedAt: number | null;
  label: string | null;
  /** The task it was started on, by id. */
  task: string | null;
}

/** Where Ledger lists a task. */
export type TaskGroup = "overdue" | "today" | "upcoming" | "none" | "done";

/** A task in Ledger; `focusedMs` is this computer's focus time on it. */
export interface Task {
  id: string;
  title: string;
  createdMs: number;
  doneMs: number | null;
  position: number;
  focusedMs: number;
  /** Local day, "2026-11-07". */
  dueDay: string | null;
  /** 0 Low, 1 Medium, 2 High. */
  priority: number;
  remindMs: number | null;
  reminded: string;
  /** The note it's attached to, by id. */
  note: string | null;
  /** Its checklist. */
  steps: Step[];
  /** How it comes back once ticked; null for never. */
  repeat: Repeat | null;
  /** Lowercase tag words. */
  tags: string[];
  /** The Chrono project it belongs to, by name; null for none. */
  project: string | null;
  group: TaskGroup;
}

/** A meeting from a connected calendar. */
export interface CalendarEvent {
  startMs: number;
  endMs: number;
  title: string;
}

/** One line of a task's checklist. */
export interface Step {
  t: string;
  d: boolean;
}

export type Repeat = "day" | "weekday" | "week" | "month" | "year";

/** A note in Ledger; `title` is its first line with text, `tasks` the open ones attached. */
export interface Note {
  id: string;
  title: string;
  body: string;
  createdMs: number;
  editedMs: number;
  pinned: boolean;
  /** What it's written on. */
  paper: Paper;
  tasks: [string, string][];
}

export type Paper = "lined" | "blank" | "dotted" | "grid";

/** Quick-add text as read: the title, its shortcuts, and "High · Fri 7 Nov". */
export interface ParsedTask {
  title: string;
  due: string | null;
  priority: number | null;
  repeat: Repeat | null;
  tags: string[];
  shown: string;
}

export interface LedgerSettings {
  reminders: boolean;
  /** "09:00" */
  morning: string;
}

export interface FocusBlock {
  /** The pomodoro cycle it was a round of, if it was. */
  cycle: number | null;
  start: number;
  end: number;
  plannedMs: number;
  pausedMs: number;
  /** False when the session was ended early. */
  completed: boolean;
  label: string | null;
}

export interface FocusStats {
  sessions: number;
  completed: number;
  focusedMs: number;
}

export interface FocusOverview {
  today: FocusStats;
  week: FocusStats;
  all: FocusStats;
  /** Newest first. */
  recent: FocusBlock[];
  /** The last session's wrap-up, until it's dismissed or another starts. */
  wrapUp: FocusWrapUp | null;
  /** Today against the daily focus target; null with none set. */
  target: FocusTarget | null;
  /** The pomodoro cycle that's on, a round running or a break; null with none. */
  cycle: PomodoroStatus | null;
}

/** A pomodoro cycle that's on. */
export interface PomodoroStatus {
  /** The round running, or on a break the one next, from 1. */
  round: number;
  rounds: number;
  onBreak: boolean;
  longBreak: boolean;
  breakLeftMs: number;
  /** The break is over and the next round waits for you to come back. */
  waiting: boolean;
}

export interface PomodoroLengths {
  workMin: number;
  breakMin: number;
  longMin: number;
  /** Rounds before the long break. */
  rounds: number;
}

/** A whole cycle, added to the wrap-up of its last round. */
export interface CycleSummary {
  rounds: number;
  early: number;
  focusedMs: number;
  breakMs: number;
  nudges: number;
  pulled: { label: string; ms: number }[];
}

/** How a session that ended went. */
export interface FocusWrapUp {
  start: number;
  end: number;
  plannedMs: number;
  /** The session less its pauses. */
  focusedMs: number;
  /** False when it was ended early. */
  completed: boolean;
  label: string | null;
  nudges: number;
  /** Distracting apps and sites in front while it ran, biggest first. */
  pulled: { label: string; ms: number }[];
  /** Lines comparing it with the sessions before it. */
  versus: string[];
  /** The whole pomodoro cycle, when it was the last round of one with at least two rounds. */
  cycle: CycleSummary | null;
}

export interface FocusTarget {
  targetMs: number;
  usedMs: number;
  met: boolean;
  /** Days in a row before today on which it was reached. */
  streak: number;
}

export type FocusAction = "stop" | "pause" | "resume" | "extend";

export interface GoalSettings {
  productiveMin: number;
  distractingMaxMin: number;
  weekProductiveMin: number;
  weekDistractingMaxMin: number;
  /** The daily focus target. */
  focusMin: number;
}

/** A project: time whose window title or site has one of its keywords. */
export interface Project {
  id: number;
  name: string;
  keywords: string[];
  /** Minutes to put in each week; 0 means no target. */
  weekTargetMin: number;
}

export interface ProjectTime {
  id: number;
  name: string;
  ms: number;
}

/** A project's time taken apart: the apps and sites and window titles it came from, and a bar an
 *  hour (for a day) or a day (for a week or month). */
export interface ProjectDetail {
  ms: number;
  sources: Usage[];
  titles: { title: string; ms: number }[];
  bars: { startMs: number; ms: number }[];
}

export type Span = "day" | "week" | "month" | "year";

/** A week's review notes. */
export interface WeekNote {
  well: string;
  change: string;
}

/** The calendars Kairo reads, and how the last reading went. */
export interface CalendarStatus {
  urls: string[];
  readMs: number | null;
  /** Meetings within two weeks either side of the reading. */
  events: number;
  error: string | null;
}

/** A stretch away from the computer. */
export interface AwayStretch {
  startMs: number;
  endMs: number;
}

/** A focus session that starts on its own each week. */
export interface FocusSlot {
  id: number;
  /** Monday in bit 0 to Sunday in bit 6. */
  days: number;
  /** Minutes after midnight. */
  startMin: number;
  minutes: number;
  label: string | null;
  /** `app:` and `site:` keys this slot blocks, and nothing else; null blocks as usual. */
  blocks: string[] | null;
  /** A saved block list it blocks instead, by name. */
  list: string | null;
}

/** A named set of apps and sites to block instead of everything distracting. */
export interface BlockList {
  name: string;
  keys: string[];
}

export interface BlockLists {
  lists: BlockList[];
  /** The list sessions started by hand block; null blocks as usual. */
  default: string | null;
}

export type GoalName = "productive" | "distracting" | "week-productive" | "week-distracting" | "focus";

export type DeleteRange = "hour" | "day" | "all";

export const api = {
  /** `date` is a local `YYYY-MM-DD`; omit it for today. */
  day: (date?: string) => invoke<DaySummary>("get_day", { date }),
  firstDay: () => invoke<string | null>("get_first_day"),
  /** Days with tracked time between two local `YYYY-MM-DD` dates, inclusive. */
  dayTotals: (from: string, to: string) => invoke<DayTotal[]>("get_day_totals", { from, to }),
  patterns: (days: number) => invoke<Patterns>("get_patterns", { days }),
  categories: () => invoke<CategoryEntry[]>("get_categories"),
  setCategory: (key: string, category: Category) => invoke<void>("set_category", { key, category }),
  /** The Monday-to-Sunday week containing `date` (a local `YYYY-MM-DD`; default today). */
  week: (date?: string) => invoke<Period>("get_week", { date }),
  /** The month containing `date` (a local `YYYY-MM-DD`; default today). */
  month: (date?: string) => invoke<Period>("get_month", { date }),
  /** The year containing `date` (a local `YYYY-MM-DD`; default today). */
  year: (date?: string) => invoke<Period>("get_year", { date }),
  /** The notes for the week containing `date`. */
  weekNotes: (date?: string) => invoke<WeekNote>("get_week_notes", { date }),
  /** Both empty takes them away; returns them as kept. */
  setWeekNotes: (date: string | undefined, well: string, change: string) => invoke<WeekNote>("set_week_notes", { date, well, change }),
  /** The weeks with notes in the month or year containing `date`, by their Monday. */
  notesBetween: (date: string | undefined, span: "month" | "year") => invoke<(WeekNote & { monday: string })[]>("get_notes_between", { date, span }),
  focus: () => invoke<FocusSession | null>("get_focus"),
  startFocus: (minutes: number, label?: string, task?: string) => invoke<FocusSession>("start_focus", { minutes, label, task }),
  tasks: () => invoke<Task[]>("get_tasks"),
  /** Quick-add text: its day and priority shortcuts are read off its end. */
  addTask: (title: string) => invoke<Task[]>("add_task", { title }),
  /** As addTask, due on `day` ("2026-11-07") unless the text names its own. */
  addTaskOnDay: (title: string, day: string) => invoke<Task[]>("add_task_on_day", { title, day }),
  parseTask: (text: string) => invoke<ParsedTask>("parse_task", { text }),
  setTaskPlan: (id: string, due: string | null, priority: number, remind: number | null) =>
    invoke<Task[]>("set_task_plan", { id, due, priority, remind }),
  /** Opens Ledger, on "note:<id>" or "task:<id>" when given. */
  openLedger: (at?: string) => invoke<void>("open_ledger", { at }),
  ledgerSettings: () => invoke<LedgerSettings>("ledger_settings"),
  setLedgerSettings: (reminders: boolean, morning: string) => invoke<LedgerSettings>("set_ledger_settings", { reminders, morning }),
  /** Pinned first, then newest edited; with a query, those whose title or text has it. */
  notes: (query?: string) => invoke<Note[]>("get_notes", { query }),
  note: (id: string) => invoke<Note | null>("get_note", { id }),
  newNote: (text: string) => invoke<Note>("new_note", { text }),
  /** null when the note was deleted elsewhere since it was opened. */
  saveNote: (id: string, body: string) => invoke<Note | null>("save_note", { id, body }),
  pinNote: (id: string, pinned: boolean) => invoke<void>("pin_note", { id, pinned }),
  setNotePaper: (id: string, paper: Paper) => invoke<void>("set_note_paper", { id, paper }),
  removeNote: (id: string) => invoke<void>("remove_note", { id }),
  /** A note deleted here but waiting on its Undo, out of every window's sight; false brings it back. */
  hideNote: (id: string, waiting: boolean) => invoke<void>("hide_note", { id, waiting }),
  setTaskNote: (task: string, note: string | null) => invoke<Task[]>("set_task_note", { task, note }),
  /** How a task comes back once ticked; null for never. One made to repeat with no due day is due today. */
  setTaskRepeat: (id: string, repeat: Repeat | null) => invoke<Task[]>("set_task_repeat", { id, repeat }),
  /** Gives a task its tags and Chrono project (by name); what's kept comes back in the list. */
  setTaskLabels: (id: string, tags: string[], project: string | null) => invoke<Task[]>("set_task_labels", { id, tags, project }),
  /** Meetings from the connected calendars between two times; only the days the calendar keeps have any. */
  calendarEvents: (fromMs: number, toMs: number) => invoke<CalendarEvent[]>("get_calendar_events", { fromMs, toMs }),
  /** Replaces a task's checklist; what's kept (trimmed, at most 30) comes back in the list. */
  setTaskSteps: (id: string, steps: Step[]) => invoke<Task[]>("set_task_steps", { id, steps }),
  syncSoon: () => invoke<void>("sync_soon"),
  /** Pops a note out into its own small window, or switches the open one to it; no id pops out the list of notes. */
  popOutNote: (id?: string) => invoke<void>("pop_out_note", { id }),
  /** The note the pop-out window is on; null for the list. */
  poppedNote: () => invoke<string | null>("popped_note"),
  noteOnTop: () => invoke<boolean>("note_on_top"),
  setNoteOnTop: (on: boolean) => invoke<void>("set_note_on_top", { on }),
  /** Where Ledger was asked to open, once: "note:<id>" or "task:<id>". */
  takeLedgerOpen: () => invoke<string | null>("take_ledger_open"),
  ringSettings: () => invoke<RingSettings>("ring_settings"),
  setRingShortcut: (shortcut: string) => invoke<RingSettings>("set_ring_shortcut", { shortcut }),
  /** Arranges the ring: what's on it, by key, in order. */
  setRingItems: (items: string[]) => invoke<RingSettings>("set_ring_items", { items }),
  setRingFocusMinutes: (minutes: number) => invoke<RingSettings>("set_ring_focus_minutes", { minutes }),
  openRing: () => invoke<void>("open_ring"),
  renameTask: (id: string, title: string) => invoke<Task[]>("rename_task", { id, title }),
  setTaskDone: (id: string, done: boolean) => invoke<Task[]>("set_task_done", { id, done }),
  removeTask: (id: string) => invoke<Task[]>("remove_task", { id }),
  /** Returns the session as it stands afterwards; null once it has ended. */
  controlFocus: (action: FocusAction, minutes?: number) => invoke<FocusControl>("control_focus", { action, minutes }),
  focusOverview: () => invoke<FocusOverview>("get_focus_overview"),
  /** Puts the last session's wrap-up away. */
  dismissFocusWrapup: () => invoke<void>("dismiss_focus_wrapup"),
  startPomodoro: (minutes?: number, rest?: number, label?: string, task?: string) => invoke<void>("start_pomodoro", { minutes, rest, label, task }),
  skipPomodoroBreak: () => invoke<void>("skip_pomodoro_break"),
  pomodoroLengths: () => invoke<PomodoroLengths>("get_pomodoro_lengths"),
  setPomodoroLengths: (lengths: PomodoroLengths) => invoke<PomodoroLengths>("set_pomodoro_lengths", { lengths }),
  focusBlocking: () => invoke<{ enabled: boolean; launchers: boolean; strict: boolean }>("get_focus_blocking"),
  /** Refused while strict mode guards a running session. */
  setFocusBlocking: (which: "enabled" | "launchers" | "strict", on: boolean) => invoke<void>("set_focus_blocking", { which, on }),
  setFocusRule: (key: string, rule: FocusRule) => invoke<void>("set_focus_rule", { key, rule }),
  /** Focus sessions on one local day (`YYYY-MM-DD`; default today). */
  focusBlocks: (date?: string) => invoke<FocusBlock[]>("get_focus_blocks", { date }),
  breakMinutes: () => invoke<number>("get_break_minutes"),
  setBreakMinutes: (minutes: number) => invoke<void>("set_break_minutes", { minutes }),
  goals: (date?: string) => invoke<DayGoals>("get_goals", { date }),
  goalSettings: () => invoke<GoalSettings>("get_goal_settings"),
  setGoal: (kind: GoalName, minutes: number) => invoke<void>("set_goal", { kind, minutes }),
  projects: () => invoke<Project[]>("get_projects"),
  /** Adds a project, or with an id changes it; `keywords` are comma-separated. */
  saveProject: (id: number | null, name: string, keywords: string) => invoke<Project[]>("save_project", { id, name, keywords }),
  removeProject: (id: number) => invoke<Project[]>("remove_project", { id }),
  /** 0 takes the target away. */
  setProjectTarget: (id: number, minutes: number) => invoke<Project[]>("set_project_target", { id, minutes }),
  /** Time per project on a day. */
  projectTimes: (date?: string) => invoke<ProjectTime[]>("get_project_times", { date }),
  /** Where a project's time came from over the day, week or month holding `date`. */
  projectDetail: (id: number, date: string | undefined, span: Span) => invoke<ProjectDetail>("get_project_detail", { id, date, span }),
  calendar: () => invoke<CalendarStatus>("get_calendar"),
  /** Keeps the addresses (one per line) and reads them at once. */
  setCalendar: (urls: string) => invoke<CalendarStatus>("set_calendar", { urls }),
  /** Half an hour or more away from this computer on a day, still to be labelled. */
  away: (date?: string) => invoke<AwayStretch[]>("get_away", { date }),
  /** `kind` is "meeting", "call", "break" or "other". */
  labelAway: (startMs: number, endMs: number, kind: string) => invoke<void>("label_away", { startMs, endMs, kind }),
  dismissAway: (startMs: number) => invoke<void>("dismiss_away", { startMs }),
  focusSchedule: () => invoke<FocusSlot[]>("get_focus_schedule"),
  /** `days` has Monday in bit 0 to Sunday in bit 6; returns the schedule after. */
  addFocusSlot: (days: number, startMin: number, minutes: number, label: string | null) =>
    invoke<FocusSlot[]>("add_focus_slot", { days, startMin, minutes, label }),
  removeFocusSlot: (id: number) => invoke<FocusSlot[]>("remove_focus_slot", { id }),
  /** null gives the slot the usual blocking back; returns the schedule after. */
  setSlotBlocks: (id: number, blocks: string[] | null) => invoke<FocusSlot[]>("set_slot_blocks", { id, blocks }),
  /** A saved list the slot blocks instead of its own; null goes back to its own. */
  setSlotList: (id: number, list: string | null) => invoke<FocusSlot[]>("set_slot_list", { id, list }),
  blockLists: () => invoke<BlockLists>("get_block_lists"),
  /** Adds a list, or with `old`, changes the list of that name. Refused for a name in use. */
  saveBlockList: (old: string | null, name: string, keys: string[]) => invoke<BlockLists>("save_block_list", { old, name, keys }),
  removeBlockList: (name: string) => invoke<BlockLists>("remove_block_list", { name }),
  setDefaultBlockList: (name: string | null) => invoke<BlockLists>("set_default_block_list", { name }),
  /** The weekly goals for the week containing `date`; the streak counts weeks. */
  weekGoals: (date?: string) => invoke<DayGoals>("get_week_goals", { date }),
  setLimit: (key: string, minutes: number) => invoke<void>("set_limit", { key, minutes }),
  setLimitHard: (key: string, on: boolean) => invoke<void>("set_limit_hard", { key, on }),
  setPrivacyRule:(key: string, mode: PrivacyMode) => invoke<void>("set_privacy_rule", { key, mode }),
  applyPrivacyToPast: (key: string) => invoke<number>("apply_privacy_to_past", { key }),
  retention: () => invoke<number>("get_retention"),
  setRetention: (days: number) => invoke<number>("set_retention", { days }),
  /** For "day", `date` is a local `YYYY-MM-DD`; omit it for today. */
  deleteHistory: (range: DeleteRange, date?: string) => invoke<number>("delete_history", { range, date }),
  status: () => invoke<Status>("get_status"),
  /** Resumes tracking if it's paused; never pauses it. */
  resumeTracking: () => invoke<void>("resume_tracking"),
  headsUp: () => invoke<HeadsUp | null>("get_heads_up"),
  dismissHeadsUp: () => invoke<void>("dismiss_heads_up"),
  clipboardHistory: () => invoke<boolean>("get_clipboard_history"),
  setClipboardHistory: (enabled: boolean) => invoke<void>("set_clipboard_history", { enabled }),
  nudgesEnabled: () => invoke<boolean>("get_nudges_enabled"),
  setNudgesEnabled: (enabled: boolean) => invoke<void>("set_nudges_enabled", { enabled }),
  testNudge: () => invoke<void>("send_test_nudge"),
  /** `minute` is when it's sent, in minutes after midnight. */
  eveningSummary: () => invoke<{ enabled: boolean; minute: number }>("get_evening_summary"),
  setEveningSummary: (enabled: boolean, minute: number) => invoke<void>("set_evening_summary", { enabled, minute }),
  testEveningSummary: () => invoke<void>("send_test_evening_summary"),
  autostart: () => invoke<Autostart>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  extensionStatus: () => invoke<{ listening: boolean; lastReportMs: number | null }>("get_extension_status"),
  openExtensionFolder: () => invoke<void>("open_extension_folder"),
  /** Installed browsers the extension can go into. */
  browsers: () => invoke<{ id: string; name: string }[]>("get_browsers"),
  /** Opens that browser's extensions page; returns the extension folder's path to paste. */
  openExtensionsPage: (id: string) => invoke<string>("open_extensions_page", { id }),
  /** Where the command palette last asked this window to go, once; null when it hasn't. `date`
   *  (local `YYYY-MM-DD`) is for Today or Week. */
  takeNavigation: () => invoke<{ view: string; date: string | null; target: string | null } | null>("take_navigation"),
  /** The web palette (macOS and Linux) opening afresh: today's footer line and the theme. */
  /** `compact`: it opens as only its search bar, results dropping down as you type. */
  paletteOpen: () => invoke<{ footer: string; appearance: Appearance; compact: boolean }>("palette_open"),
  /** What a query finds; with nothing typed, the suggestions. */
  paletteQuery: (query: string, mode: string | null = null) => invoke<PaletteResults>("palette_query", { query, mode }),
  /** Icons for listed apps, as data URLs by path. */
  paletteIcons: (paths: string[]) => invoke<Record<string, string>>("palette_icons", { paths }),
  /** The card for item `at` of the results numbered `seq`; null for one without. */
  paletteCard: (seq: number, at: number) => invoke<PaletteCard | null>("palette_card", { seq, at }),
  /** Runs item `at` of the results numbered `seq`, then puts the palette away. A switch is
   *  flipped with the palette left open instead, and the results showing its new state come back. */
  paletteRun: (seq: number, at: number) => invoke<PaletteResults | null>("palette_run", { seq, at }),
  /** Runs what Ctrl+Enter (Command+Enter on a Mac) does for item `at` of the results numbered
   *  `seq`, then puts the palette away; nothing for an item without one. One that acts in place,
   *  like cancelling a timer, leaves the palette open and returns fresh results. */
  paletteRunSecond: (seq: number, at: number) => invoke<PaletteResults | null>("palette_run_second", { seq, at }),
  paletteHide: () => invoke<void>("palette_hide"),
  /** Where the panel sits in the palette's window, in points in from each edge: a Mac puts its
   *  frosted glass there. */
  paletteGlass: (room: { left: number; top: number; right: number; bottom: number }) => invoke<void>("palette_glass", room),
  paletteSettings: () => invoke<{ shortcut: string; suggestions: boolean; overFullscreen: boolean; searchFiles: boolean; look: PaletteLook }>("get_palette_settings"),
  setPaletteLook: (look: PaletteLook) => invoke<void>("set_palette_look", { look }),
  /** Refused, keeping the old shortcut, when another app holds the keys. */
  setPaletteShortcut: (shortcut: string) => invoke<void>("set_palette_shortcut", { shortcut }),
  /** Settings is recording a shortcut (or has stopped); on Windows Alt+Space then isn't taken by the window menu. */
  recordingShortcut: (on: boolean) => invoke<void>("recording_shortcut", { on }),
  setPaletteSuggestions: (on: boolean) => invoke<void>("set_palette_suggestions", { on }),
  setPaletteSearchFiles: (on: boolean) => invoke<void>("set_palette_search_files", { on }),
  setPaletteOverFullscreen: (on: boolean) => invoke<void>("set_palette_over_fullscreen", { on }),
  /** Each asks where with the system's file dialog; null when it was closed. */
  exportTime: () => invoke<number | null>("export_time"),
  backupData: () => invoke<string | null>("backup_data"),
  /** Restarts Kairo when it restores, so it only comes back false (not restored) or failing. */
  restoreData: () => invoke<boolean>("restore_data"),
  sync: () => invoke<SyncState>("get_sync"),
  /** Null when the folder dialog was closed. */
  chooseSyncFolder: () => invoke<SyncState | null>("choose_sync_folder"),
  stopSync: () => invoke<SyncState>("stop_sync"),
  /** Shows a code for another computer to type; refused while combining through a folder. */
  pairStart: () => invoke<Linking>("pair_start"),
  pairStatus: () => invoke<Linking | null>("pair_status"),
  pairCancel: () => invoke<void>("pair_cancel"),
  /** Links with another computer's code; rejects with what to tell the person. */
  pairJoin: (code: string) => invoke<{ from: string; sync: SyncState }>("pair_join", { code }),
  /** Leaves every other computer out and shows a code to link the ones still used again. */
  startOver: () => invoke<Linking>("start_over"),
  setShownComputer: (computer: string) => invoke<void>("set_shown_computer", { computer }),
  appearance: () => invoke<Appearance>("get_appearance"),
  setAppearance: (mode: Appearance) => invoke<void>("set_appearance", { mode }),
  /** PNG data URLs by executable, for the apps that have an icon to show. */
  appIcons: (exes: string[]) => invoke<Record<string, string>>("get_app_icons", { exes }),
  version: () => invoke<string>("get_version"),
  /** The result arrives as a notification, like Kairo's settings' Updates → "Check for updates". */
  checkForUpdates: () => invoke<void>("check_for_updates"),
  seedDemo: () => invoke<number>("seed_demo_data"),
  clearDemo: () => invoke<number>("clear_demo_data"),
  openDashboard: (tab?: "settings") => invoke<void>("open_dashboard", { tab }),
  /** The tab a dashboard opened for before its page was ready to hear about it, once. */
  takeDashboardTab: () => invoke<string | null>("take_dashboard_tab"),
  /** `target` is the id of a control on that view to land on. */
  openChrono: (view?: string, target?: string) => invoke<void>("open_chrono", { view, target }),
  openPalette: () => invoke<void>("open_palette"),
};
