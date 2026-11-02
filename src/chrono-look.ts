/// Chrono drawn as a watch: the day as a dial, a focus session as a ring that unwinds.
import { RING, type DayParts } from "./logos";

/// A day's active time three ways: productive, distracting, and the rest.
export function dayParts(d: { activeMs: number; productiveMs: number; distractingMs: number }): DayParts {
  const productive = Math.max(0, d.productiveMs);
  const distracting = Math.max(0, d.distractingMs);
  return { productive, distracting, neutral: Math.max(0, d.activeMs - productive - distracting) };
}

/// The focus watch: the time left as a ring from the top, shrinking clockwise as it goes. The
/// time itself is drawn over it by the page.
export function watchSvg(left: number, planned: number, paused: boolean): string {
  const share = planned > 0 ? Math.min(1, Math.max(0, left / planned)) : 1;
  const length = share * RING;
  return (
    `<svg class="watch${paused ? " paused" : ""}" viewBox="0 0 100 100" aria-hidden="true">` +
    `<circle class="watch-face" cx="50" cy="50" r="46" stroke-width="2"/>` +
    `<circle class="watch-track" cx="50" cy="50" r="36" fill="none" stroke-width="5"/>` +
    `<circle class="watch-left" cx="50" cy="50" r="36" fill="none" stroke-width="5" stroke-linecap="round" stroke-dasharray="${length.toFixed(3)} ${RING.toFixed(3)}" transform="rotate(-90 50 50)"/>` +
    `</svg>`
  );
}
