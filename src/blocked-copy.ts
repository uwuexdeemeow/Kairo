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

/// While strict mode holds a button back: why, and for how long. `what` is the press held back.
export function holdNote(what: "allow" | "end", leftMs: number): string {
  return `Strict mode is on. You can ${what === "allow" ? "ask again" : "end the session"} in ${seconds(leftMs)}.`;
}

/// Once the wait is over: what confirming will do. `label` is the button's own words.
export function confirmNote(what: "allow" | "end", label: string): string {
  if (what === "end") return "Confirm to end the session.";
  // "Allow 5 minutes" follows "to" as it is; "5 more minutes" needs a verb.
  return /^allow\b/i.test(label) ? `Confirm to ${label.charAt(0).toLowerCase()}${label.slice(1)}.` : `Confirm to get ${label}.`;
}
