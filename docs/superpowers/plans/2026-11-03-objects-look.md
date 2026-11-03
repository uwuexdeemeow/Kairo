# Kairo's new look ("Objects") Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Kairo and each app its own character (type, paper/ink colours, logos, objects, small motion) without new runtime dependencies.

**Architecture:** New theme tokens and an `--app` accent in `styles.css`; one `src/fonts.ts`; inline SVG logos and pure geometry helpers in `src/logos.ts`; page-by-page restyles; the Windows palette's theme constants and embedded fonts.

**Tech Stack:** CSS, TypeScript (Vitest), Rust (Direct2D palette). New packages: `@fontsource-variable/fraunces`, `@fontsource-variable/instrument-sans` (font files only).

**Spec:** `docs/superpowers/specs/2026-11-03-objects-look-design.md`

## Global Constraints

- Colour values exactly as the spec's table; data colours unchanged.
- No image files; logos and grain inline. Motion off under `prefers-reduced-motion: reduce`.
- Greetings exactly: "Good morning, *let's begin.*" (5–12), "Good afternoon, *keep going.*" (12–17), "Good evening, *nearly done.*" (17–22), "Still up? *Go easy.*" (22–5).
- Behaviour and wording otherwise unchanged; every existing test keeps passing.

## Review Focus

- Text contrast on `--raised` and `--bg` in both themes (body ≥ 4.5:1).
- Pages that didn't import a font now: none may fall back to a system face for headings.
- The ticked-task delay must not drop a second tick made within the 280 ms.
- Windows palette with the new fonts: row heights and text baselines unchanged.
- Light/dark switching live still repaints every window (the `kairo:appearance` path).

---

### Task 1: Tokens, fonts and the app accent
**Files:** `package.json` (two font packages), `src/fonts.ts` (new), every page entry (`main.ts`, `dashboard.ts`, `ledger.ts`, `palette.ts`, `blocked.ts`), `src/styles.css` (token blocks, `--app`, `--sans`, `--display`, grain, sheet/label/title rules, motion tokens), `index.html`/`dashboard.html`/`ledger.html`/`palette.html`/`blocked.html` (`data-app` on `<html>`).
- [ ] Vitest `theme.test.ts`: each page's HTML sets the right `data-app`; `styles.css` defines `--app` for each `data-app` value and every spec token in both theme blocks.
- [ ] Fail → implement → pass → commit.

### Task 2: Logos and dial geometry
**Files:** `src/logos.ts` (+ test): `logo(name, size)`, `dialArcs(parts: {productive, neutral, distracting}) -> {offset,length,colour}[]`, `dialSvg(parts, centre, sub)`, `greeting(hour) -> {lead, aside}`.
- [ ] Vitest: greetings at 4, 5, 11, 12, 16, 17, 21, 22; arcs sum to the ring, empty day draws only the track; `dialSvg` escapes its text.
- [ ] Fail → implement → pass → commit.

### Task 3: Dashboard
**Files:** `dashboard.html`, `src/dashboard.ts` (+ test), styles.
- [ ] Vitest: greeting shown; three object buttons (dial with today's shares and "2h 05", notebook, keycap with the shortcut keys); each opens its app; lines still update.
- [ ] Fail → implement → pass → commit.

### Task 4: Chrono
**Files:** `index.html`, `src/main.ts`, styles.
- [ ] Vitest (existing `main` tests or a new small one): the Today hero gets a day dial from the day's totals; the Focus timer face draws a ring whose dash length follows time left.
- [ ] Fail → implement → pass → commit.

### Task 5: Ledger
**Files:** `ledger.html`, `src/ledger-tasks.ts`, `src/ledger-notes.ts` (+ tests), styles.
- [ ] Vitest: ticking adds `.inking` and calls `done` after 280 ms (at once with reduced motion); a second tick within it isn't lost; overdue rows have `.ribbon`; empty states include the notebook drawing.
- [ ] Fail → implement → pass → commit.

### Task 6: Palette (web) and blocked window
**Files:** `palette.html`, `src/palette.ts`, `blocked.html`, `src/blocked.ts`, styles.
- [ ] Vitest: blocked window draws the watch face with the minutes left; palette root has `data-app="palette"`.
- [ ] Fail → implement → pass → commit.

### Task 7: Windows palette
**Files:** `src-tauri/src/palette/native.rs`.
- [ ] Rust tests: embedded fonts start with `wOF2`; `DARK`/`LIGHT` ink-on-bg and ink-on-raised contrast ≥ 4.5.
- [ ] Fail → implement (themes, accent, fonts) → pass → commit.

### Task 8: Look, checklist, release notes
- [ ] Ask, then launch and screenshot each window in light and dark; fix what's off.
- [ ] `docs/device-checklist.md` "New look" section; commit.
