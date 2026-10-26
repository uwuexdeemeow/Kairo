// Minimizes one window for a Kairo focus session; Kairo fills in which and loads it once.
const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const w of windows) {
    if (String(w.internalId) === "KAIRO_WINDOW")
        w.minimized = true;
}
