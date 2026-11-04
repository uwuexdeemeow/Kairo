import { h } from "./ui";

/// A delete that can be taken back: what was deleted goes from view at once, and is deleted for
/// good only when the next delete comes, the view is left, or the window closes. Until then
/// "Deleted “Essay” · Undo" says so, and Undo or Ctrl+Z brings it back. Ledger's tasks and notes
/// both delete this way.

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
  /** Makes what's waiting final now. */
  finish(): void;
  /** Takes back what's waiting. */
  undo(): void;
  waiting(): Pending | null;
};

/// Whether a key press belongs to a field being typed in, which has its own Ctrl+Z.
const typing = (target: EventTarget | null) => target instanceof HTMLElement && !!target.closest("input, textarea, select, [contenteditable]");

export function mountUndo(line: HTMLElement): Undo {
  let pending: Pending | null = null;

  const clear = () => {
    const hadFocus = line.contains(document.activeElement);
    const after = pending?.focusAfter() ?? null;
    pending = null;
    line.hidden = true;
    line.replaceChildren();
    if (hadFocus) after?.focus();
  };

  const u: Undo = {
    offer: (p) => {
      u.finish();
      pending = p;
      const button = h("button", { class: "link-btn undo-btn" }, "Undo");
      button.addEventListener("click", () => u.undo());
      line.replaceChildren(`Deleted “${p.what}” · `, button);
      line.hidden = false;
      button.focus();
    },
    finish: () => {
      if (!pending) return;
      const p = pending;
      clear();
      p.finish();
    },
    undo: () => {
      if (!pending) return;
      const p = pending;
      // The undone thing takes the keyboard back itself.
      pending = null;
      line.hidden = true;
      line.replaceChildren();
      p.undo();
    },
    waiting: () => pending,
  };

  document.addEventListener("keydown", (e) => {
    if (!pending || typing(e.target) || e.altKey || e.shiftKey) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      u.undo();
    }
  });
  return u;
}
