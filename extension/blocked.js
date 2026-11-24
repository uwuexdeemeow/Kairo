// The page a blocked site turns into during a focus session.

const params = new URLSearchParams(location.search);
const site = params.get("site") ?? "This site";
const until = Number(params.get("until"));
const original = params.get("url");
// Outside a focus session, a distracting site asks for a short pause instead of being blocked.
const pause = params.get("reason") === "bump";

function paint() {
  const minutes = Math.max(1, Math.ceil((until - Date.now()) / 60000));
  const at = new Date(until).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  document.getElementById("title").textContent = pause ? `Still want ${site}?` : `${site} is blocked`;
  document.getElementById("eyebrow").textContent = pause ? "Kairo · a short pause" : "Kairo · focus session";
  document.getElementById("detail").textContent = pause
    ? "You’re not in a focus session. Close the tab, or wait a few seconds and open it."
    : until
    ? `Your focus session runs until ${at}, ${minutes} ${minutes === 1 ? "minute" : "minutes"} from now.`
    : "Your focus session is still running.";
  // Once the session is over, the site is fair game again.
  if (until && Date.now() >= until && original) location.replace(original);
}

document.getElementById("close").addEventListener("click", async () => {
  const tab = await chrome.tabs.getCurrent();
  if (tab?.id !== undefined) chrome.tabs.remove(tab.id);
});

const allow = document.getElementById("allow");
if (pause) allow.textContent = "Open it for 15 minutes";
const allowLabel = allow.textContent;
let holdTimer = 0;

// Kairo's strict mode answers "allow" with a time to come back: count down to it, then ask for
// the confirming click.
function hold(readyAt) {
  clearInterval(holdTimer);
  const tick = () => {
    const left = readyAt - Date.now();
    allow.disabled = left > 0;
    allow.textContent = left > 0 ? `${pause ? "Wait" : "Strict mode: wait"} ${Math.ceil(left / 1000)}s` : `Confirm: ${allowLabel.toLowerCase()}`;
    if (left <= 0) clearInterval(holdTimer);
  };
  holdTimer = setInterval(tick, 500);
  tick();
}

allow.addEventListener("click", async () => {
  try {
    const response = await fetch("http://127.0.0.1:47613/allow", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ site: new URL(original).hostname, reason: pause ? "bump" : undefined }),
    });
    if (response.status === 423) {
      hold((await response.json()).readyAt);
      return;
    }
    if (!response.ok) throw new Error(String(response.status));
    location.replace(original);
  } catch {
    document.getElementById("note").textContent = "Couldn't reach Kairo. Is it running?";
  }
});

paint();
setInterval(paint, 15000);
