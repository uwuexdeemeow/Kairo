import { api, type PauseWindow } from "./api";
import { confirmTwice } from "./confirm";
import { $, fmtDur, h, why } from "./ui";
import { clockOf, dayToggles, daysOf, daysText } from "./week-text";

// Chrono's settings, Quiet hours: times each week when tracking pauses by itself.

/// The lengths to choose from, in minutes: a night is long.
export const QUIET_LENGTHS = [60, 120, 240, 360, 480, 600, 720, 840];

/// "22:00 to 07:00", the end on the next day's clock when it passes midnight.
export const spanText = (w: PauseWindow) => `${clockOf(w.startMin)}–${clockOf(w.startMin + w.minutes)}`;

export function renderQuietHours(windows: PauseWindow[]): void {
  const status = $("#quiet-status");
  $("#quiet-list").replaceChildren(
    ...windows.map((w) => {
      const remove = h("button", { class: "link-btn", type: "button", "aria-label": `Remove ${daysText(w.days)} ${spanText(w)}` }, "Remove") as HTMLButtonElement;
      confirmTwice(remove, () => "Press again to remove", async () => {
        try {
          renderQuietHours(await api.removePauseWindow(w.id));
          status.textContent = "";
        } catch (err) {
          status.textContent = why(err);
        }
      });
      return h("div", { class: "history-row schedule-row" }, h("span", { class: "history-time" }, spanText(w)), h("span", { class: "history-label" }, daysText(w.days)), h("span", { class: "history-len muted" }, fmtDur(w.minutes * 60_000)), remove);
    }),
  );
}

/// The add row: day toggles (every day to start), a start and a length.
export function setUpQuietForm(): void {
  const row = $("#quiet-days");
  row.replaceChildren(...dayToggles(() => true));
  const length = $<HTMLSelectElement>("#quiet-length");
  length.replaceChildren(...QUIET_LENGTHS.map((m) => h("option", { value: String(m) }, fmtDur(m * 60_000))));
  length.value = "480";
  $("#quiet-add").addEventListener("click", async () => {
    const [h24, m] = $<HTMLInputElement>("#quiet-start").value.split(":").map(Number);
    try {
      renderQuietHours(await api.addPauseWindow(daysOf(row), (h24 || 0) * 60 + (m || 0), Number(length.value)));
      $("#quiet-status").textContent = "";
    } catch (err) {
      $("#quiet-status").textContent = why(err);
    }
  });
}

export async function loadQuietHours(): Promise<void> {
  renderQuietHours(await api.pauseSchedule());
}
