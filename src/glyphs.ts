/// Small drawn icons for Ledger and its notes, in the 24-unit stroke the palette's and the
/// sidebar's icons use (`.icon`: no fill, round caps, the text's colour), in place of emoji and
/// symbol characters that each system draws its own way.

const PATHS = {
  bell: '<path d="M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" />',
  note: '<path d="M6 3h8l4 4v14H6z" /><path d="M14 3v4h4M9 12h6M9 16h4" />',
  play: '<path class="fill" d="M8 5.5v13l10.5-6.5z" />',
  pencil: '<path d="M4 20l1.2-4.2L15.5 5.5l3 3L8.2 18.8z" /><path d="M13.5 7.5l3 3" />',
  box: '<rect x="4" y="4" width="16" height="16" rx="3" /><path d="M8.5 12.5l2.5 2.5 4.5-5.5" />',
  close: '<path d="M7 7l10 10M17 7L7 17" />',
  list: '<path d="M5 7h14M5 12h14M5 17h14" />',
  pin: '<path d="M9 4h6l-1 5 3 3H7l3-3z" /><path d="M12 12v8" />',
  open: '<path d="M14 4h6v6M20 4l-9 9" /><path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />',
} as const;

export type Glyph = keyof typeof PATHS;

/// An icon as an element, hidden from screen readers: the control it sits in carries the words.
export function glyph(name: Glyph): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", `icon glyph-${name}`);
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = PATHS[name];
  return svg;
}
