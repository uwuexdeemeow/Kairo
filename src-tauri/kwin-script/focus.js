// Tells Kairo which window is in front on KDE Plasma's Wayland session, where apps can't see
// each other's windows. Kairo loads it into KWin and listens on the session bus. Everything goes
// as text, since KWin would send numbers as fractions.
function front() {
    return workspace.activeWindow !== undefined ? workspace.activeWindow : workspace.activeClient;
}

function report(w) {
    callDBus("io.github.uwuexdeemeow.Kairo", "/Kairo", "io.github.uwuexdeemeow.Kairo", "Focused",
        w ? String(w.pid) : "0", w ? String(w.caption) : "", w ? String(w.internalId) : "", w && w.fullScreen ? "1" : "0");
}

const watched = new Set();

function activated(w) {
    report(w);
    if (w && !watched.has(String(w.internalId))) {
        watched.add(String(w.internalId));
        w.captionChanged.connect(function () { if (front() === w) report(w); });
        w.fullScreenChanged.connect(function () { if (front() === w) report(w); });
    }
}

(workspace.windowActivated || workspace.clientActivated).connect(activated);
activated(front());
