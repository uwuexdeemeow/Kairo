/// Ledger's note editor: a plain textarea, its text transparent, over a highlighted copy of the
/// same text in the same font and wrapping. The textarea keeps native typing, undo, spell check,
/// input methods and screen readers; the copy behind it only paints. No library: Kairo stays light.
/// The font is monospaced, so bold and italic letters are as wide as plain ones and the two
/// layers stay lined up.

const MAX_CHARS = 100_000;

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const mark = (m: string) => `<span class="md-mark">${esc(m)}</span>`;

/// Code, bold, italic and links within a line.
const INLINE = /(`)([^`\n]+)(`)|(\*\*)([^*\n]+)(\*\*)|([*_])([^*_\n]+)(\7)|(\[)([^\]\n]+)(\]\([^)\s]+\))/g;

function inline(line: string): string {
  let out = "";
  let at = 0;
  for (const m of line.matchAll(INLINE)) {
    out += esc(line.slice(at, m.index));
    if (m[1]) out += `<code class="md-c">${mark(m[1])}${esc(m[2])}${mark(m[3])}</code>`;
    else if (m[4]) out += `<strong class="md-b">${mark(m[4])}${esc(m[5])}${mark(m[6])}</strong>`;
    else if (m[7]) out += `<em class="md-i">${mark(m[7])}${esc(m[8])}${mark(m[9])}</em>`;
    else out += `<span class="md-link">${mark(m[10])}${esc(m[11])}${mark(m[12])}</span>`;
    at = m.index + m[0].length;
  }
  return out + esc(line.slice(at));
}

const HEADING = /^(#{1,3} )(.*)$/;
const BOX = /^(\s*[-*] )\[([ xX])\](.*)$/;

function line(text: string): string {
  const h = HEADING.exec(text);
  if (h) return `<span class="md-h">${mark(h[1])}${inline(h[2])}</span>`;
  const b = BOX.exec(text);
  if (b) {
    const done = b[2] !== " ";
    const rest = inline(b[3]);
    return `${mark(b[1])}<span class="md-box${done ? " md-done" : ""}">[${b[2]}]</span>${done ? `<span class="md-done-text">${rest}</span>` : rest}`;
  }
  return inline(text);
}

/// The copy drawn behind the textarea. A last empty line gets a space, or the copy would be a
/// line shorter than the textarea and scroll out of step.
export function highlight(text: string): string {
  const out = text.split("\n").map(line).join("\n");
  return text.endsWith("\n") ? `${out} ` : out;
}

const ITEM = /^(\s*)([-*] \[[ xX]\] |[-*] |(\d+)\. )(.*)$/;

/// Enter on a list item: the next item's marker on a new line (a box unticked, a number one
/// more), or, on an empty item, the list's end. Null when the line isn't a list item.
export function continueList(text: string, caret: number): { text: string; caret: number } | null {
  const start = text.lastIndexOf("\n", caret - 1) + 1;
  const endAt = text.indexOf("\n", caret);
  const end = endAt === -1 ? text.length : endAt;
  const m = ITEM.exec(text.slice(start, end));
  if (!m) return null;
  const [, indent, marker, num, rest] = m;
  if (!rest.trim() && caret === end) {
    // An empty item ends the list: its marker goes, and so does the line it was on.
    const before = text.slice(0, start);
    const after = text.slice(end);
    return { text: before + after, caret: before.length };
  }
  const next = num !== undefined ? `${Number(num) + 1}. ` : marker.includes("[") ? `${marker[0]} [ ] ` : marker;
  const insert = `\n${indent}${next}`;
  return { text: text.slice(0, caret) + insert + text.slice(caret), caret: caret + insert.length };
}

/// A click at `caret` inside a checkbox's brackets ticks or unticks it; null elsewhere.
export function toggleBoxAt(text: string, caret: number): string | null {
  const start = text.lastIndexOf("\n", caret - 1) + 1;
  const endAt = text.indexOf("\n", start);
  const lineText = text.slice(start, endAt === -1 ? text.length : endAt);
  const m = BOX.exec(lineText);
  if (!m) return null;
  const open = start + m[1].length;
  if (caret < open || caret > open + 3) return null;
  const now = m[2] === " " ? "x" : " ";
  return text.slice(0, open + 1) + now + text.slice(open + 2);
}

/// `mark` around the selection, which stays on the text inside; with none, a pair of marks
/// with the caret between them.
export function wrapSelection(text: string, start: number, end: number, mark: string): { text: string; start: number; end: number } {
  const out = text.slice(0, start) + mark + text.slice(start, end) + mark + text.slice(end);
  return { text: out, start: start + mark.length, end: end + mark.length };
}

export type Editor = {
  el: HTMLElement;
  set(text: string): void;
  value(): string;
  focus(): void;
  /** Wraps the selection in `mark` (** for bold, * for italic), as Ctrl+B and Ctrl+I do. */
  wrap(mark: string): void;
  /** Starts the caret's line with an unticked box. */
  box(): void;
};

export function mountEditor(root: HTMLElement, text: string, onChange: (text: string) => void): Editor {
  const layer = document.createElement("pre");
  layer.className = "note-layer";
  layer.setAttribute("aria-hidden", "true");
  const area = document.createElement("textarea");
  area.className = "note-text";
  area.maxLength = MAX_CHARS;
  area.spellcheck = true;
  area.setAttribute("aria-label", "Note");
  const el = document.createElement("div");
  el.className = "note-editor";
  el.append(layer, area);
  root.replaceChildren(el);

  const paint = () => {
    layer.innerHTML = highlight(area.value);
    layer.scrollTop = area.scrollTop;
  };
  // Changes made here (a list's next marker, bold, a tick) go in as typed text, so Ctrl+Z undoes
  // them like typing; setting the value outright would clear the undo history. Only the part
  // that differs is replaced.
  const replace = (next: string, start: number, end = start) => {
    const old = area.value;
    let head = 0;
    while (head < old.length && head < next.length && old[head] === next[head]) head++;
    let tail = 0;
    while (tail < old.length - head && tail < next.length - head && old[old.length - 1 - tail] === next[next.length - 1 - tail]) tail++;
    area.setSelectionRange(head, old.length - tail);
    const typed = typeof document.execCommand === "function" && document.execCommand("insertText", false, next.slice(head, next.length - tail));
    if (!typed || area.value !== next) area.value = next;
    area.setSelectionRange(start, end);
    paint();
    onChange(area.value);
  };

  area.value = text;
  paint();
  area.addEventListener("input", () => {
    paint();
    onChange(area.value);
  });
  area.addEventListener("scroll", () => (layer.scrollTop = area.scrollTop));
  area.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && area.selectionStart === area.selectionEnd) {
      const next = continueList(area.value, area.selectionStart);
      if (next) {
        e.preventDefault();
        replace(next.text, next.caret);
      }
      return;
    }
    const key = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (key === "b" || key === "i")) {
      e.preventDefault();
      const w = wrapSelection(area.value, area.selectionStart, area.selectionEnd, key === "b" ? "**" : "*");
      replace(w.text, w.start, w.end);
    }
  });
  area.addEventListener("click", () => {
    if (area.selectionStart !== area.selectionEnd) return;
    const caret = area.selectionStart;
    const next = toggleBoxAt(area.value, caret);
    if (next !== null) replace(next, caret);
  });

  return {
    el,
    // A newer text from elsewhere (the other window, another computer) keeps the caret where it
    // was, as far as the new text reaches.
    set: (t) => {
      const [start, end] = [area.selectionStart, area.selectionEnd];
      area.value = t;
      area.setSelectionRange(Math.min(start, t.length), Math.min(end, t.length));
      paint();
    },
    value: () => area.value,
    focus: () => area.focus(),
    wrap: (m) => {
      const w = wrapSelection(area.value, area.selectionStart, area.selectionEnd, m);
      replace(w.text, w.start, w.end);
      area.focus();
    },
    box: () => {
      const caret = area.selectionStart;
      const start = area.value.lastIndexOf("\n", caret - 1) + 1;
      if (/^\s*[-*] \[[ xX]\] /.test(area.value.slice(start))) return area.focus();
      replace(area.value.slice(0, start) + "- [ ] " + area.value.slice(start), caret + 6);
      area.focus();
    },
  };
}
