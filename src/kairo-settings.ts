import { listen } from "@tauri-apps/api/event";
import { api, type Appearance, type Linking, type PaletteLook, type SyncState } from "./api";
import { glyph } from "./glyphs";
import { mountRingEditor } from "./ring-editor";
import { vt } from "./virustotal-api";
import { $, IS_LINUX, IS_MAC, applyTheme, h } from "./ui";

// The settings that belong to Kairo as a whole rather than to Chrono: appearance, the command
// palette, starting at login, your data, combining computers and updates. Only the dashboard
// includes this markup; it calls wireKairoSettings once.

/// A shortcut row: where its keys show, its Change button, its note, and how it's saved.
type Recorder = { keys: string; button: string; note: string; saved: string; save(shortcut: string): Promise<unknown>; opens: string };

const PALETTE: Recorder = { keys: "#palette-keys", button: "#palette-record", note: "#palette-note", saved: "", save: (s) => api.setPaletteShortcut(s), opens: "the palette" };
const RING: Recorder = { keys: "#ring-keys", button: "#ring-record", note: "#ring-note", saved: "", save: (s) => api.setRingShortcut(s), opens: "the app ring" };
/// An error as the page says it: Kairo's own messages are sentences and show as they are; a
/// system's raw one says what didn't work first, its detail after.
export function plain(err: unknown, what: string): string {
  const text = String(err).trim();
  const own = /^[A-Z].*[.!?]$/s.test(text) && !/os error|error:|panicked|\{/i.test(text);
  return own ? text : `Couldn't ${what}. Try again in a moment. (${text})`;
}

/// The ring's editor, once made.
let ringEditor: ReturnType<typeof mountRingEditor> | null = null;

/// The row recording now, if one is: only one at a time.
let recording: Recorder | null = null;

/// A key as this computer's keyboard labels it; the saved shortcut keeps the names the shortcut
/// parser knows.
function keyName(key: string): string {
  if (IS_MAC) return { Ctrl: "Control", Alt: "Option", Super: "Command" }[key] ?? key;
  if (!IS_LINUX && key === "Super") return "Win";
  return key;
}

/// A saved shortcut ("Ctrl+Space") as keys, named the way this computer's keyboard labels them.
export function shortcutKeys(shortcut: string): HTMLElement[] {
  return shortcut.split("+").map((k) => h("kbd", {}, keyName(k)));
}

function showShortcut(row: Recorder, shortcut: string) {
  row.saved = shortcut;
  $(row.keys).replaceChildren(...shortcutKeys(shortcut));
}

/// The key a shortcut names, in the spelling Kairo's shortcut parser knows; modifiers on their
/// own and keys it can't use give nothing.
function shortcutKey(e: KeyboardEvent): string | null {
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
  if (/^Digit\d$/.test(e.code)) return e.code.slice(5);
  if (/^F([1-9]|1[0-2])$/.test(e.code)) return e.code;
  if (e.code === "Space") return "Space";
  return null;
}

function stopRecording(note: string) {
  const row = recording;
  if (!row) return;
  recording = null;
  void api.recordingShortcut(false).catch(() => {});
  document.removeEventListener("keydown", recordKeys, true);
  $(row.keys).classList.remove("recording");
  $(row.button).textContent = "Change";
  showShortcut(row, row.saved);
  $(row.note).textContent = note;
}

function startRecording(row: Recorder) {
  if (recording) stopRecording("Kept as it was.");
  recording = row;
  heldMods = [];
  void api.recordingShortcut(true).catch(() => {});
  $(row.keys).classList.add("recording");
  $(row.button).textContent = "Cancel";
  $(row.note).textContent = "Press the keys you want, or Esc to keep the current ones.";
  document.addEventListener("keydown", recordKeys, true);
}

/// Stops recording a shortcut, if one is being recorded, keeping the one saved; for leaving
/// Settings, so keys pressed elsewhere don't set it.
export function cancelShortcutRecording(): void {
  if (recording) stopRecording("Kept as it was.");
}

/// A switch that saves as it's flipped, then says so beside its words for a moment; one that
/// can't be saved flips back.
function saves(sel: string, save: (on: boolean) => Promise<unknown>, failed?: (err: unknown) => void) {
  const box = $<HTMLInputElement>(sel);
  box.addEventListener("change", async () => {
    const kept = await mark(box.closest("label") ?? box.parentElement!, () => save(box.checked), failed);
    if (!kept) box.checked = !box.checked;
  });
}

const fades = new WeakMap<HTMLElement, number>();

/// Saves, then says "Saved" (or "Couldn't save") beside `label`'s words for a moment. Whether it
/// was kept.
async function mark(label: HTMLElement, save: () => Promise<unknown>, failed?: (err: unknown) => void): Promise<boolean> {
  let said = label.querySelector<HTMLElement>(".saved-mark");
  if (!said) {
    said = h("span", { class: "saved-mark", role: "status" });
    label.append(said);
  }
  window.clearTimeout(fades.get(label));
  let kept = true;
  try {
    await save();
    said.classList.remove("failed");
    said.textContent = "Saved";
  } catch (err) {
    kept = false;
    said.classList.add("failed");
    said.textContent = "Couldn't save";
    failed?.(err);
  }
  const shown = said;
  fades.set(label, window.setTimeout(() => (shown.textContent = ""), 2500));
  return kept;
}

/// While recording, the next key press with a modifier becomes the shortcut.
async function recordKeys(e: KeyboardEvent) {
  const row = recording;
  if (!row) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    stopRecording("Kept as it was.");
    return;
  }
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  heldMods = mods;
  const key = shortcutKey(e);
  $(row.keys).replaceChildren(...[...mods, key ?? "…"].map((k) => h("kbd", {}, keyName(k))));
  if (!key) {
    // Only modifiers so far: wait for the rest. Any other key is one a shortcut can't use.
    if (!["Control", "Alt", "Shift", "Meta", "AltGraph", "OS"].includes(e.key)) {
      $(row.note).textContent = `${e.key || "That key"} can't be used in a shortcut. Try a letter, a number, F1 to F12 or Space, with a modifier.`;
    }
    return;
  }
  await choose(row, mods, key);
}

/// The modifiers held at the last key press while recording, for Alt+Space, which Windows turns
/// into its window menu before the page hears the Space.
let heldMods: string[] = [];

/// Windows told us Alt+Space was pressed (`menukey.rs` held its menu back and sent word): taken as
/// the Space key with Alt and whatever else was held.
async function altSpacePressed() {
  const row = recording;
  if (!row) return;
  const order = ["Ctrl", "Alt", "Shift", "Super"];
  const mods = order.filter((m) => m === "Alt" || heldMods.includes(m));
  $(row.keys).replaceChildren(...[...mods, "Space"].map((k) => h("kbd", {}, keyName(k))));
  await choose(row, mods, "Space");
}

/// Saves the keys pressed as the row's shortcut, or says what's missing.
async function choose(row: Recorder, mods: string[], key: string) {
  if (!mods.length) {
    const hold = IS_MAC ? "Control, Option, Shift or Command" : IS_LINUX ? "Ctrl, Alt, Shift or Super" : "Ctrl, Alt, Shift or the Windows key";
    $(row.note).textContent = `Hold ${hold} with it, so typing elsewhere doesn't set it off.`;
    return;
  }
  const shortcut = [...mods, key].join("+");
  try {
    await row.save(shortcut);
    row.saved = shortcut;
    stopRecording(`Saved. ${[...mods, key].map(keyName).join("+")} opens ${row.opens} from anywhere.`);
  } catch (err) {
    stopRecording(plain(err, "save that shortcut"));
  }
}

/// Runs one of the data buttons, saying how it went beside them.
async function dataAction(run: () => Promise<string | null>) {
  const out = $("#data-status");
  try {
    const said = await run();
    if (said) out.textContent = said;
  } catch (err) {
    out.textContent = plain(err, "do that");
  }
}

/// When a computer last wrote its time, the way people say it.
function ago(ms: number): string {
  const minutes = Math.round((Date.now() - ms) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 48 * 60) return `${Math.round(minutes / 60)} h ago`;
  return new Date(ms).toLocaleDateString();
}

function whereText(sync: SyncState): string {
  if (sync.mode === "folder") return `Combining through ${sync.folder}.`;
  if (sync.mode === "relay") {
    const line = "Combined through Kairo's relay, encrypted end to end: the relay can't read your time.";
    if (!sync.error) return line;
    const last = sync.syncedMs === null ? "Not synced yet." : `Last synced ${ago(sync.syncedMs)}.`;
    // The error is Kairo's own sentence: offline, or what the relay said, like being full.
    return `${line} ${last} ${sync.error}`;
  }
  if (sync.error === "unlinked") return "This computer was unlinked. Enter a new code to combine again.";
  return `Off. This computer would show up as ${sync.name}.`;
}

function renderSync(sync: SyncState) {
  $("#sync-where").textContent = whereText(sync);
  $("#sync-computers").replaceChildren(
    ...sync.computers.map((c) =>
      h("li", {}, h("strong", {}, c.name), h("span", { class: "muted" }, c.thisOne ? " (this computer)" : ` updated ${ago(c.updatedMs)}`)),
    ),
  );
  const { mode } = sync;
  $("#sync-link").hidden = mode === "folder";
  $("#sync-link").textContent = mode === "relay" ? "Link another computer" : "Link a computer";
  $("#sync-enter").hidden = mode !== "off";
  $("#sync-choose").hidden = mode === "relay";
  $("#sync-choose").textContent = mode === "folder" ? "Use another folder" : "Use a folder instead";
  $("#sync-stop").hidden = mode === "off";
  $("#sync-over").hidden = mode !== "relay";
  if (mode !== "off") $("#sync-join").hidden = true;
  if (mode !== "relay") $("#sync-confirm").hidden = true;
}

async function refreshSync() {
  renderSync(await api.sync());
}

// ---------- a code being shown ----------

let codeTimer: ReturnType<typeof setInterval> | undefined;

function hideCode() {
  clearInterval(codeTimer);
  codeTimer = undefined;
  $("#sync-code").hidden = true;
}

function timeLeft(expiresMs: number): string {
  const seconds = Math.max(0, Math.round((expiresMs - Date.now()) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")} left`;
}

/// What a finished code says, or null while it's still waiting.
function codeOutcome(linking: Linking): string | null {
  switch (linking.state) {
    case "waiting":
      return null;
    case "linked":
      return `Linked with ${linking.name}.`;
    case "expired":
      return "The code ran out. Make a new one.";
    case "mismatch":
      return "That code didn't match. Make a new one.";
    case "failed":
      return linking.message === "Cancelled." ? "" : linking.message;
  }
}

/// Shows a code, counting down, and checks once a second how it's going.
function showCode(linking: Linking) {
  hideCode();
  $("#sync-join").hidden = true;
  $("#sync-confirm").hidden = true;
  $("#sync-status").textContent = "";
  $("#sync-code-text").textContent = linking.code;
  $("#sync-code-left").textContent = timeLeft(linking.expiresMs);
  $("#sync-code").hidden = false;
  codeTimer = setInterval(async () => {
    $("#sync-code-left").textContent = timeLeft(linking.expiresMs);
    let now: Linking | null;
    try {
      now = await api.pairStatus();
    } catch {
      return;
    }
    const said = now ? codeOutcome(now) : "";
    if (said === null) return;
    hideCode();
    $("#sync-status").textContent = said;
    if (now?.state === "linked") await refreshSync();
  }, 1000);
}

async function codeAction(run: () => Promise<Linking>) {
  try {
    showCode(await run());
  } catch (err) {
    $("#sync-status").textContent = plain(err, "reach the relay");
  }
}

/// Attaches every handler once.
/// The VirusTotal key: saved as it's pasted, and tried on request.
function wireVirusTotal() {
  const key = $<HTMLInputElement>("#vt-key");
  const note = $("#vt-note");
  key.addEventListener("change", async () => {
    try {
      await vt.setKey(key.value);
      note.textContent = key.value.trim() ? "Saved. Test key checks that VirusTotal accepts it." : "Key removed.";
      key.placeholder = key.value.trim() ? "Saved. Paste a new one to change it" : "Paste your key";
      key.value = "";
    } catch (err) {
      note.textContent = plain(err, "keep the key");
    }
  });
  $("#vt-test").addEventListener("click", async () => {
    note.textContent = "Testing…";
    try {
      await vt.testKey();
      note.replaceChildren(glyph("check"), "Works. The free key allows 4 lookups a minute and 500 a day.");
    } catch (err) {
      note.textContent = plain(err, "test the key");
    }
  });
}

/// "Find a setting": sections whose words have what's typed stay, the rest and any group left
/// empty go, and a line says when nothing matches. Esc clears it.
/// What a setting can be found by: its words, and the names on its controls.
const wordsOf = (el: HTMLElement) =>
  `${el.textContent} ${[...el.querySelectorAll("[aria-label], [placeholder]")].map((c) => `${c.getAttribute("aria-label") ?? ""} ${c.getAttribute("placeholder") ?? ""}`).join(" ")}`.toLowerCase();

function wireFind() {
  const find = $<HTMLInputElement>("#settings-find");
  const filter = () => {
    const q = find.value.trim().toLowerCase();
    let any = false;
    for (const group of document.querySelectorAll<HTMLElement>(".settings-group")) {
      let shown = 0;
      for (const block of group.querySelectorAll<HTMLElement>(".block")) {
        // A list of rows (each app's settings) is matched row by row, so "reminder" shows Ledger's
        // row alone; any other section as a whole.
        const rows = [...block.querySelectorAll<HTMLElement>(".app-settings > li")];
        let match: boolean;
        if (rows.length) {
          for (const row of rows) {
            // The app's own settings that match are named under it, each a way straight to it.
            const finds = [...row.querySelectorAll<HTMLElement>("[data-finds]")];
            for (const f of finds) f.hidden = !q || !f.dataset.finds!.includes(q);
            const found = finds.some((f) => !f.hidden);
            const list = row.querySelector<HTMLElement>(".app-finds");
            if (list) list.hidden = !found;
            // The row's own words, not those of finds that didn't match.
            const own = `${row.querySelector("strong")?.textContent ?? ""} ${row.querySelector("p")?.textContent ?? row.textContent}`.toLowerCase();
            row.hidden = !!q && !found && !own.includes(q);
          }
          match = rows.some((row) => !row.hidden);
        } else match = !q || wordsOf(block).includes(q);
        block.hidden = !match;
        if (match) shown++;
      }
      group.hidden = shown === 0;
      any ||= shown > 0;
    }
    $("#settings-none").hidden = any;
  };
  find.addEventListener("input", filter);
  find.addEventListener("keydown", (e) => {
    // Enter goes to the first match: its first control, brought into view.
    if (e.key === "Enter" && find.value.trim()) {
      const first = [...document.querySelectorAll<HTMLElement>(".settings-group:not([hidden]) .block:not([hidden])")]
        .map((b) => [...b.querySelectorAll<HTMLElement>("input, select, button")].find((c) => !b.contains(c.closest("[hidden]")) && !(c as HTMLButtonElement).disabled))
        .find(Boolean);
      if (first) {
        e.preventDefault();
        first.scrollIntoView?.({ block: "center" });
        first.focus();
      }
      return;
    }
    if (e.key !== "Escape" || !find.value) return;
    // The field's own Esc: it clears, rather than also closing what's behind it.
    e.stopPropagation();
    find.value = "";
    filter();
  });
}

export function wireKairoSettings(): void {
  wireFind();
  wireVirusTotal();
  if (IS_MAC) {
    $("#appearance-system").textContent = "Match macOS";
    $("#autostart-title").textContent = "Open at login";
    $("#autostart-label").textContent = "Start Kairo quietly in the menu bar when I log in";
  }
  if (IS_LINUX) {
    $("#appearance-system").textContent = "Match the system";
    $("#autostart-title").textContent = "Start at login";
    $("#autostart-label").textContent = "Start Kairo quietly in the tray when I log in";
    // Wayland keeps keys from apps; GNOME and Plasma take the shortcut into their own settings.
    $("#palette-note").textContent =
      "Works everywhere, even with Kairo's windows closed. On Wayland, GNOME and KDE Plasma keep it with their own keyboard shortcuts; on other Wayland desktops, add a shortcut in the keyboard settings that runs kairo --palette.";
  }

  for (const row of [PALETTE, RING]) {
    $(row.button).addEventListener("click", () => {
      if (recording === row) stopRecording("Kept as it was.");
      else startRecording(row);
    });
  }

  saves("#palette-suggestions", (on) => api.setPaletteSuggestions(on));
  // Compact opens with nothing to suggest, so the switch for suggestions rests meanwhile.
  $<HTMLSelectElement>("#palette-look").addEventListener("change", async (e) => {
    const select = e.target as HTMLSelectElement;
    const look = select.value as PaletteLook;
    $<HTMLInputElement>("#palette-suggestions").disabled = look === "compact";
    await mark(select.closest("label")!, () => api.setPaletteLook(look));
  });
  saves("#palette-over-fullscreen", (on) => api.setPaletteOverFullscreen(on));
  saves("#palette-search-files", (on) => api.setPaletteSearchFiles(on));
  saves("#clipboard-history", (on) => api.setClipboardHistory(on));

  // The theme shows at once; whether it was kept is said beside it, as the switches do.
  $<HTMLSelectElement>("#appearance").addEventListener("change", async (e) => {
    const select = e.target as HTMLSelectElement;
    const mode = select.value as Appearance;
    applyTheme(mode);
    await mark(select.closest("label")!, () => api.setAppearance(mode));
  });
  // Chrono's window or the palette can change it too.
  listen<Appearance>("kairo:appearance", (e) => ($<HTMLSelectElement>("#appearance").value = e.payload));
  // The palette flipped a switch.
  listen("kairo:settings", () => loadKairoSettings());
  // Windows' Alt+Space, held back while recording: the page never hears that Space itself.
  listen("kairo:altspace", () => void altSpacePressed());

  $("#update-check").addEventListener("click", async () => {
    const out = $("#update-status");
    try {
      await api.checkForUpdates();
      out.textContent = "Checking…";
    } catch (err) {
      out.textContent = `Couldn't check: ${err}`;
    }
  });
  // The answer comes back here, as well as in a notification.
  listen<string>("kairo:update", (e) => ($("#update-status").textContent = e.payload));

  saves("#autostart-enabled", (on) => api.setAutostart(on), (err) => ($("#autostart-note").textContent = `Couldn't change it: ${err}`));

  $("#export-time").addEventListener("click", () =>
    dataAction(async () => {
      const rows = await api.exportTime();
      return rows === null ? null : `Saved ${rows} rows of tracked time.`;
    }),
  );
  $("#backup-save").addEventListener("click", () =>
    dataAction(async () => {
      const name = await api.backupData();
      return name === null ? null : `Saved ${name}.`;
    }),
  );
  $("#backup-restore").addEventListener("click", () =>
    dataAction(async () => {
      await api.restoreData();
      return null;
    }),
  );

  $("#sync-choose").addEventListener("click", async () => {
    const out = $("#sync-status");
    out.textContent = "";
    try {
      const sync = await api.chooseSyncFolder();
      if (!sync) return;
      renderSync(sync);
      out.textContent =
        sync.computers.length > 1 ? "Combined. Other computers' time now shows in Today and Week." : "Set. Choose the same folder on your other computers.";
    } catch (err) {
      out.textContent = plain(err, "use that folder");
    }
  });
  $("#sync-link").addEventListener("click", () => codeAction(api.pairStart));
  $("#sync-code-cancel").addEventListener("click", () => {
    hideCode();
    api.pairCancel();
  });
  $("#sync-enter").addEventListener("click", () => {
    // The code shown stops working too, so it can't link this computer after it joins another.
    hideCode();
    api.pairCancel();
    $("#sync-status").textContent = "";
    $("#sync-join").hidden = false;
    $<HTMLInputElement>("#sync-join-code").focus();
  });
  $("#sync-join-cancel").addEventListener("click", () => ($("#sync-join").hidden = true));
  $("#sync-join").addEventListener("submit", async (e) => {
    e.preventDefault();
    const out = $("#sync-status");
    out.textContent = "Linking…";
    try {
      const joined = await api.pairJoin($<HTMLInputElement>("#sync-join-code").value);
      $<HTMLInputElement>("#sync-join-code").value = "";
      renderSync(joined.sync);
      out.textContent = `Linked with ${joined.from}.`;
    } catch (err) {
      out.textContent = plain(err, "link with that code");
    }
  });
  // What reaches the other computers asks first, in words, with the safe answer beside it.
  let confirmed: (() => Promise<void>) | null = null;
  const ask = (question: string, yes: string, no: string, run: () => Promise<void>) => {
    hideCode();
    $("#sync-confirm-text").textContent = question;
    $("#sync-confirm-yes").textContent = yes;
    $("#sync-confirm-no").textContent = no;
    confirmed = run;
    $("#sync-confirm").hidden = false;
    $("#sync-confirm-no").focus();
  };
  $("#sync-confirm-no").addEventListener("click", () => {
    $("#sync-confirm").hidden = true;
    confirmed = null;
  });
  $("#sync-confirm-yes").addEventListener("click", async () => {
    $("#sync-confirm").hidden = true;
    const run = confirmed;
    confirmed = null;
    await run?.();
  });
  $("#sync-over").addEventListener("click", () =>
    ask(
      "Unlink every computer? Each one linked now stops combining with this one, including one that's lost or sold. Link the ones you still use again with a new code.",
      "Unlink every computer",
      "Keep them linked",
      async () => {
        await codeAction(api.startOver);
        await refreshSync();
      },
    ),
  );
  // A sync round finished: the computers and when they last synced.
  listen("kairo:sync", () => refreshSync());

  // It reaches the other computers too, so it asks first, saying so.
  $("#sync-stop").addEventListener("click", () =>
    ask(
      "Stop combining? Only this computer's time will show here, and the other computers drop this one's too.",
      "Stop combining",
      "Keep combining",
      async () => {
        const out = $("#sync-status");
        out.textContent = "";
        try {
          renderSync(await api.stopSync());
          out.textContent = "Stopped. Only this computer's time shows here now.";
        } catch (err) {
          out.textContent = `Couldn't stop: ${err}`;
        }
      },
    ),
  );
}

/// Fills the controls from what's saved.
/// Fills one section from what's saved. One that can't be read says so and its controls wait,
/// rather than showing unchecked boxes and empty keys that look like the real settings.
async function fill(anchor: string, read: () => Promise<void>): Promise<void> {
  const block = $(anchor).closest<HTMLElement>(".block");
  block?.querySelector(".load-failed")?.remove();
  try {
    await read();
    for (const c of block?.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("[data-waiting]") ?? []) {
      c.disabled = false;
      delete c.dataset.waiting;
    }
  } catch {
    if (!block) return;
    for (const c of block.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("input, select, button")) {
      if (c.disabled) continue;
      c.disabled = true;
      c.dataset.waiting = "";
    }
    block.querySelector("h2, h3")?.after(h("p", { class: "muted load-failed", role: "status" }, "Couldn't read this. Try again in a moment."));
  }
}

/// Fills the controls from what's saved, each section on its own.
export async function loadKairoSettings(): Promise<void> {
  await Promise.all([
    fill("#settings-version", async () => {
      $("#settings-version").textContent = `Kairo ${await api.version()}`;
    }),
    fill("#sync-status", async () => {
      renderSync(await api.sync());
      // A code still waiting from before the window was last open.
      const linking = await api.pairStatus();
      if (linking?.state === "waiting" && !codeTimer) showCode(linking);
    }),
    fill("#appearance", async () => {
      $<HTMLSelectElement>("#appearance").value = await api.appearance();
    }),
    fill("#palette-keys", async () => {
      const palette = await api.paletteSettings();
      showShortcut(PALETTE, palette.shortcut);
      $<HTMLInputElement>("#palette-suggestions").checked = palette.suggestions;
      $<HTMLInputElement>("#palette-suggestions").disabled = palette.look === "compact";
      $<HTMLSelectElement>("#palette-look").value = palette.look;
      $<HTMLInputElement>("#palette-over-fullscreen").checked = palette.overFullscreen;
      $<HTMLInputElement>("#palette-search-files").checked = palette.searchFiles;
      $<HTMLInputElement>("#clipboard-history").checked = await api.clipboardHistory();
    }),
    fill("#vt-key", async () => {
      // The key itself never comes back to the page: only whether one is saved.
      $<HTMLInputElement>("#vt-key").placeholder = (await vt.hasKey()) ? "Saved. Paste a new one to change it" : "Paste your key";
    }),
    fill("#ring-keys", async () => {
      const ring = await api.ringSettings();
      showShortcut(RING, ring.shortcut);
      // Made once, then updated: rebuilt on every reload, it would drop the keyboard from it.
      if (ringEditor) ringEditor.update(ring);
      else ringEditor = mountRingEditor($("#ring-items"), ring, { save: (items) => api.setRingItems(items), setFocusMinutes: (m) => api.setRingFocusMinutes(m) });
    }),
    fill("#autostart-enabled", async () => {
      const auto = await api.autostart();
      const box = $<HTMLInputElement>("#autostart-enabled");
      box.checked = auto.enabled;
      box.disabled = !auto.available;
      if (!auto.available) $("#autostart-note").textContent = "Only available in the installed app, not in development builds.";
    }),
  ]);
}
