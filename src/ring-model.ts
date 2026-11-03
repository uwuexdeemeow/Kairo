/// The app ring's model for the web ring (macOS, Linux), the twin of `src-tauri/src/ring/mod.rs`
/// that the Windows ring uses: what's on it, how it turns, what's under a point. Both have the
/// same tests, so they agree.

export const SEGMENTS = ["chrono", "ledger", "palette", "dashboard", "task", "note", "focus"] as const;
export type Segment = (typeof SEGMENTS)[number];
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

const wrap = (i: number) => ((i % SEGMENTS.length) + SEGMENTS.length) % SEGMENTS.length;

export class Ring {
  /** The segment at the top. */
  lit: number;

  constructor(last: number) {
    this.lit = Number.isInteger(last) && last >= 0 && last < SEGMENTS.length ? last : 0;
  }

  /** Turns `steps` segments: positive brings the next ones (to the right) to the top. */
  turn(steps: number): void {
    this.lit = wrap(this.lit + steps);
  }

  segment(): Segment {
    return SEGMENTS[this.lit];
  }

  /** The segment shown `offset` places from the top (negative to the left). */
  at(offset: number): Segment {
    return SEGMENTS[wrap(this.lit + offset)];
  }

  /** The segment under a point in the window, if one is drawn there. */
  hit(x: number, y: number): Segment | null {
    const r = Math.hypot(x - CENTRE.x, y - CENTRE.y);
    if (r < INNER || r > OUTER + PROUD) return null;
    const offset = Math.round(angle(x, y) / STEP);
    return Math.abs(offset) <= 2 ? this.at(offset) : null;
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
