import { describe, expect, it } from "vitest";
import { RING, dialArcs, dialSvg, greeting, logo } from "./logos";

describe("the greeting", () => {
  it.each([
    [4, "Still up?", "Go easy."],
    [5, "Good morning,", "let's begin."],
    [11, "Good morning,", "let's begin."],
    [12, "Good afternoon,", "keep going."],
    [16, "Good afternoon,", "keep going."],
    [17, "Good evening,", "nearly done."],
    [21, "Good evening,", "nearly done."],
    [22, "Still up?", "Go easy."],
  ])("at %i", (hour, lead, aside) => {
    expect(greeting(hour)).toEqual({ lead, aside });
  });
});

describe("the dial", () => {
  it("shares the ring by part, in order, from the top", () => {
    const arcs = dialArcs({ productive: 2, neutral: 1, distracting: 1 }, 100);
    expect(arcs).toEqual([
      { part: "productive", offset: 0, length: 50 },
      { part: "neutral", offset: 50, length: 25 },
      { part: "distracting", offset: 75, length: 25 },
    ]);
  });

  it("draws only the track on an empty day, and skips empty parts", () => {
    expect(dialArcs({ productive: 0, neutral: 0, distracting: 0 }, 100)).toEqual([]);
    expect(dialArcs({ productive: 3, neutral: 0, distracting: 1 }, 100).map((a) => a.part)).toEqual(["productive", "distracting"]);
  });

  it("rounds so the arcs fill the ring exactly", () => {
    const arcs = dialArcs({ productive: 1, neutral: 1, distracting: 1 }, RING);
    const total = arcs.reduce((sum, a) => sum + a.length, 0);
    expect(total).toBeCloseTo(RING, 6);
    expect(arcs[2].offset + arcs[2].length).toBeCloseTo(RING, 6);
  });

  it("draws its centre text escaped", () => {
    const svg = dialSvg({ productive: 1, neutral: 0, distracting: 0 }, "2h 05", "<today>");
    expect(svg).toContain("2h 05");
    expect(svg).toContain("&lt;today&gt;");
    expect(svg).not.toContain("<today>");
    expect(svg).toContain('class="dial-arc productive"');
  });
});

describe("the logos", () => {
  it.each(["chrono", "ledger", "palette", "kairo"] as const)("%s draws as an svg in its own ink", (name) => {
    const svg = logo(name, 30);
    expect(svg).toMatch(/^<svg /);
    expect(svg).toContain('width="30"');
    expect(svg).toContain(`logo-${name}`);
    expect(svg).toContain('aria-hidden="true"');
  });
});
