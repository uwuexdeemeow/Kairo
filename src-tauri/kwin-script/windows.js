// Sends Kairo the windows a person would switch to, front first, for the command palette. Kairo
// loads it each time the palette opens. Everything goes as text, since KWin would send numbers as
// fractions.
const stack = workspace.stackingOrder ? workspace.stackingOrder : (workspace.windowList ? workspace.windowList() : workspace.clientList());
const list = [];
for (let i = stack.length - 1; i >= 0; i--) {
    const w = stack[i];
    if (w.normalWindow && !w.skipTaskbar)
        list.push({id: String(w.internalId), title: String(w.caption), pid: String(w.pid)});
}
callDBus("io.github.uwuexdeemeow.Kairo", "/Kairo", "io.github.uwuexdeemeow.Kairo", "Windows", JSON.stringify(list));
