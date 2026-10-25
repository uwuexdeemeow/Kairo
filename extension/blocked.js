// The page a blocked site turns into during a focus session.

const params = new URLSearchParams(location.search);
const site = params.get("site") ?? "This site";
const until = Number(params.get("until"));
const original = params.get("url");

function paint() {
  const minutes = Math.max(1, Math.ceil((until - Date.now()) / 60000));
  const at = new Date(until).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  document.getElementById("title").textContent = `${site} is blocked`;
  document.getElementById("detail").textContent = until
    ? `Your focus session runs until ${at}, ${minutes} ${minutes === 1 ? "minute" : "minutes"} from now.`
    : "Your focus session is still running.";
  // Once the session is over, the site is fair game again.
  if (until && Date.now() >= until && original) location.replace(original);
}

document.getElementById("close").addEventListener("click", async () => {
  const tab = await chrome.tabs.getCurrent();
  if (tab?.id !== undefined) chrome.tabs.remove(tab.id);
});

document.getElementById("allow").addEventListener("click", async () => {
  try {
    const response = await fetch("http://127.0.0.1:47613/allow", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ site: new URL(original).hostname }),
    });
    if (!response.ok) throw new Error(String(response.status));
    location.replace(original);
  } catch {
    document.getElementById("note").textContent = "Couldn't reach Kairo. Is it running?";
  }
});

paint();
setInterval(paint, 15000);
