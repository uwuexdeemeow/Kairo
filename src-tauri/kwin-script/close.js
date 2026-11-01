// Asks one window to close for the command palette, as its close button would; Kairo fills in
// which and loads it once.
const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const w of windows) {
    if (String(w.internalId) === "KAIRO_WINDOW")
        w.closeWindow();
}
