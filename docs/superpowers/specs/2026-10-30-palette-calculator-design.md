# Calculator in the command palette

## Goal

Typing maths or a unit conversion into the palette shows the answer as the top row, ready to
copy, on Windows, macOS and Linux, offline. Normal searches don't change.

## What the user sees

- Queries like `12*7+3`, `15% of 240`, `2^10`, `sqrt(2)`, `5 km in mi`, `180 lb to kg`,
  `30 c in f`, `2 gb in mb`, `90 min in h`, `3 days in hours` show one row in a **Calculator**
  section, first in the list.
- The row's title is the answer (`3.10686 mi`), its second line is the query as worked out
  (`5 km in mi`), and its icon is a new calculator glyph.
- **Enter** copies the answer with its unit (`3.10686 mi`). **Ctrl+Enter** (⌘+Enter on a Mac)
  copies the number alone (`3.10686`); the footer's second-action hint reads "Copy number". A
  unitless answer has no second action, since both would copy the same text.
- Copying works like the palette's other copy rows: the text goes on the clipboard and the
  palette closes.
- No currency: Kairo stays offline.

## When a query counts as maths

Kairo decides before asking the calculator. A query is maths when it has a digit and either:

- an operator: `+ - * / × ÷ ^ %`, `(` or `)`, or a function call like `sqrt(`; or
- a conversion: a number, then a unit, then `in`, `to` or `as`, then a unit.

It isn't, and no row shows, when:

- it's a focus command (`focus 50 essay`), whose meaning stays as it is;
- it reads as a day (`14 oct`, `2026-10-30`), which `parse_day` already handles;
- it's a bare number (`2026`);
- the answer's text is the same as the query's, ignoring spaces (`(5)` gives `5`, nothing new);
- the calculator fails or gives an error (`abc + 1`): no row, never an error message;
- the calculation runs past 100 ms (fend's interrupt), so an odd input can't stall the palette.

## Shorthand Kairo rewrites first

fend reads some everyday shorthand as other units, so Kairo rewrites whole words, ignoring case,
before asking:

- In a conversion whose sides are temperatures: a lone `c` → `°C`, `f` → `°F`, `k` → `K`
  (`30 c in f` → `30 °C in °F`; fend alone reads `c` as the speed of light).
- Data sizes: `kb`, `mb`, `gb`, `tb` (and `kib`, `mib`, `gib`, `tib`) → `KB`, `MB`, `GB`, `TB`
  (`KiB` …), since fend reads lowercase `mb` as millibits.

The second line shows the query as typed, not the rewrite.

## Formatting the answer

- fend's `approx. ` prefix is dropped.
- Numbers are rounded to 6 significant digits, trailing zeros trimmed (`3.1068559612` →
  `3.10686`, `0.3333333333` → `0.333333`, `2.5` stays).
- A number with more than 15 digits before the point, or a non-zero one smaller than 0.000001,
  is shown in scientific notation (`1e200`, `3.2e-9`).
- The unit is kept as fend writes it (`mi`, `kg`, `°F`, `hours`).

## How it's built

- **`src-tauri/src/palette/calc.rs`** (new): `is_maths(query) -> bool`,
  `rewrite(query) -> String` and `evaluate(query) -> Option<Answer>`, where `Answer` holds the
  text with its unit and the number alone (None when unitless). It wraps `fend_core::evaluate`
  with an interrupt at 100 ms, and does the formatting above. No drawing and no I/O, so it's
  all unit-tested.
- **`search.rs`**: `search()` puts the calculator's row first (score above every other row) when
  `calc::evaluate` answers. New `Action::Copy(String)`.
- **`mod.rs`**: `Action::Copy(text)` writes `text` to the clipboard through the clipboard plugin
  already used by `CopyAnswer`, and the palette closes.
- **New `Glyph::Calc`**, drawn in `native.rs` (Windows, Direct2D) and in `src/palette.ts`'s
  stroke icons (macOS and Linux).
- **Dependency**: `fend-core` (pure Rust, no dependencies of its own) in `src-tauri/Cargo.toml`
  for every platform.

## Testing

- `calc.rs` unit tests:
  - `is_maths`: yes for the examples above; no for `focus 50`, `14 oct`, `2026-10-30`, `2026`,
    `slack this week`, `finals`.
  - Answers for each example in "What the user sees", including the rewrites
    (`30 c in f` → `86 °F`, `2 gb in mb` → `2000 MB`).
  - Formatting: rounding, trimming, scientific notation, `approx.` dropped.
  - Nonsense (`abc + 1`) gives None.
- `search.rs` test: `5 km in mi` puts the calculator row first with `Action::Copy`; `focus 50`
  still gives the focus row and no calculator row.
- Vitest: the web palette draws the `calc` glyph.
- `docs/device-checklist.md`: a short Calculator section under the palette, for all three
  systems.

## Out of scope

Currency, variables and history of past answers, and plotting.
