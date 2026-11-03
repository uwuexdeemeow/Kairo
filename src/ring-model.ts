/// The app ring's model for the web ring (macOS, Linux) and the ring's preview in Kairo's
/// settings, the twin of `src-tauri/src/ring/mod.rs` that the Windows ring uses: what can be on
/// it, how it turns, what's under a point. Both have the same tests, so they agree.

/// Every item there is, in the order the settings list them.
export const ALL = ["chrono", "ledger", "palette", "dashboard", "task", "note", "focus", "notes", "week", "settings", "pause"] as const;
export type Segment = (typeof ALL)[number];
/// What's on the ring until it's arranged, and after Reset.
export const DEFAULT: Segment[] = ["chrono", "ledger", "palette", "dashboard", "task", "note", "focus"];
export const MIN_ITEMS = 3;
export const MAX_ITEMS = 10;

/// Where the segments drawn sit, as places from the top, in the order each comes into view: the
/// lit one, then right and left alternately. Five at most; with fewer items, each shows once.
export function shown(count: number): number[] {
  return [0, 1, -1, 2, -2].slice(0, Math.min(count, 5));
}
/// The window, in pixels.
export const WIDTH = 300;
export const HEIGHT = 210;
/// The ring is an arc of a circle whose centre is below the window.
export const CENTRE = { x: WIDTH / 2, y: 360 };
export const OUTER = 330;
export const INNER = 230;
/// How far the lit segment stands out.
export const PROUD = 12;
/// Degrees each segment takes around the ring.
export const STEP = 21;
/// A modifier let go sooner than this after opening was a tap: the ring stays open.
export const TAP_MS = 200;
/// A press that moves less than this is a click, not a drag.
export const DRAG_PX = 4;

export class Ring {
  /** What's on it, in order around it: never empty. */
  items: Segment[];
  /** The one at the top, by its place in `items`. */
  lit: number;

  constructor(items: Segment[], lit: number) {
    this.items = items.length ? items : DEFAULT;
    this.lit = Number.isInteger(lit) && lit >= 0 && lit < this.items.length ? lit : 0;
  }

  private wrap(i: number): number {
    const n = this.items.length;
    return ((i % n) + n) % n;
  }

  /** Turns `steps` segments: positive brings the next ones (to the right) to the top. */
  turn(steps: number): void {
    this.lit = this.wrap(this.lit + steps);
  }

  segment(): Segment {
    return this.items[this.lit];
  }

  /** The place in `items` of the one `offset` places from the top (negative to the left). */
  indexAt(offset: number): number {
    return this.wrap(this.lit + offset);
  }

  /** The segment shown `offset` places from the top (negative to the left). */
  at(offset: number): Segment {
    return this.items[this.indexAt(offset)];
  }

  /** The place in `items` of the segment under a point in the window, if one is drawn there. */
  hit(x: number, y: number): number | null {
    const r = Math.hypot(x - CENTRE.x, y - CENTRE.y);
    if (r < INNER || r > OUTER + PROUD) return null;
    // Math.round takes -0.5 to -0; the Rust one rounds away from zero, as does this.
    const steps = angle(x, y) / STEP;
    const offset = Math.sign(steps) * Math.round(Math.abs(steps)) + 0;
    return shown(this.items.length).includes(offset) ? this.indexAt(offset) : null;
  }
}

/// The angle of a point about the ring's centre, in degrees clockwise from straight up.
export function angle(x: number, y: number): number {
  return (Math.atan2(x - CENTRE.x, CENTRE.y - y) * 180) / Math.PI;
}

/// How many segments a drag turns the ring, snapped: pulling it leftward brings the next ones up.
export function dragSteps(from: number, to: number): number {
  // Math.round rounds halves up; the Rust one rounds them away from zero. Away from zero here too.
  const steps = (from - to) / STEP;
  // Adding zero turns a -0 into 0.
  return Math.sign(steps) * Math.round(Math.abs(steps)) + 0;
}

export function onModifierUp(elapsedMs: number): "pick" | "stay" {
  return elapsedMs < TAP_MS ? "stay" : "pick";
}

const point = (r: number, deg: number) => {
  const a = (deg * Math.PI) / 180;
  return `${(CENTRE.x + r * Math.sin(a)).toFixed(2)} ${(CENTRE.y - r * Math.cos(a)).toFixed(2)}`;
};

/// The segment `offset` places from the top as an SVG path: a band between the two circles, with
/// a hair of gap either side; the lit one reaches further out.
export function segmentPath(offset: number, proud: boolean): string {
  const gap = 0.6;
  const from = offset * STEP - STEP / 2 + gap;
  const to = offset * STEP + STEP / 2 - gap;
  const outer = OUTER + (proud ? PROUD : 0);
  return `M${point(outer, from)} A${outer} ${outer} 0 0 1 ${point(outer, to)} L${point(INNER, to)} A${INNER} ${INNER} 0 0 0 ${point(INNER, from)} Z`;
}
