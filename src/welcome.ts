import { api } from "./api";
import { shortcutKeys } from "./shortcut-keys";
import { $, IS_MAC } from "./ui";

// The welcome's first lines: how to open the palette, which is what most of Kairo is reached
// through, and where tasks and notes are. The rest of the welcome is Chrono's own steps.

/// Shows the palette's keys as this computer is set to have them; the usual ones when they can't be read.
export async function showWelcomeKeys(): Promise<void> {
  let shortcut = IS_MAC ? "Alt+Space" : "Ctrl+Space";
  try {
    shortcut = (await api.paletteSettings()).shortcut || shortcut;
  } catch {
    // The keys the palette starts with.
  }
  $("#welcome-keys").replaceChildren(...shortcutKeys(shortcut));
}

/// Try the palette opens it, and Ledger opens Ledger; a failure is said beside them.
export function setUpWelcome(): void {
  const say = (text: string) => ($("#welcome-palette-status").textContent = text);
  $("#welcome-palette").addEventListener("click", async () => {
    try {
      await api.openPalette();
      say("");
    } catch {
      say("Couldn't open the palette.");
    }
  });
  $("#welcome-ledger").addEventListener("click", async () => {
    try {
      await api.openLedger();
      say("");
    } catch {
      say("Couldn't open Ledger.");
    }
  });
}
