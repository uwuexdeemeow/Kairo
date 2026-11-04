import { h } from "./ui";

/// Deletes that can be taken back: what was deleted goes from view at once, and is deleted for
/// good only when the view is left or the window closes. Until then "Deleted “Essay” · Undo"
/// says so for a few seconds, and Undo or Ctrl+Z brings back the latest, then the one before,
/// and so on. The keyboard stays where it was: the line speaks for itself, and Ctrl+Z works
/// after it has faded. Ledger's tasks and notes both delete this way.

export type Pending = {
  /** What was deleted, as the line names it. */
  what: string;
  /** Deletes it for good. */
  finish(): void;
  /** Brings it back. */
  undo(): void;
  /** Where the keyboard goes if it was on Undo when the line goes. */
  focusAfter(): HTMLElement | null;
};

export type Undo = {
  offer(p: Pending): void;
  /** Makes everything waiting final now. */
  finish(): void;
  /** Takes back the latest delete. */
  undo(): void;
  /** The latest delete still waiting, if any. */
  waiting(): Pending | null;
};

/// How long the line shows after a delete; Ctrl+Z still works once it has gone.
export const SHOWN_FOR_MS = 8000;

/// Whether a key press belongs to a field being typed in, which has its own Ctrl+Z.
const typing = (target: EventTarget | null) => target instanceof HTMLElement && !!target.closest("input, textarea, select, [contenteditable]");

export function mountUndo(line: HTMLElement): Undo {
  const pending: Pending[] = [];
  let fade = 0;

  /// Hides the line, the keyboard going somewhere sensible if it was on Undo.
  const hide = (after: Pending | undefined) => {
    window.clearTimeout(fade);
    const hadFocus = line.contains(document.activeElement);
    line.hidden = true;
    line.replaceChildren();
    if (hadFocus) after?.focusAfter()?.focus();
  };

  /// Fades the line after a while, but not from under the pointer or the keyboard.
  const fadeLater = () => {
    window.clearTimeout(fade);
    fade = window.setTimeout(() => {
      if (line.contains(document.activeElement) || line.matches(":hover")) fadeLater();
      else hide(undefined);
    }, SHOWN_FOR_MS);
  };

  const show = () => {
    const latest = pending[pending.length - 1];
    if (!latest) return hide(undefined);
    const button = h("button", { class: "link-btn undo-btn" }, "Undo");
    button.addEventListener("click", () => u.undo());
    const more = pending.length > 1 ? ` and ${pending.length - 1} more` : "";
    line.replaceChildren(`Deleted “${latest.what}”${more} · `, button);
    line.hidden = false;
    fadeLater();
  };

  const u: Undo = {
    offer: (p) => {
      pending.push(p);
      show();
    },
    finish: () => {
      if (!pending.length) return;
      const all = pending.splice(0);
      hide(all[all.length - 1]);
      for (const p of all) p.finish();
    },
    undo: () => {
      const p = pending.pop();
      if (!p) return;
      // The undone thing takes the keyboard back itself.
      // What's left still waits, said again; pressed from the line, the keyboard stays on Undo.
      const onUndo = line.contains(document.activeElement);
      if (pending.length) {
        show();
        if (onUndo) line.querySelector<HTMLElement>("button")?.focus();
      } else hide(undefined);
      p.undo();
    },
    waiting: () => pending[pending.length - 1] ?? null,
  };

  document.addEventListener("keydown", (e) => {
    if (!pending.length || typing(e.target) || e.altKey || e.shiftKey) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      u.undo();
    }
  });
  return u;
}
