import { api, type Appearance, type SyncState } from "./api";
import { $, IS_LINUX, IS_MAC, applyTheme, getAppearance, h } from "./ui";

// The settings that belong to Kairo as a whole rather than to Chrono: appearance, the command
// palette, starting at login, your data, combining computers and updates. Only the dashboard
// includes this markup; it calls wireKairoSettings once.

let savedShortcut = "";
let recording = false;

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

function showShortcut(shortcut: string) {
  savedShortcut = shortcut;
  $("#palette-keys").replaceChildren(...shortcutKeys(shortcut));
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
  recording = false;
  document.removeEventListener("keydown", recordKeys, true);
  $("#palette-keys").classList.remove("recording");
  $("#palette-record").textContent = "Change";
  showShortcut(savedShortcut);
  $("#palette-note").textContent = note;
}

/// While recording, the next key press with a modifier becomes the shortcut.
async function recordKeys(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    stopRecording("Kept as it was.");
    return;
  }
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  const key = shortcutKey(e);
  $("#palette-keys").replaceChildren(...[...mods, key ?? "…"].map((k) => h("kbd", {}, keyName(k))));
  if (!key) return;
  if (!mods.length) {
    const hold = IS_MAC ? "Control, Option, Shift or Command" : IS_LINUX ? "Ctrl, Alt, Shift or Super" : "Ctrl, Alt, Shift or the Windows key";
    $("#palette-note").textContent = `Hold ${hold} with it, so typing elsewhere doesn't set it off.`;
    return;
  }
  const shortcut = [...mods, key].join("+");
  try {
    await api.setPaletteShortcut(shortcut);
    savedShortcut = shortcut;
    stopRecording(`Saved. ${[...mods, key].map(keyName).join("+")} opens the palette from anywhere.`);
  } catch (err) {
    stopRecording(String(err));
  }
}

/// Runs one of the data buttons, saying how it went beside them.
async function dataAction(run: () => Promise<string | null>) {
  const out = $("#data-status");
  try {
    const said = await run();
    if (said) out.textContent = said;
  } catch (err) {
    out.textContent = String(err);
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

function renderSync(sync: SyncState) {
  const on = sync.folder !== null;
  $("#sync-where").textContent = on
    ? `Combining through ${sync.folder}.`
    : `Off. This computer would show up as ${sync.name}.`;
  $("#sync-computers").replaceChildren(
    ...sync.computers.map((c) =>
      h("li", {}, h("strong", {}, c.name), h("span", { class: "muted" }, c.thisOne ? " (this computer)" : ` updated ${ago(c.updatedMs)}`)),
    ),
  );
  $("#sync-choose").textContent = on ? "Use another folder" : "Choose a folder";
  $("#sync-stop").hidden = !on;
}

/// Attaches every handler once.
export function wireKairoSettings(): void {
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

  $("#palette-record").addEventListener("click", () => {
    if (recording) {
      stopRecording("Kept as it was.");
      return;
    }
    recording = true;
    $("#palette-keys").classList.add("recording");
    $("#palette-record").textContent = "Cancel";
    $("#palette-note").textContent = "Press the keys you want, or Esc to keep the current ones.";
    document.addEventListener("keydown", recordKeys, true);
  });

  $<HTMLInputElement>("#palette-suggestions").addEventListener("change", (e) => api.setPaletteSuggestions((e.target as HTMLInputElement).checked));
  $<HTMLInputElement>("#palette-over-fullscreen").addEventListener("change", (e) =>
    api.setPaletteOverFullscreen((e.target as HTMLInputElement).checked),
  );

  $<HTMLSelectElement>("#appearance").addEventListener("change", async (e) => {
    const mode = (e.target as HTMLSelectElement).value as Appearance;
    applyTheme(mode);
    await api.setAppearance(mode);
  });

  $("#update-check").addEventListener("click", async () => {
    const out = $("#update-status");
    try {
      await api.checkForUpdates();
      out.textContent = "Checking… the result shows as a notification.";
    } catch (err) {
      out.textContent = `Couldn't check: ${err}`;
    }
  });

  $<HTMLInputElement>("#autostart-enabled").addEventListener("change", async (e) => {
    const box = e.target as HTMLInputElement;
    try {
      await api.setAutostart(box.checked);
    } catch (err) {
      box.checked = !box.checked;
      $("#autostart-note").textContent = `Couldn't change it: ${err}`;
    }
  });

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
      out.textContent = String(err);
    }
  });
  $("#sync-stop").addEventListener("click", async () => {
    renderSync(await api.stopSync());
    $("#sync-status").textContent = "Stopped. Only this computer's time shows here, and the others drop it too.";
  });
}

/// Fills the controls from what's saved.
export async function loadKairoSettings(): Promise<void> {
  $("#settings-version").textContent = `Kairo ${await api.version()}`;
  renderSync(await api.sync());
  $<HTMLSelectElement>("#appearance").value = getAppearance();
  const palette = await api.paletteSettings();
  showShortcut(palette.shortcut);
  $<HTMLInputElement>("#palette-suggestions").checked = palette.suggestions;
  $<HTMLInputElement>("#palette-over-fullscreen").checked = palette.overFullscreen;
  const auto = await api.autostart();
  const box = $<HTMLInputElement>("#autostart-enabled");
  box.checked = auto.enabled;
  box.disabled = !auto.available;
  if (!auto.available) $("#autostart-note").textContent = "Only available in the installed app, not in development builds.";
}
