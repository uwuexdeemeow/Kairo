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
