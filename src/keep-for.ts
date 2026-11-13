import { confirmTwice } from "./confirm";
import { why } from "./ui";

// A "Keep … for" choice on the settings page: how long something is kept, 0 meaning forever.
// Choosing a longer time (or forever) saves at once; a shorter one deletes what is past it, so it
// waits for a confirmed click.

export interface KeepFor {
  /// Shows the saved choice, as the page loads or after a save.
  render(days: number): void;
}

export interface KeepForOptions {
  select: HTMLSelectElement;
  /// The button that confirms a shorter time, hidden until one is chosen.
  apply: HTMLButtonElement;
  status: HTMLElement;
  /// What each choice is called in "older than …", by its value in days.
  labels: Record<string, string>;
  /// What the confirm button says for a shorter time, given its label.
  applyText: (label: string) => string;
  /// What it says once armed.
  armedText: string;
  /// What the status says after a save that changed something.
  changedText: string;
  /// Saves the choice, returning how many sessions it changed.
  save: (days: number) => Promise<number>;
}

export function keepFor(o: KeepForOptions): KeepFor {
  let saved = "0";
  const render = (days: number) => {
    saved = String(days);
    o.select.value = saved;
    o.apply.hidden = true;
  };
  const store = async (days: number) => {
    try {
      const changed = await o.save(days);
      render(days);
      o.status.textContent = changed ? o.changedText : "Saved.";
    } catch (err) {
      o.status.textContent = `Couldn't save. ${why(err)}`;
    }
  };
  const disarm = confirmTwice(o.apply, () => o.armedText, () => store(Number(o.select.value)));
  o.select.addEventListener("change", async () => {
    const value = o.select.value;
    const shorter = value !== "0" && (saved === "0" || Number(value) < Number(saved));
    o.status.textContent = "";
    disarm();
    if (shorter) {
      o.apply.textContent = o.applyText(o.labels[value] ?? `${value} days`);
      o.apply.hidden = false;
      return;
    }
    await store(Number(value));
  });
  return { render };
}
