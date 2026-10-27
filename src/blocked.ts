// The small always-on-top window shown when a focus session, or a hard daily limit, minimizes a
// blocked app.

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";

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
  $("#blocked-eyebrow").textContent = limit ? "Daily limit" : "Focus session";
  $("#blocked-detail").textContent = limit
    ? `You've passed today's limit for it, so it stays blocked until midnight.`
    : `Your focus session runs until ${until}, ${minutes} ${minutes === 1 ? "minute" : "minutes"} from now.`;
  // A limit has no session to end.
  $("#blocked-end").hidden = limit;
}

window.kairoBlocked = (b) => {
  current = b;
  paint();
};

/// Strict mode answers with a time to come back; the button counts down, then asks to confirm.
function hold(btn: HTMLButtonElement, readyAt: number) {
  const label = btn.dataset.label ?? btn.textContent ?? "";
  btn.dataset.label = label;
  window.clearInterval(Number(btn.dataset.timer));
  const tick = () => {
    const left = readyAt - Date.now();
    btn.disabled = left > 0;
    btn.textContent = left > 0 ? `Strict mode: wait ${Math.ceil(left / 1000)}s` : `Confirm: ${label.toLowerCase()}`;
    if (left <= 0) window.clearInterval(Number(btn.dataset.timer));
  };
  btn.dataset.timer = String(window.setInterval(tick, 500));
  tick();
}

const allowBtn = $("#blocked-allow") as HTMLButtonElement;
const endBtn = $("#blocked-end") as HTMLButtonElement;

$("#blocked-back").addEventListener("click", () => invoke("close_blocked"));
allowBtn.addEventListener("click", async () => {
  if (!current) return;
  const waitUntil = await invoke<number | null>("allow_blocked", { key: current.key });
  if (waitUntil) hold(allowBtn, waitUntil);
});
endBtn.addEventListener("click", async () => {
  const result = await invoke<{ waitUntil: number | null }>("control_focus", { action: "stop" });
  if (result.waitUntil) hold(endBtn, result.waitUntil);
  else await invoke("close_blocked");
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("close_blocked");
});

paint();
setInterval(paint, 15_000);
$("#blocked-back").focus();
