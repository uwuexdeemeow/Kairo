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

export interface CategoryEntry {
  key: string;
  label: string;
  kind: "app" | "site";
  category: Category;
  ms: number;
}

export const api = {
  today: () => invoke<DaySummary>("get_today"),
  patterns: (days: number) => invoke<Patterns>("get_patterns", { days }),
  categories: () => invoke<CategoryEntry[]>("get_categories"),
  setCategory: (key: string, category: Category) => invoke<void>("set_category", { key, category }),
  status: () => invoke<Status>("get_status"),
  headsUp: () => invoke<HeadsUp | null>("get_heads_up"),
  dismissHeadsUp: () => invoke<void>("dismiss_heads_up"),
  nudgesEnabled: () => invoke<boolean>("get_nudges_enabled"),
  setNudgesEnabled: (enabled: boolean) => invoke<void>("set_nudges_enabled", { enabled }),
  testNudge: () => invoke<void>("send_test_nudge"),
  autostart: () => invoke<Autostart>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  seedDemo: () => invoke<number>("seed_demo_data"),
  clearDemo: () => invoke<number>("clear_demo_data"),
};
