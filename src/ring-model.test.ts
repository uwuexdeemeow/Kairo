import { describe, expect, it } from "vitest";
import { CENTRE, INNER, OUTER, Ring, STEP, TAP_MS, angle, dragSteps, onModifierUp, segmentPath } from "./ring-model";

// The same cases as the Rust model's tests, so both rings agree.

/// A point on the ring's middle line at `deg` degrees from straight up.
function onRing(deg: number): [number, number] {
  const r = (OUTER + INNER) / 2;
  const a = (deg * Math.PI) / 180;
  return [CENTRE.x + r * Math.sin(a), CENTRE.y - r * Math.cos(a)];
}

describe("the ring", () => {
  it("opens on the last one and turns both ways round", () => {
    const ring = new Ring(1);
    expect(ring.segment()).toBe("ledger");
    ring.turn(1);
    expect(ring.segment()).toBe("palette");
    ring.turn(-3);
    expect(ring.segment()).toBe("focus");
    ring.turn(8);
    expect(ring.segment()).toBe("chrono");
    expect([ring.at(-1), ring.at(1), ring.at(2)]).toEqual(["focus", "ledger", "palette"]);
    expect(new Ring(40).segment()).toBe("chrono");
  });

  it("knows which segment is under a point", () => {
    const ring = new Ring(0);
    expect(ring.hit(...onRing(0))).toBe("chrono");
    expect(ring.hit(...onRing(STEP))).toBe("ledger");
    expect(ring.hit(...onRing(-STEP))).toBe("focus");
    expect(ring.hit(...onRing(STEP * 0.45))).toBe("chrono");
    expect(ring.hit(CENTRE.x, CENTRE.y - INNER + 20)).toBeNull();
    expect(ring.hit(CENTRE.x, 1)).toBeNull();
  });

  it("measures angles clockwise from the top", () => {
    expect(angle(CENTRE.x, 0)).toBeCloseTo(0, 2);
    expect(angle(CENTRE.x + 100, CENTRE.y)).toBeCloseTo(90, 2);
    expect(angle(CENTRE.x - 100, CENTRE.y)).toBeCloseTo(-90, 2);
  });

  it("snaps drags to segments", () => {
    expect(dragSteps(STEP, 0)).toBe(1);
    expect(dragSteps(0, STEP * 2)).toBe(-2);
    expect(dragSteps(0, STEP * 0.4)).toBe(0);
    expect(dragSteps(0, -STEP * 0.6)).toBe(1);
  });

  it("takes a quick release for a tap", () => {
    expect(onModifierUp(150)).toBe("stay");
    expect(onModifierUp(TAP_MS)).toBe("pick");
    expect(onModifierUp(250)).toBe("pick");
  });

  it("draws each segment as a closed band, the lit one further out", () => {
    const plain = segmentPath(0, false);
    const lit = segmentPath(0, true);
    expect(plain).toMatch(/^M[\d.-]+ [\d.-]+ A/);
    expect(plain.endsWith("Z")).toBe(true);
    expect(lit).not.toBe(plain);
    expect(segmentPath(1, false)).not.toBe(plain);
  });
});
