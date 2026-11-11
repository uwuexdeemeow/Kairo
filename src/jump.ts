import { shownControl } from "./dropdown";

/// How long the section stays marked after a jump.
export const JUMP_MS = 1500;

/// Brings the setting with this element id into view and focuses it, and marks its section for a
/// moment so the eye finds it. Nothing happens for an id the page doesn't have.
export function jumpTo(id: string): void {
  const el = document.getElementById(id);
  if (!el) return;
  // A control inside a folded section (Set up focus, a "More") needs it open to be seen.
  for (let d = el.closest("details"); d; d = d.parentElement?.closest("details") ?? null) d.open = true;
  const shown = shownControl(el);
  shown.scrollIntoView({ block: "center" });
  shown.focus();
  const section = el.closest<HTMLElement>(".block");
  if (!section) return;
  section.classList.add("jumped");
  setTimeout(() => section.classList.remove("jumped"), JUMP_MS);
}
