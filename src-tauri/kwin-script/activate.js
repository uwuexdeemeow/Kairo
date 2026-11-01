// Brings one window forward for the command palette; Kairo fills in which and loads it once.
const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const w of windows) {
    if (String(w.internalId) === "KAIRO_WINDOW") {
        w.minimized = false;
        if (workspace.activeWindow !== undefined)
            workspace.activeWindow = w;
        else
            workspace.activeClient = w;
    }
}
