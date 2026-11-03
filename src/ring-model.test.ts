import { describe, expect, it } from "vitest";
import { ALL, CENTRE, DEFAULT, INNER, MAX_ITEMS, MIN_ITEMS, OUTER, Ring, STEP, TAP_MS, angle, dragSteps, onModifierUp, segmentPath, shown } from "./ring-model";

// The same cases as the Rust model's tests, so both rings agree.

/// A point on the ring's middle line at `deg` degrees from straight up.
function onRing(deg: number): [number, number] {
  const r = (OUTER + INNER) / 2;
  const a = (deg * Math.PI) / 180;
  return [CENTRE.x + r * Math.sin(a), CENTRE.y - r * Math.cos(a)];
}

describe("the ring", () => {
  it("opens on the last one and turns both ways round", () => {
    const ring = new Ring(DEFAULT, 1);
    expect(ring.segment()).toBe("ledger");
    ring.turn(1);
    expect(ring.segment()).toBe("palette");
    ring.turn(-3);
    expect(ring.segment()).toBe("focus");
    ring.turn(8);
    expect(ring.segment()).toBe("chrono");
    expect([ring.at(-1), ring.at(1), ring.at(2)]).toEqual(["focus", "ledger", "palette"]);
    expect(new Ring(DEFAULT, 40).segment()).toBe("chrono");
  });

  it("turns any number of items round, and shows each once", () => {
    const ring = new Ring(["notes", "week", "pause"], 0);
    ring.turn(4);
    expect(ring.segment()).toBe("week");
    ring.turn(-2);
    expect(ring.segment()).toBe("pause");
    expect(shown(3)).toEqual([0, 1, -1]);
    expect(shown(4)).toEqual([0, 1, -1, 2]);
    expect(shown(10)).toEqual([0, 1, -1, 2, -2]);
    for (let n = MIN_ITEMS; n <= MAX_ITEMS; n++) {
      const r = new Ring(ALL.slice(0, n), 0);
      expect(new Set(shown(n).map((k) => r.indexAt(k))).size).toBe(Math.min(n, 5));
    }
  });

  it("knows which segment is under a point", () => {
    const ring = new Ring(DEFAULT, 0);
    const hit = (deg: number) => {
      const i = ring.hit(...onRing(deg));
      return i === null ? null : ring.items[i];
    };
    expect(hit(0)).toBe("chrono");
    expect(hit(STEP)).toBe("ledger");
    expect(hit(-STEP)).toBe("focus");
    expect(hit(STEP * 0.45)).toBe("chrono");
    // With three, nothing is drawn two places round.
    const three = new Ring(["notes", "week", "pause"], 0);
    expect(three.hit(...onRing(STEP * 2))).toBeNull();
    expect(three.hit(...onRing(-STEP))).toBe(2);
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
