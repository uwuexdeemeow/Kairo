import { listen } from "@tauri-apps/api/event";
import { api, type Appearance } from "./api";

// ---------- helpers ----------

export const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;

/// Kairo also runs on a Mac and on Linux, where a few things go by other names.
export const IS_MAC = navigator.userAgent.includes("Macintosh");
export const IS_LINUX = !IS_MAC && navigator.userAgent.includes("Linux");

export function h(tag: string, attrs: Record<string, string> = {}, ...children: (Node | string)[]): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  el.append(...children);
  return el;
}

const SVG_NS = "http://www.w3.org/2000/svg";
export function s(tag: string, attrs: Record<string, string | number> = {}): SVGElement {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
}

export function fmtDur(ms: number): string {
  const mins = Math.round(ms / 60_000);
  if (mins < 1) return "<1m";
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}m`;
}

export function fmtClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

export const pct = (part: number, whole: number) => (whole ? Math.round((part / whole) * 100) : 0);

// Labels come from window titles, which any web page controls, so escape before using innerHTML.
export const esc = (text: string) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

// ---------- theme ----------

const THEME_KEY = "kairo.theme";
const prefersLight = window.matchMedia("(prefers-color-scheme: light)");
let appearance: Appearance = "system";

export function getAppearance(): Appearance {
  return appearance;
}

/// Puts the palette for a choice on the page. "Match Windows" asks the system, and the choice
/// is kept for the next launch so the window opens in the right colors straight away.
export function applyTheme(mode: Appearance) {
  appearance = mode;
  const theme = mode === "system" ? (prefersLight.matches ? "light" : "dark") : mode;
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // The next launch may open in the other palette for a moment; that's all.
  }
}

prefersLight.addEventListener("change", () => appearance === "system" && applyTheme("system"));

/// Paints the saved choice, then repaints whenever another window changes it.
export function followTheme() {
  api.appearance().then(applyTheme, () => {});
  listen<Appearance>("kairo:appearance", (e) => applyTheme(e.payload));
}
