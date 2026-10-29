import { invoke } from "@tauri-apps/api/core";

export type Category = "productive" | "neutral" | "distracting";

/** Combining computers: the synced folder while it's on, and the computers found there. */
export interface SyncState {
  folder: string | null;
  computers: { id: string; name: string; updatedMs: number; thisOne: boolean }[];
  name: string;
  /** Whose time the views show: "all", "this" or a computer's id. */
  shown: string;
}

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
}

/** Items are numbered across rows, tiles and chips, in that order. */
export interface PaletteResults {
  seq: number;
  rows: PaletteItem[];
  tiles: PaletteItem[];
  chips: PaletteItem[];
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
export type Appearance = "system" | "light" | "dark";

export interface GoalProgress {
  key: string;
  label: string;
  /** "productive" is a target to reach; the others are caps not to pass. */
  kind: "productive" | "distracting" | "limit";
  usedMs: number;
  targetMs: number;
  met: boolean;
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
  /** Last week, or last month. */
  previous: WeekTotals;
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
}

export interface FocusBlock {
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
}

export type FocusAction = "stop" | "pause" | "resume" | "extend";

export interface GoalSettings {
  productiveMin: number;
  distractingMaxMin: number;
  weekProductiveMin: number;
  weekDistractingMaxMin: number;
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

export type GoalName ="productive" | "distracting" | "week-productive" | "week-distracting";

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
  startFocus: (minutes: number, label?: string) => invoke<FocusSession>("start_focus", { minutes, label }),
  /** Returns the session as it stands afterwards; null once it has ended. */
  controlFocus: (action: FocusAction, minutes?: number) => invoke<FocusControl>("control_focus", { action, minutes }),
  focusOverview: () => invoke<FocusOverview>("get_focus_overview"),
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
  headsUp: () => invoke<HeadsUp | null>("get_heads_up"),
  dismissHeadsUp: () => invoke<void>("dismiss_heads_up"),
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
  takeNavigation: () => invoke<{ view: string; date: string | null } | null>("take_navigation"),
  /** The web palette (macOS and Linux) opening afresh: today's footer line and the theme. */
  paletteOpen: () => invoke<{ footer: string; appearance: Appearance }>("palette_open"),
  /** What a query finds; with nothing typed, the suggestions. */
  paletteQuery: (query: string) => invoke<PaletteResults>("palette_query", { query }),
  /** Icons for listed apps, as data URLs by path. */
  paletteIcons: (paths: string[]) => invoke<Record<string, string>>("palette_icons", { paths }),
  /** The card for item `at` of the results numbered `seq`; null for one without. */
  paletteCard: (seq: number, at: number) => invoke<PaletteCard | null>("palette_card", { seq, at }),
  /** Runs item `at` of the results numbered `seq`, then puts the palette away. */
  paletteRun: (seq: number, at: number) => invoke<void>("palette_run", { seq, at }),
  paletteHide: () => invoke<void>("palette_hide"),
  paletteSettings: () => invoke<{ shortcut: string; suggestions: boolean; overFullscreen: boolean }>("get_palette_settings"),
  /** Refused, keeping the old shortcut, when another app holds the keys. */
  setPaletteShortcut: (shortcut: string) => invoke<void>("set_palette_shortcut", { shortcut }),
  setPaletteSuggestions: (on: boolean) => invoke<void>("set_palette_suggestions", { on }),
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
  openChrono: (view?: string) => invoke<void>("open_chrono", { view }),
  openPalette: () => invoke<void>("open_palette"),
};
