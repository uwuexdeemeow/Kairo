import { describe, expect, it } from "vitest";
import blockedHtml from "../blocked.html?raw";
import dashboardHtml from "../dashboard.html?raw";
import indexHtml from "../index.html?raw";
import ledgerHtml from "../ledger.html?raw";
import paletteHtml from "../palette.html?raw";
import blockedTs from "./blocked.ts?raw";
import dashboardTs from "./dashboard.ts?raw";
import ledgerTs from "./ledger.ts?raw";
import mainTs from "./main.ts?raw";
import paletteTs from "./palette.ts?raw";

const files: Record<string, string> = {
  "index.html": indexHtml,
  "dashboard.html": dashboardHtml,
  "ledger.html": ledgerHtml,
  "palette.html": paletteHtml,
  "blocked.html": blockedHtml,
  "src/main.ts": mainTs,
  "src/dashboard.ts": dashboardTs,
  "src/ledger.ts": ledgerTs,
  "src/palette.ts": paletteTs,
  "src/blocked.ts": blockedTs,
};
const read = (p: string) => files[p];
// The stylesheet as written: Vitest would hand a CSS import over processed. Read relative to the
// project, where the tests run; Kairo carries no Node types, so the module is named at run time.
const fs = (await import(/* @vite-ignore */ ["node", "fs"].join(":"))) as { readFileSync(path: string, encoding: "utf8"): string };
const css = fs.readFileSync("src/styles.css", "utf8");

/// The declarations of the first block whose selector is exactly `selector`.
function block(selector: string): string {
  const at = css.indexOf(`${selector} {`);
  expect(at, `${selector} in styles.css`).toBeGreaterThanOrEqual(0);
  return css.slice(at, css.indexOf("}", at));
}

describe("each window says whose it is", () => {
  it.each([
    ["index.html", "chrono"],
    ["dashboard.html", "kairo"],
    ["ledger.html", "ledger"],
    ["palette.html", "palette"],
    ["blocked.html", "kairo"],
  ])("%s is %s's", (file, app) => {
    expect(read(file)).toMatch(new RegExp(`<html[^>]*data-app="${app}"`));
  });

  it.each(["chrono", "ledger", "palette", "kairo"])("%s's accent is its ink", (app) => {
    expect(block(`:root[data-app="${app}"]`)).toContain(`--app: var(--${app});`);
  });
});

describe("the way back", () => {
  it.each([
    ["index.html", "src/main.ts"],
    ["ledger.html", "src/ledger.ts"],
  ])("%s has a button back to the dashboard", (page, script) => {
    expect(read(page)).toMatch(/<button[^>]*id="back-to-kairo"[^>]*>← Kairo<\/button>/);
    expect(read(script)).toMatch(/#back-to-kairo"\)\.addEventListener\("click", \(\) => api\.openDashboard\(\)\)/);
  });
});

describe("the two papers", () => {
  const dark = { bg: "#141a24", raised: "#1b2230", ink: "#e3e7ee", rule: "#232c3d", line: "#33405a", chrono: "#7fc4e8", ledger: "#e8a86a", palette: "#b9a2e6", kairo: "#a7b0bf" };
  const light = { bg: "#f4efe6", raised: "#fffaf2", ink: "#2b2622", rule: "#e3d9c9", line: "#d3c7b3", chrono: "#2f6f8f", ledger: "#a35a1f", palette: "#6a4c93", kairo: "#7a6f63" };

  it("deep blue ink, by default", () => {
    const root = block(":root");
    for (const [name, value] of Object.entries(dark)) expect(root, name).toContain(`--${name}: ${value};`);
  });

  it("paper, in light", () => {
    const paper = block(':root[data-theme="light"]');
    for (const [name, value] of Object.entries(light)) expect(paper, name).toContain(`--${name}: ${value};`);
  });

  it("the new faces, Geist Sans gone", () => {
    const root = block(":root");
    expect(root).toContain('--sans: "Instrument Sans Variable"');
    expect(root).toContain('--display: "Fraunces Variable"');
    for (const page of ["main", "dashboard", "ledger", "palette", "blocked"]) {
      const code = read(`src/${page}.ts`);
      expect(code, page).toContain('import "./fonts"');
      expect(code, page).not.toContain("@fontsource-variable/geist\"");
    }
  });

  it("no motion for those who ask for none", () => {
    expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)/);
  });
});

describe("the grounds", () => {
  const hex = (b: string, token: string) => b.match(new RegExp(`--${token}: #([0-9a-f]{6});`))?.[1];
  const luminance = (h: string) => {
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const contrast = (a: string, b: string) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  };
  const dark = block(":root");
  const light = block(':root[data-theme="light"]');
  const grounds: [string, string, string][] = [
    ["ink", dark, dark],
    ["paper", light, light],
    ...["graphite", "walnut", "baize", "midnight"].map((g): [string, string, string] => [g, block(`:root[data-ground="${g}"]`), dark]),
    ...["linen", "sage"].map((g): [string, string, string] => [g, block(`:root[data-ground="${g}"]`), light]),
  ];

  it.each(grounds)("%s keeps its text, edges and inks readable on its page and sheets", (_, own, set) => {
    // A token may name another (`--ribbon-ink: var(--ribbon)`), the ground's own before the set's.
    const token = (t: string): string => {
      for (const b of [own, set]) {
        const found = hex(b, t) ?? b.match(new RegExp(`--${t}: var\\(--([a-z0-9-]+)\\);`))?.[1];
        if (found) return /^[0-9a-f]{6}$/.test(found) ? found : token(found);
      }
      throw new Error(`no --${t}`);
    };
    for (const paper of [token("bg"), token("raised")]) {
      expect(contrast(token("ink"), paper)).toBeGreaterThanOrEqual(7);
      for (const t of ["ink-2", "muted"]) expect(contrast(token(t), paper), t).toBeGreaterThanOrEqual(4.5);
      expect(contrast(token("edge"), paper), "edge").toBeGreaterThanOrEqual(3);
      // The same floor as Paper's, whose pewter sits at 4.28 on its page.
      for (const t of ["chrono", "ledger", "palette", "kairo", "distracting-ink", "rec-ink", "ribbon-ink"]) expect(contrast(token(t), paper), t).toBeGreaterThanOrEqual(4.2);
    }
    // Faint is small labels on the page as much as on a sheet.
    for (const paper of [token("bg"), token("raised")]) expect(contrast(token("faint"), paper), "faint").toBeGreaterThanOrEqual(4.5);
    // "Shown" is the app's ink on a 14% tint of it; on paper the ink is deepened with the text ink.
    const mix = (a: string, b: string, share: number) => [0, 2, 4].map((i) => Math.round(parseInt(a.slice(i, i + 2), 16) * share + parseInt(b.slice(i, i + 2), 16) * (1 - share)).toString(16).padStart(2, "0")).join("");
    for (const app of ["chrono", "ledger", "palette", "kairo"]) {
      const shown = set === light ? mix(token(app), token("ink"), 0.65) : token(app);
      expect(contrast(shown, mix(token(app), token("bg"), 0.14)), `${app} shown`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("deepens the shown ink on paper, where its tint would cost it contrast", () => {
    expect(css).toContain(':root[data-theme="light"] .tabs button[aria-current="page"]');
    expect(css).toContain("color: color-mix(in srgb, var(--app) 65%, var(--ink));");
  });

  it.each(Object.entries(files).filter(([f]) => f.endsWith(".html")))("%s opens in the last ground before anything loads", (_, page) => {
    expect(page).toContain('const g = localStorage.getItem("kairo.ground");');
    expect(page).toContain("if (g) document.documentElement.dataset.ground = g;");
  });
});
