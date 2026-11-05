import "./fonts";
import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { useDropdowns } from "./dropdown";
import { mountPopout, type NoteChange } from "./note-popout";
import { $, followTheme } from "./ui";

// The window a Ledger note, or the list of notes, is popped out into.

followTheme();
useDropdowns();

const win = getCurrentWindow();
const pop = mountPopout($("#popout"), {
  load: (id) => api.note(id),
  search: (q) => api.notes(q),
  create: (body) => api.newNote(body),
  save: (id, body) => api.saveNote(id, body),
  setPaper: (id, paper) => api.setNotePaper(id, paper),
  hide: (id, waiting) => api.hideNote(id, waiting),
  remove: (id) => api.removeNote(id),
  setTitle: (title) => void win.setTitle(title).catch(() => {}),
  onTop: () => api.noteOnTop(),
  setOnTop: (on) => api.setNoteOnTop(on),
  openInLedger: (id) => void api.openLedger(`note:${id}`),
  syncSoon: () => void api.syncSoon(),
});

// A note, or the list of notes when none was asked for.
const go = (id: string | null) => void (id ? pop.show(id) : pop.list());
void api.poppedNote().then(go, () => go(null));
// Popping out another note, or the list, switches this window to it.
listen<string | null>("kairo:note-open", (e) => go(e.payload));
listen<NoteChange>("kairo:note", (e) => void pop.changed(e.payload));
listen("kairo:sync", () => void pop.refresh());
// Typed here is in Ledger before you can type there; closing keeps it too.
window.addEventListener("blur", () => void pop.flush());
window.addEventListener("pagehide", () => void pop.flush());
window.addEventListener("beforeunload", () => void pop.flush());
// Closing the window makes a delete waiting on its Undo final (Kairo does too, should it not hear).
window.addEventListener("pagehide", () => pop.finishDelete());
