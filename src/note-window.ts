import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { mountPopout, type NoteChange } from "./note-popout";
import { $, followTheme } from "./ui";

// The window a Ledger note is popped out into.

followTheme();

const win = getCurrentWindow();
const pop = mountPopout($("#popout"), {
  load: (id) => api.note(id),
  save: (id, body) => api.saveNote(id, body),
  setTitle: (title) => void win.setTitle(title).catch(() => {}),
  close: () => void win.close().catch(() => {}),
  onTop: () => api.noteOnTop(),
  setOnTop: (on) => api.setNoteOnTop(on),
  openInLedger: (id) => void api.openLedger(`note:${id}`),
  syncSoon: () => void api.syncSoon(),
});

void api.poppedNote().then((id) => (id ? pop.show(id) : win.close()));
// Popping out another note switches this window to it.
listen<string>("kairo:note-open", (e) => void pop.show(e.payload));
listen<NoteChange>("kairo:note", (e) => void pop.changed(e.payload));
listen("kairo:sync", () => void pop.refresh());
// Typed here is in Ledger before you can type there; closing keeps it too.
window.addEventListener("blur", () => void pop.flush());
window.addEventListener("pagehide", () => void pop.flush());
window.addEventListener("beforeunload", () => void pop.flush());
