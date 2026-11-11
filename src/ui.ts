import { listen } from "@tauri-apps/api/event";
import { api, type Appearance, type SyncState } from "./api";
import { look, wear } from "./themes";

/// Whether another computer's time is combined here, through a folder or the relay, so there's a
/// choice of whose time to show.
export function combinedWithOthers(sync: SyncState): boolean {
  return sync.mode !== "off" && sync.computers.some((c) => !c.thisOne);
}

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

/// What to show beside a failure: the reason when it reads like one, else a calm nudge. Raw
/// exceptions ("TypeError: Cannot read properties of undefined") are no help to anyone.
export function why(err: unknown): string {
  const text = String(err instanceof Error ? err.message : err).replace(/^Error:\s*/, "").trim();
  return !text || /TypeError|ReferenceError|SyntaxError|undefined|\bnull\b|Cannot read|not a function|not defined|\binvoke\b|sqlite|database|panicked|\[object/i.test(text) ? "Try again in a moment." : text;
}

/// Makes `items` one stop for the Tab key, with the arrow keys moving between them (Home and End
/// too, and up and down by a row when `columns` is given). For charts: dozens of bars, cells or
/// segments that would otherwise be dozens of Tab stops, or none.
export function rove(items: Element[], columns = 0): void {
  const usable = items.filter((el) => !(el as HTMLButtonElement).disabled);
  usable.forEach((el, i) => {
    el.setAttribute("tabindex", i === 0 ? "0" : "-1");
    el.setAttribute("data-rove", "");
    el.addEventListener("keydown", (ev) => {
      const e = ev as KeyboardEvent;
      let to = i;
      if (e.key === "ArrowRight") to = i + 1;
      else if (e.key === "ArrowLeft") to = i - 1;
      else if (e.key === "ArrowDown" && columns) to = i + columns;
      else if (e.key === "ArrowUp" && columns) to = i - columns;
      else if (e.key === "Home") to = 0;
      else if (e.key === "End") to = usable.length - 1;
      else return;
      e.preventDefault();
      e.stopPropagation();
      const next = usable[Math.max(0, Math.min(usable.length - 1, to))];
      el.setAttribute("tabindex", "-1");
      next.setAttribute("tabindex", "0");
      (next as HTMLElement).focus();
    });
  });
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
const GROUND_KEY = "kairo.ground";
const prefersLight = window.matchMedia("(prefers-color-scheme: light)");
let appearance: Appearance = "system";

/// Puts the palette for a choice on the page. "Match Windows" asks the system, and the choice
/// is kept for the next launch so the window opens in the right colors straight away.
export function applyTheme(mode: Appearance) {
  appearance = mode;
  const worn = look(mode, prefersLight.matches);
  wear(document.documentElement, worn);
  try {
    localStorage.setItem(THEME_KEY, worn.theme);
    if (worn.ground) localStorage.setItem(GROUND_KEY, worn.ground);
    else localStorage.removeItem(GROUND_KEY);
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
