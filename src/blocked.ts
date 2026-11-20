// The small always-on-top window shown when a focus session, or a hard daily limit, minimizes a
// blocked app.

import "./fonts";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { blockedWatch } from "./chrono-look";
import { confirmNote, FAILED_NOTE, holdNote, limitLine } from "./blocked-copy";

interface Blocked {
  label: string;
  key: string;
  untilMs: number;
  /** "focus" for a focus session, "limit" for a daily limit that's been passed. */
  reason: "focus" | "limit";
}

declare global {
  interface Window {
    /// Set before this script runs, for the first app blocked.
    __KAIRO_BLOCKED__?: Blocked;
    /// Called by Kairo when another app is blocked while this window is open.
    kairoBlocked?: (b: Blocked) => void;
  }
}

const $ = (sel: string) => document.querySelector<HTMLElement>(sel)!;
let current = window.__KAIRO_BLOCKED__;

function paint() {
  if (!current) return;
  const left = Math.max(0, current.untilMs - Date.now());
  const minutes = Math.ceil(left / 60_000);
  const until = new Date(current.untilMs).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  $("#blocked-title").textContent = `${current.label} is blocked`;
  const limit = current.reason === "limit";
  document.title = limit ? "Blocked for today" : "Blocked during focus";
  // Past a limit it's more of the same day's time, not an exception to a session; while strict
  // mode holds the button back it says so instead.
  const allow = $("#blocked-allow");
  if (!allow.dataset.timer) allow.textContent = limit ? "5 more minutes" : "Allow 5 minutes";
  $("#blocked-eyebrow").textContent = limit ? "Daily limit" : "Focus session";
  $("#blocked-detail").textContent = limit
    ? limitLine()
    : `Your focus session runs until ${until}, ${minutes} ${minutes === 1 ? "minute" : "minutes"} from now.`;
  // A limit has no session to end, and no watch: it lasts the day.
  $("#blocked-end").hidden = limit;
  $("#blocked-watch").hidden = limit;
  if (!limit) {
    const watch = blockedWatch(current.untilMs, Date.now());
    $("#blocked-ring").innerHTML = watch.svg;
    $("#blocked-minutes").textContent = watch.minutes;
  }
}

window.kairoBlocked = (b) => {
  current = b;
  paint();
};

/// What a change of state says, in a line that is read out (the time left, redrawn every
/// minute, is not).
const say = (text: string) => ($("#blocked-note").textContent = text);

/// Strict mode answers with a time to come back; the line says so and counts down, then asks to
/// confirm. The button is held back with aria-disabled rather than disabled, so the keyboard stays
/// on it instead of falling to the page when it is pressed.
function hold(btn: HTMLButtonElement, what: "allow" | "end", readyAt: number) {
  const label = btn.dataset.label ?? btn.textContent ?? "";
  btn.dataset.label = label;
  window.clearInterval(Number(btn.dataset.timer));
  const note = $("#blocked-note");
  let started = false;
  const tick = () => {
    const left = readyAt - Date.now();
    if (left > 0) {
      btn.setAttribute("aria-disabled", "true");
      note.textContent = holdNote(what, left);
      // Read out once, when the wait starts; the count that follows is for the eyes.
      if (!started) {
        started = true;
        window.setTimeout(() => note.setAttribute("aria-live", "off"), 300);
      }
      return;
    }
    btn.removeAttribute("aria-disabled");
    btn.textContent = "Confirm";
    note.setAttribute("aria-live", "polite");
    say(confirmNote(what, label));
    window.clearInterval(Number(btn.dataset.timer));
  };
  btn.dataset.timer = String(window.setInterval(tick, 500));
  tick();
}

const allowBtn = $("#blocked-allow") as HTMLButtonElement;
const endBtn = $("#blocked-end") as HTMLButtonElement;

/// A press while strict mode holds the button back does nothing, and no more than that.
const held = (btn: HTMLElement) => btn.getAttribute("aria-disabled") === "true";

$("#blocked-back").addEventListener("click", () => invoke("close_blocked"));
allowBtn.addEventListener("click", async () => {
  if (!current || held(allowBtn)) return;
  try {
    const waitUntil = await invoke<number | null>("allow_blocked", { key: current.key });
    if (waitUntil) hold(allowBtn, "allow", waitUntil);
  } catch {
    say(FAILED_NOTE);
  }
});
endBtn.addEventListener("click", async () => {
  if (held(endBtn)) return;
  try {
    const result = await invoke<{ waitUntil: number | null }>("control_focus", { action: "stop" });
    if (result.waitUntil) hold(endBtn, "end", result.waitUntil);
    else await invoke("close_blocked");
  } catch {
    say(FAILED_NOTE);
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("close_blocked");
});

paint();
setInterval(paint, 15_000);
$("#blocked-back").focus();
