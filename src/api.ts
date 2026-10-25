import { invoke } from "@tauri-apps/api/core";

export type Category = "productive" | "neutral" | "distracting";

export interface Usage {
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
  hourly: HourStat[];
  heatmap: Cell[][];
  insights: Insight[];
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
  ms: number;
}

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

export interface Week {
  /** Monday, as a local `YYYY-MM-DD`. */
  start: string;
  totals: WeekTotals;
  previous: WeekTotals;
  days: { date: string; totals: WeekTotals; goalsMet: boolean | null }[];
  bestDay: string | null;
  goalDaysMet: number;
  goalDays: number;
  topApps: { label: string; category: Category; ms: number; previousMs: number }[];
  limitsOver: { label: string; overMs: number; daysOver: number }[];
}

export interface GoalSettings {
  productiveMin: number;
  distractingMaxMin: number;
}

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
  week: (date?: string) => invoke<Week>("get_week", { date }),
  goals: (date?: string) => invoke<DayGoals>("get_goals", { date }),
  goalSettings: () => invoke<GoalSettings>("get_goal_settings"),
  setGoal: (kind: "productive" | "distracting", minutes: number) => invoke<void>("set_goal", { kind, minutes }),
  setLimit: (key: string, minutes: number) => invoke<void>("set_limit", { key, minutes }),
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
  autostart: () => invoke<Autostart>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  version: () => invoke<string>("get_version"),
  /** The result arrives as a notification, like the tray's "Check for updates". */
  checkForUpdates: () => invoke<void>("check_for_updates"),
  seedDemo: () => invoke<number>("seed_demo_data"),
  clearDemo: () => invoke<number>("clear_demo_data"),
};
