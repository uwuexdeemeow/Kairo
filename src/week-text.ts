// How days and times of a week are said, for the schedules (focus and quiet hours).
import { h } from "./ui";

export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// "Weekdays", "Every day", "Weekends" or the days themselves.
export function daysText(days: number): string {
  if (days === 0x7f) return "Every day";
  if (days === 0x1f) return "Weekdays";
  if (days === 0x60) return "Weekends";
  return WEEKDAYS.filter((_, i) => days & (1 << i)).join(", ");
}

/// Minutes after midnight as "09:00"; past midnight wraps to the next day's time.
export const clockOf = (min: number) => `${String(Math.floor(min / 60) % 24).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;

/// The day toggles of an add row, Monday to Sunday, `on` of them pressed to start.
export function dayToggles(on: (i: number) => boolean): HTMLElement[] {
  return WEEKDAYS.map((day, i) => {
    const button = h("button", { class: "day-toggle", type: "button", "aria-pressed": String(on(i)), title: day }, day.slice(0, 2));
    button.addEventListener("click", () => button.setAttribute("aria-pressed", String(button.getAttribute("aria-pressed") !== "true")));
    return button;
  });
}

/// The days pressed in a row of toggles, as the bits the app keeps.
export function daysOf(row: HTMLElement): number {
  return [...row.children].reduce((bits, b, i) => (b.getAttribute("aria-pressed") === "true" ? bits | (1 << i) : bits), 0);
}
