/// Kairo's logos and Chrono's dial, drawn inline: no image files, a few hundred bytes each, and
/// in the inks of the theme they're shown in.

export type AppName = "chrono" | "ledger" | "palette" | "kairo";
export type DayParts = { productive: number; neutral: number; distracting: number };
export type Arc = { offset: number; length: number; part: keyof DayParts };

/// The dial's ring: a circle of radius 36 in a 100-unit box.
const R = 36;
export const RING = 2 * Math.PI * R;

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/// The dashboard's greeting for a local hour: a lead in bold, an aside in italic.
export function greeting(hour: number): { lead: string; aside: string } {
  if (hour >= 5 && hour < 12) return { lead: "Good morning,", aside: "let's begin." };
  if (hour >= 12 && hour < 17) return { lead: "Good afternoon,", aside: "keep going." };
  if (hour >= 17 && hour < 22) return { lead: "Good evening,", aside: "nearly done." };
  return { lead: "Still up?", aside: "Go easy." };
}

/// The day's parts as arcs around a ring of `ring` length, from the top and clockwise, in
/// order: productive, neutral, distracting. Empty parts draw nothing; the last arc ends exactly
/// where the ring does, whatever rounding came before.
export function dialArcs(parts: DayParts, ring = RING): Arc[] {
  const order: (keyof DayParts)[] = ["productive", "neutral", "distracting"];
  const total = order.reduce((sum, k) => sum + Math.max(0, parts[k]), 0);
  if (total <= 0) return [];
  const arcs: Arc[] = [];
  let at = 0;
  const shown = order.filter((k) => parts[k] > 0);
  shown.forEach((part, i) => {
    const length = i === shown.length - 1 ? ring - at : (parts[part] / total) * ring;
    arcs.push({ part, offset: at, length });
    at += length;
  });
  return arcs;
}

/// Chrono's dial: the day's time around a ring, a number in its middle and a word under it.
export function dialSvg(parts: DayParts, centre: string, sub: string): string {
  const arcs = dialArcs(parts)
    .map(
      (a) =>
        `<circle class="dial-arc ${a.part}" cx="50" cy="50" r="${R}" fill="none" stroke-width="7" stroke-linecap="butt" stroke-dasharray="${a.length.toFixed(3)} ${RING.toFixed(3)}" stroke-dashoffset="${(-a.offset).toFixed(3)}" transform="rotate(-90 50 50)"/>`,
    )
    .join("");
  return (
    `<svg class="dial" viewBox="0 0 100 100" role="img" aria-label="${esc(`${centre} ${sub}`)}">` +
    `<circle class="dial-face" cx="50" cy="50" r="44" stroke-width="3"/>` +
    `<circle class="dial-track" cx="50" cy="50" r="${R}" fill="none" stroke-width="7"/>` +
    arcs +
    `<text class="dial-centre" x="50" y="50" text-anchor="middle">${esc(centre)}</text>` +
    `<text class="dial-sub" x="50" y="63" text-anchor="middle">${esc(sub)}</text>` +
    `</svg>`
  );
}

const DRAWINGS: Record<AppName, string> = {
  // A watch dial, its ring a third full, its hands at ten past.
  chrono:
    `<circle cx="50" cy="50" r="44" style="fill:var(--raised);stroke:var(--chrono)" stroke-width="5"/>` +
    `<circle cx="50" cy="50" r="31" fill="none" style="stroke:var(--track)" stroke-width="9"/>` +
    `<circle cx="50" cy="50" r="31" fill="none" style="stroke:var(--chrono)" stroke-width="9" stroke-dasharray="70 195" transform="rotate(-90 50 50)" stroke-linecap="round"/>` +
    `<path d="M50 50V27M50 50l13 8" style="stroke:var(--ink)" stroke-width="5" stroke-linecap="round"/>`,
  // A cloth notebook, its spine darker, a gold ribbon hanging from its top.
  ledger:
    `<rect x="20" y="10" width="62" height="80" rx="6" style="fill:var(--ledger)"/>` +
    `<rect x="20" y="10" width="11" height="80" rx="3" style="fill:color-mix(in srgb,var(--ledger) 72%,#000)"/>` +
    `<path d="M64 6h10v34l-5-6-5 6z" style="fill:var(--ribbon)"/>` +
    `<path d="M40 40h30M40 52h30M40 64h20" style="stroke:color-mix(in srgb,var(--ledger) 30%,#fff)" stroke-width="4" stroke-linecap="round" opacity=".7"/>`,
  // A keycap, raised on its shadow.
  palette:
    `<rect x="12" y="30" width="76" height="52" rx="13" style="fill:color-mix(in srgb,var(--palette) 55%,#000)"/>` +
    `<rect x="12" y="22" width="76" height="52" rx="13" style="fill:var(--raised);stroke:var(--palette)" stroke-width="4"/>` +
    `<path d="M34 48h32M42 40l-8 8 8 8" style="stroke:var(--palette)" stroke-width="5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>`,
  // A small k in a ring: Kairo itself.
  kairo:
    `<circle cx="50" cy="50" r="42" style="fill:none;stroke:var(--kairo)" stroke-width="6"/>` +
    `<path d="M40 28v44M40 54l20-22M47 47l15 25" style="stroke:var(--ink)" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>`,
};

/// An app's logo as inline SVG, `size` pixels square.
export function logo(name: AppName, size = 30): string {
  return `<svg class="logo logo-${name}" width="${size}" height="${size}" viewBox="0 0 100 100" aria-hidden="true">${DRAWINGS[name]}</svg>`;
}
