// Reports the domain of the tab in front to Kairo, on every change and every 30 seconds. Only
// the hostname leaves this script (never the path, query or page title), and only to Kairo's
// listener on this computer. If Kairo isn't running, the report is simply dropped.
//
// During a focus session Kairo answers with whether the site is blocked; if it is, the tab
// shows the extension's blocked page instead, which can go back once the site is allowed.

const KAIRO = "http://127.0.0.1:47613/tab";

function showBlocked(tab, verdict) {
  const page = new URL(chrome.runtime.getURL("blocked.html"));
  page.searchParams.set("site", verdict.site);
  page.searchParams.set("until", String(verdict.untilMs));
  page.searchParams.set("url", tab.url);
  chrome.tabs.update(tab.id, { url: page.href });
}

async function report() {
  let tab;
  try {
    [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  } catch {
    return;
  }
  let host = null;
  try {
    const url = new URL(tab?.url ?? "");
    if (url.protocol === "http:" || url.protocol === "https:") host = url.hostname;
  } catch {
    // Not a web page: a new tab, browser settings, a PDF viewer.
  }
  try {
    const response = await fetch(KAIRO, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ host, incognito: Boolean(tab?.incognito) }),
    });
    // Older versions of Kairo answer with no body.
    const verdict = response.status === 200 ? await response.json() : null;
    if (verdict?.block && host && tab?.id !== undefined) showBlocked(tab, verdict);
  } catch {
    // Kairo isn't running.
  }
}

chrome.tabs.onActivated.addListener(report);
chrome.tabs.onUpdated.addListener((_id, change, tab) => {
  if (tab.active && change.url) report();
});
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (windowId !== chrome.windows.WINDOW_ID_NONE) report();
});
chrome.runtime.onStartup.addListener(report);
chrome.runtime.onInstalled.addListener(report);

// A heartbeat, so Kairo knows the last report still holds while you stay on one page.
chrome.alarms.create("heartbeat", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(report);
