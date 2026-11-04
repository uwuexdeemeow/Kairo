import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "./api";
import { mountScan } from "./scan-view";
import { $, followTheme } from "./ui";
import { vt } from "./virustotal-api";

// The scan window: files, links and hashes checked against VirusTotal.

followTheme();

void (async () => {
  const scan = await mountScan($("#scan"), {
    hasKey: vt.hasKey,
    checkFile: vt.checkFile,
    check: vt.check,
    upload: vt.upload,
    follow: vt.follow,
    pickFile: vt.pickFile,
    openReport: (link) => void vt.openReport(link),
    openSettings: () => void api.openDashboard("settings"),
  });
  // What the palette asked to look up, as the window opened or since.
  const asked = await vt.take().catch(() => null);
  if (asked) scan.check(asked);
  listen<string | null>("kairo:scan", (e) => {
    if (e.payload) scan.check(e.payload);
  });
  // A key added in settings meanwhile is picked up when the window is next in front.
  window.addEventListener("focus", () => void scan.refreshKey());
  // Files dropped on the window come with their paths; each is checked by its hash.
  void getCurrentWebview().onDragDropEvent((e) => {
    const zone = document.querySelector(".scan-drop");
    if (e.payload.type === "over" || e.payload.type === "enter") zone?.classList.add("over");
    else zone?.classList.remove("over");
    if (e.payload.type === "drop") for (const path of e.payload.paths) scan.file(path);
  });
})();
