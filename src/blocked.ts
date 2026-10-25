// The small always-on-top window shown when a focus session minimizes a blocked app.

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";

interface Blocked {
  label: string;
  key: string;
  untilMs: number;
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
  $("#blocked-detail").textContent = `Your focus session runs until ${until}, ${minutes} ${minutes === 1 ? "minute" : "minutes"} from now.`;
}

window.kairoBlocked = (b) => {
  current = b;
  paint();
};

$("#blocked-back").addEventListener("click", () => invoke("close_blocked"));
$("#blocked-allow").addEventListener("click", () => current && invoke("allow_blocked", { key: current.key }));
$("#blocked-end").addEventListener("click", async () => {
  await invoke("control_focus", { action: "stop" });
  await invoke("close_blocked");
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("close_blocked");
});

paint();
setInterval(paint, 15_000);
$("#blocked-back").focus();
