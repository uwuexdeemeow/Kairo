// The words of the blocked window that change with state, kept apart from the page so they can
// be tested: what strict mode says while it holds a button back, what asks to confirm, and how
// a failure reads.

/// What a failed press says: the same plain line wherever it happens.
export const FAILED_NOTE = "Couldn’t do that. Try again.";

/// A daily limit that has been passed, as the person's own.
export const limitLine = (): string => "You’ve reached the limit you set for it today, so it stays blocked until midnight.";

const seconds = (ms: number) => {
  const n = Math.max(1, Math.ceil(ms / 1000));
  return `${n} ${n === 1 ? "second" : "seconds"}`;
};

/// While a button is held back: why, and for how long. `what` is the press held back. Strict mode
/// holds it; the short pause before a distracting app (`pause`) only asks you to wait.
export function holdNote(what: "allow" | "end", leftMs: number, pause = false): string {
  if (pause) return `Wait ${seconds(leftMs)}, then confirm if you still want it.`;
  return `Strict mode is on. You can ${what === "allow" ? "ask again" : "end the session"} in ${seconds(leftMs)}.`;
}

/// The window's words for the pause before a distracting app, outside a focus session.
export const PAUSE = {
  eyebrow: "A short pause",
  title: (label: string) => `Still want ${label}?`,
  detail: "You’re not in a focus session. Back to work, or wait a few seconds and open it.",
  allow: "Open it for 15 minutes",
};

/// Once the wait is over: what confirming will do. `label` is the button's own words.
export function confirmNote(what: "allow" | "end", label: string): string {
  if (what === "end") return "Confirm to end the session.";
  // "Open it for 15 minutes" is a sentence of its own.
  if (/^open\b/i.test(label)) return `Confirm to ${label.charAt(0).toLowerCase()}${label.slice(1)}.`;
  // "Allow 5 minutes" follows "to" as it is; "5 more minutes" needs a verb.
  return /^allow\b/i.test(label) ? `Confirm to ${label.charAt(0).toLowerCase()}${label.slice(1)}.` : `Confirm to get ${label}.`;
}
