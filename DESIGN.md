---
name: Kairo
description: A desktop hub of small tools, each drawn as the object it is, in ink on paper.
colors:
  ink-deep: "#141a24"
  ink-sheet: "#1b2230"
  ink-text: "#e3e7ee"
  ink-text-2: "#c4cbd6"
  ink-muted: "#97a1b2"
  ink-faint: "#818c9e"
  ink-rule: "#232c3d"
  ink-line: "#33405a"
  paper: "#f4efe6"
  paper-sheet: "#fffaf2"
  paper-text: "#2b2622"
  paper-text-2: "#4a423b"
  paper-muted: "#5e544a"
  paper-faint: "#746a60"
  paper-rule: "#e3d9c9"
  paper-line: "#d3c7b3"
  chrono-dark: "#7fc4e8"
  chrono-light: "#2f6f8f"
  ledger-dark: "#e8a86a"
  ledger-light: "#a35a1f"
  palette-dark: "#b9a2e6"
  palette-light: "#6a4c93"
  kairo-dark: "#a7b0bf"
  kairo-light: "#7a6f63"
  ribbon-gold: "#d9b23f"
  productive-dark: "#3987e5"
  productive-light: "#2f6fd0"
  distracting-dark: "#d95926"
  distracting-light: "#cf5220"
  distracting-ink-dark: "#e17a51"
  distracting-ink-light: "#ac441b"
  good-dark: "#7fd18b"
  good-light: "#267a37"
  rec-ink-dark: "#e96569"
  rec-ink-light: "#c03237"
  ribbon-ink-light: "#7f6212"
  edge-dark: "#657086"
  edge-light: "#8d8274"
typography:
  display:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "42px"
    fontWeight: 800
    lineHeight: 1.08
    letterSpacing: "-0.02em"
  hero:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "84px"
    fontWeight: 800
    lineHeight: 1
  figure:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "64px"
    fontWeight: 800
    lineHeight: 1
    letterSpacing: "-0.03em"
    fontFeature: "tnum"
  figure-hours:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "48px"
    fontWeight: 800
    lineHeight: 1
  headline:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "22px"
    fontWeight: 800
  title:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "16px"
    fontWeight: 800
  total:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "40px"
    fontWeight: 300
  data-lg:
    fontFamily: "Geist Mono Variable, ui-monospace, monospace"
    fontSize: "30px"
  data:
    fontFamily: "Geist Mono Variable, ui-monospace, monospace"
    fontSize: "18px"
  input:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "17px"
  body:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  small:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
  label:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.12em"
  caption:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "10px"
    fontWeight: 600
    letterSpacing: "0.14em"
  mono:
    fontFamily: "Geist Mono Variable, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
  display-narrow:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "34px"
    fontWeight: 800
  watch:
    fontFamily: "Fraunces Variable, Georgia, serif"
    fontSize: "44px"
    fontWeight: 800
  stat:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 600
  dial-label:
    fontFamily: "Instrument Sans Variable, system-ui, sans-serif"
    fontSize: "8px"
    fontWeight: 600
rounded:
  hair: "2px"
  hair-2: "3px"
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "10px"
  xl: "12px"
  panel: "14px"
  object: "16px"
  pill: "999px"
spacing:
  xs: "6px"
  sm: "8px"
  md: "14px"
  lg: "28px"
  xl: "40px"
components:
  sheet:
    backgroundColor: "{colors.ink-sheet}"
    rounded: "{rounded.lg}"
    padding: "18px"
  button:
    backgroundColor: "{colors.ink-sheet}"
    textColor: "{colors.ink-text}"
    rounded: "{rounded.lg}"
    padding: "0 16px"
    height: "36px"
  button-primary:
    backgroundColor: "{colors.chrono-dark}"
    textColor: "{colors.ink-deep}"
    rounded: "{rounded.lg}"
    padding: "0 16px"
    height: "36px"
  field:
    backgroundColor: "{colors.ink-sheet}"
    textColor: "{colors.ink-text}"
    rounded: "{rounded.lg}"
    padding: "8px 12px"
    height: "38px"
  chip:
    textColor: "{colors.ink-muted}"
    rounded: "{rounded.pill}"
    padding: "1px 8px"
  palette-bubble:
    textColor: "{colors.palette-dark}"
    rounded: "{rounded.pill}"
    padding: "3px 10px 3px 7px"
  calendar-day:
    textColor: "{colors.ink-text-2}"
    rounded: "{rounded.sm}"
    height: "36px"
---

# Design System: Kairo

## Overview

**Creative North Star: "Objects on Paper"**

Each of Kairo's apps is drawn as the thing it is: Chrono a watch dial, Ledger a cloth notebook with a gold ribbon, the command palette a keycap, Kairo itself a ring with one stretch of it lit gold, the moment. They sit on a sheet of cream paper by day, or on deep blue ink by night (the default), with a faint grain over the ground. Everything else is ruled lines, small capitals and quiet sheets, so the objects and the numbers carry the character.

It is a tool used for seconds at a time, many times a day, so density is moderate and calm: one display face for titles and big numbers, one text face for everything you read and press, a mono face for keys and figures. Each window takes its app's ink as its accent, the same way a notebook and a watch keep their own colour on one desk; the data colours (productive, distracting, the blue ramp) mean the same thing everywhere.

The look is defined by `src/styles.css` (tokens on `:root`, dark first, `[data-theme="light"]` for paper), the drawings in `src/logos.ts`, and the native Windows palette and ring, which copy the same values in Rust and must stay in step.

**Key Characteristics:**
- Real objects as app marks, drawn inline in each app's ink; no image files.
- Two grounds at heart: deep blue ink (default) and cream paper, each with a fine grain. The other themes are grounds over one of them, swapping only the paper and its text inks: Graphite, Walnut, Baize and Midnight over ink; Linen and Sage over paper. App inks and data colours stay those of the set beneath (`data-ground` in `src/styles.css`, `GROUNDS` in `src-tauri/src/appearance.rs`).
- Fraunces 800 for titles and figures, with an italic 400 aside in the app's ink.
- Section labels in small capitals with wide letter-spacing.
- Kairo's own controls everywhere the system's would ignore the theme: dropdowns, the calendar, time lists.

## Colors

Warm paper and cool ink, with four app inks for accents and a separate set of data colours that never change meaning.

### Primary
- **Each app's ink** (Chrono's lake blue, Ledger's leather amber, the palette's violet, Kairo's pewter): set as `--app` by the window it belongs to, and used for primary buttons, links, focus outlines, selection, the active tab, the lit calendar day and the palette's bubble. Light mode uses the deeper of each pair so it holds its contrast on paper.

### Secondary
- **Ribbon Gold** (`ribbon-gold`): Ledger's ribbon, a pinned note, and the lit stretch of Kairo's own ring mark. Decoration and marks only, never text. (Medium priority, every task's to begin with, wears no mark: High is a filled dot in Ledger's ink, Low a faint ring.)

### Neutral
- **Deep Ink / Ink Sheet** (`ink-deep`, `ink-sheet`): the dark ground and its raised sheets.
- **Paper / Paper Sheet** (`paper`, `paper-sheet`): the light ground and its sheets.
- **Text, second text, muted, faint**: four steps of ink on each ground. Faint is the floor for small labels and still meets 4.5:1 on a sheet.
- **Rule and line**: rule separates (sidebar edge, sheet edge); line outlines controls (fields, buttons, dropdowns).

### Data colours
- **Productive blue, distracting orange, neutral grey, the seven-step blue ramp**: charts, dials, timelines and the heatmap. Fills only.
- **Distracting-ink, good-ink, warm-ink**: the same colours deepened for when they are words ("Overdue", "2 days late", "+18% vs last week").

### Named Rules
**The Whose Window Rule.** Accent colour comes from `--app`, never a literal: the same component is blue in Chrono and amber in Ledger.

**The Shown Rule.** What's shown (the sidebar's page, a range toggle, a length preset, the dashboard's tab) is the app's ink on a 14% tint of it. On paper the tint costs the ink its contrast, so there the ink is mixed 65/35 with the text ink to stay above 4.5:1.

**The Fill Is Not Ink Rule.** A data colour used as small text takes its `-ink` variant; fills keep the original so charts read the same.

## Typography

**Display Font:** Fraunces Variable (with Georgia, serif)
**Body Font:** Instrument Sans Variable (with system-ui)
**Label/Mono Font:** Geist Mono Variable (with ui-monospace)

**Character:** a soft, bookish serif with real weight for the few things that deserve it, beside a plain modern sans that stays out of the way.

### Hierarchy
- **Hero** (Fraunces 800, 84px): Today's total beside its dial.
- **Figure** (Fraunces 800, 64px, tabular): the focus timer. With hours ("1:30:00") it steps down to **figure-hours** (48px) to stay inside its ring.
- **Display** (Fraunces 800, 42px, 1.08): page titles and greetings; the aside in italic 400 in the app's ink ("Good evening, *nearly done.*").
- **Headline** (Fraunces 800, 22px): wordmarks and app names on the dashboard.
- **Title** (Fraunces 800, 16px): the lit name on the ring, panel titles.
- **Data** (Geist Mono, 30px and 18px): an insight's metric; stat values.
- **Total** (Instrument Sans 300, 40px): an answer's total in the palette.
- **Input** (Instrument Sans, 17px): the palette's search box.
- **Body** (Instrument Sans 400, 14px, 1.5): everything read and pressed.
- **Small** (Instrument Sans 400, 13px): palette notes, chips, the bubble.
- **Label** (Instrument Sans 600, 11px, 0.12em, uppercase): section labels, in faint.
- **Caption** (Instrument Sans 600, 10px, 0.14em, uppercase): group headings and field names in panels; chart labels.
- **Mono** (Geist Mono, 11 to 12px): keys, durations, tabular data, the note editor.

### Named Rules
**The Two Weights Rule.** Fraunces is 800 or an italic 400 aside; nothing in between.

## Layout

App windows are a 200px sidebar (wordmark, sections, Settings at the foot) beside a scrolling page of at most 1040px; Kairo's dashboard is the one without a sidebar, its Home and Settings tabs in the page head, since it is Kairo's way into the apps rather than an app itself. Pages are padded 28px on top and 40px at the sides. Windows open at 1120 by 780 and never go below 720 by 520, so every view must fit at that size without sideways scrolling. Ledger's task details slide in as a sheet over the right of the list. The palette and the ring are small floating windows of their own (640 to 680px and 300 by 210px).

## Elevation & Depth

Mostly flat: depth comes from the ground and sheet tones and from a hairline rule, with shadow kept for things that float.

### Shadow Vocabulary
- **Sheet rest** (`0 10px 24px -18px var(--shadow)`): a soft lift under sheets and cards.
- **Button** (`inset 0 1px 0` ink at 6%, `0 1px 2px var(--shadow)`): a pressable edge.
- **Float** (`0 8px 24px var(--shadow)`): popovers, the calendar, dropdown lists.
- **Keycap** (`0 7px 0` palette ink at 40%, `0 12px 18px -6px var(--shadow)`): the palette's mark only.

### Named Rules
**The Float Only Rule.** Strong shadows belong to what floats above the page (popovers, lists, the palette); everything at rest is a sheet.

## Shapes

Gently rounded: 10px for sheets, fields and buttons, 6 to 8px for small targets like calendar days and options, a full pill for chips, the palette's bubble and the "← Kairo" link, a hair (2 to 3px) for bars and timeline segments, 14px for the palette's floating panel, and 16px for the keycap the dashboard draws. Lines are hairlines (1px). Objects are drawn with round caps and joins.

## Components

### Buttons
- **Shape:** gently curved (10px), 36px tall.
- **Default:** a sheet with a line border and the pressable edge shadow.
- **Primary:** filled with the app's ink, its text in `--on-app`.
- **Focus:** a solid 2px outline in the app's ink, 2px out. Never a faint glow.

### Fields
- **Style:** a sheet with a line border and a faint inner shadow, 10px corners, 38px tall.
- **Focus:** the border and a 2px outline in the app's ink.
- **Picked values:** a day is a field-styled button that opens Kairo's calendar; a time is Kairo's dropdown of quarter hours. Never the system's date or time field.

### Chips
- **Style:** pill, line tint ground, muted text, 11px. "Late" takes the distracting tint with distracting-ink text.

### Dropdowns (`src/dropdown.ts`)
- Every `<select>` becomes a themed button and floating list. In settings the button is underlined; in a panel beside fields it takes the field's box.

### Calendar (`src/calendar.ts`)
- A month on a floating sheet, Monday first, weekday initials in mono. Today has a line ring, the picked day is filled with the app's ink with `--on-app` text, and Chrono's days carry a dot from the blue ramp for time tracked.

### Palette bubble
- A pill before the palette's input naming the mode it's in (Calculator, GitHub, Files), its glyph and name in the palette's ink on an 18% tint of it. Drawn the same in the web and native palettes.

## Do's and Don'ts

### Do:
- **Do** take every colour from the tokens on `:root`, and define a new one for both grounds.
- **Do** use `--app` for accents and the `-ink` variants for data colours as text.
- **Do** draw the edge of anything pressed or typed into (buttons, fields, toggles, checkboxes) with `--edge`, which holds 3:1 on page and sheet; `--line` and `--rule` are for dividers and containers.
- **Do** draw icons and marks inline in the 24-unit stroke family (or the 100-unit object drawings in `src/logos.ts`).
- **Do** give every motion an alternative under `prefers-reduced-motion`.
- **Do** change the native Windows palette and ring in step with the web ones when a shared value moves.

### Don't:
- **Don't** use the system's date, time or select pickers; they ignore the theme.
- **Don't** remove the outline on focus without putting a solid one back.
- **Don't** set small text in the faint grey below 4.5:1, or a data fill colour as words.
- **Don't** hard-code a colour in a component.
- **Don't** add image files for marks or icons.
