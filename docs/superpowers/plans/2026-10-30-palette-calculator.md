# Palette Calculator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Maths and unit conversions typed into the command palette show their answer as the first row, copied with Enter, on every platform.

**Architecture:** A new pure module `palette/calc.rs` decides whether a query is maths, rewrites shorthand, asks `fend-core` with a 100 ms interrupt and formats the answer. `search::search()` adds its row first, with a new `Action::Copy`; the Windows (native) and macOS/Linux (web) palettes draw a new `calc` glyph and keep the row under a "Calculator" heading.

**Tech Stack:** Rust (Tauri 2), `fend-core` 1.5.9, TypeScript + Vitest for the web palette.

**Spec:** `docs/superpowers/specs/2026-10-30-palette-calculator-design.md`

## Global Constraints

- Works offline on Windows, macOS and Linux; no currency.
- Section name: `"Calculator"`. Second-action label: `"Copy number"`.
- Calculation time limit: 100 ms.
- Rounding: 6 significant digits, trailing zeros trimmed; scientific notation when |x| ≥ 1e15 or 0 < |x| < 1e-6; `approx. ` dropped.
- Rewrites (whole words, any case): data sizes `kb mb gb tb kib mib gib tib` → `KB MB GB TB KiB MiB GiB TiB`; in a conversion whose sides are both temperature words, lone `c`/`f`/`k` → `°C`/`°F`/`K`.
- The row's second line is the query as typed, trimmed.
- Comments and names follow the codebase's style: plain sentences, `///` on every item.

## Review Focus

- Normal queries that contain digits must not grow a calculator row: `focus 50`, `14 oct`, `2026-10-30`, `2026`, `kairo this week`, `last 7 days` (no operator, no conversion) → no row.
- A query whose letters fend reads as units but the user meant a search (`5 km` with no conversion, `chrome 2`) → no row.
- Negative and decimal answers format correctly: `3-10` → `-7`, `-0.5*3` → `-1.5`.
- An answer that is only a unit-less huge integer (`2^64`) → `1.84467e19`, not 20 digits.
- Typing fast: evaluation is synchronous inside `search()`, so a slow expression (`10^10^10`) must hit the 100 ms interrupt and give no row.

---

### Task 1: The calculator module

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `fend-core = "1.5"` under `[dependencies]`, all platforms)
- Create: `src-tauri/src/palette/calc.rs` (tests inline, `#[cfg(test)] mod tests` at the end, as in the other palette modules)
- Modify: `src-tauri/src/palette/mod.rs` (add `pub mod calc;` beside `pub mod cards;`)

**Interfaces:**
- Produces:
  - `pub struct Answer { pub text: String, pub number: Option<String> }` — `text` is the formatted answer with its unit (`"3.10686 mi"`); `number` is the formatted number alone, `None` when the answer has no unit.
  - `pub fn answer(query: &str) -> Option<Answer>` — `None` unless `is_maths(query)`, fend answers within 100 ms without error, and the answer differs from the query (compared without spaces).
  - `pub fn is_maths(query: &str) -> bool`, `pub fn rewrite(query: &str) -> String`, `fn format(result: &str) -> Option<(String, Option<String>)>` (private, tested).
  - It does not know about focus commands or days; `search()` excludes those (Task 2).

- [ ] **Step 1: Write the failing tests** in `calc.rs`:

```rust
#[test]
fn maths_needs_a_digit_and_an_operator_or_a_conversion() {
    for q in ["12*7+3", "15% of 240", "2^10", "sqrt(2)", "5 km in mi", "180 lb to kg", "30 c in f", "2 gb in mb", "90 min in h", "3 days in hours", "3-10", "(5)"] {
        assert!(is_maths(q), "{q}");
    }
    for q in ["2026", "kairo this week", "last 7 days", "5 km", "chrome 2", "finals", "", "in to as"] {
        assert!(!is_maths(q), "{q}");
    }
}

#[test]
fn shorthand_is_rewritten_before_asking() {
    assert_eq!(rewrite("30 c in f"), "30 °C in °F");
    assert_eq!(rewrite("300 K to c"), "300 K to °C");
    assert_eq!(rewrite("2 gb in mb"), "2 GB in MB");
    assert_eq!(rewrite("4 GiB to mib"), "4 GiB to MiB");
    // A lone letter outside a temperature conversion is left alone.
    assert_eq!(rewrite("c + 1"), "c + 1");
}

#[test]
fn answers_for_everyday_queries() {
    let text = |q: &str| answer(q).map(|a| a.text);
    assert_eq!(text("12*7+3").as_deref(), Some("87"));
    assert_eq!(text("15% of 240").as_deref(), Some("36"));
    assert_eq!(text("2^10").as_deref(), Some("1024"));
    assert_eq!(text("5 km in mi").as_deref(), Some("3.10686 mi"));
    assert_eq!(text("30 c in f").as_deref(), Some("86 °F"));
    assert_eq!(text("2 gb in mb").as_deref(), Some("2000 MB"));
    assert_eq!(text("90 min in h").as_deref(), Some("1.5 h"));
    assert_eq!(text("3-10").as_deref(), Some("-7"));
    assert_eq!(text("-0.5*3").as_deref(), Some("-1.5"));
    assert_eq!(answer("5 km in mi").unwrap().number.as_deref(), Some("3.10686"));
    assert_eq!(answer("12*7+3").unwrap().number, None);
}

#[test]
fn nothing_for_nonsense_repeats_or_slow_sums() {
    assert!(answer("abc + 1").is_none());
    assert!(answer("(5)").is_none());
    let start = std::time::Instant::now();
    assert!(answer("10^10^10").is_none());
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
}

#[test]
fn answers_are_rounded_and_long_ones_go_scientific() {
    assert_eq!(format("approx. 3.1068559612 mi"), Some(("3.10686 mi".into(), Some("3.10686".into()))));
    assert_eq!(format("approx. 0.3333333333"), Some(("0.333333".into(), None)));
    assert_eq!(format("2.5"), Some(("2.5".into(), None)));
    assert_eq!(format("18446744073709551616"), Some(("1.84467e19".into(), None)));
    assert_eq!(format("0.0000000032 s"), Some(("3.2e-9 s".into(), Some("3.2e-9".into()))));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test --lib palette::calc`
Expected: compile errors (`answer`, `is_maths`, `rewrite`, `format` not found).

- [ ] **Step 3: Implement `calc.rs`**

- `is_maths`: has an ASCII digit, and either one of `+ - * / × ÷ ^ % ( )` (`-5` alone passes here and is then dropped by `answer`, since its answer repeats it), or the words match `<number> <word>… (in|to|as) <word>…` with a unit word on both sides (any letters; fend decides if they're units).
- `rewrite`: split on whitespace, keep separators; data-size words by a lookup table; temperature letters only when both sides of the `in|to|as` word are a single temperature token (`c f k °c °f celsius fahrenheit kelvin`, any case).
- `answer`: `fend_core::evaluate_with_interrupt(&rewrite(q), &mut fend_core::Context::new(), &Deadline(Instant::now() + 100ms))`, where `struct Deadline(Instant)` implements `fend_core::Interrupt::should_interrupt` as `Instant::now() >= self.0`; then `get_main_result()` through `format`; drop it when it equals the query with spaces removed.
- `format`: strip `approx. `; split at the first space into number and unit; parse the number as `f64`, round to 6 significant digits, print plainly with trailing zeros trimmed, or as `{mantissa}e{exp}` (mantissa trimmed the same way) under the scientific rule; `None` if the number part doesn't parse.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd src-tauri && cargo test --lib palette::calc`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/palette/calc.rs src-tauri/src/palette/mod.rs
git commit -m "Work out maths and unit conversions for the palette with fend"
```

### Task 2: The calculator row in search, copying, and the Windows palette

**Files:**
- Modify: `src-tauri/src/palette/search.rs` (`Glyph`, `Action`, `search()` at ~line 470, tests at the end)
- Modify: `src-tauri/src/palette/mod.rs` (run `Action::Copy` beside `Action::CopyAnswer` ~line 409; glyph name `Glyph::Calc => "calc"` beside `Glyph::Alert => "alert"` ~line 503)
- Modify: `src-tauri/src/palette/native.rs` (`heading()` ~line 317 and its test ~line 1597; `glyph()` ~line 1130)

**Interfaces:**
- Consumes: `calc::answer(query) -> Option<Answer>` (Task 1).
- Produces: `Glyph::Calc` (serialised to the web palette as `"calc"`), `Action::Copy(String)`, section `"Calculator"`.

- [ ] **Step 1: Write the failing tests**

In `search.rs` tests, using the existing test context helper there:

```rust
#[test]
fn maths_leads_with_a_row_that_copies_the_answer() {
    let items = search(&ctx(), "5 km in mi");
    let first = &items[0];
    assert_eq!(first.section, "Calculator");
    assert_eq!(first.title, "3.10686 mi");
    assert_eq!(first.sub.as_deref(), Some("5 km in mi"));
    assert_eq!(first.action, Action::Copy("3.10686 mi".into()));
    assert_eq!(first.second, Some((Action::Copy("3.10686".into()), "Copy number")));
    assert!(search(&ctx(), "12*7+3")[0].second.is_none());
}

#[test]
fn focus_commands_and_days_never_become_sums() {
    for q in ["focus 50", "2026-10-30", "14 oct"] {
        assert!(search(&ctx(), q).iter().all(|i| i.section != "Calculator"), "{q}");
    }
}
```

In `native.rs`'s `a_file_is_never_the_top_hit` test, add: an item with section `"Calculator"` at index 0 while typing has heading `"Calculator"`.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test --lib palette::`
Expected: compile errors (`Action::Copy`, `Glyph::Calc` missing).

- [ ] **Step 3: Implement**

- `search.rs`: add `Glyph::Calc` and `Action::Copy(String)` (doc: "Puts this text on the clipboard."). In `search()`, before the focus command, when `focus_command(query)` and `parse_day(query, ctx.today)` are both `None` and `calc::answer(query)` answers: push an item titled `answer.text`, `sub` = trimmed query, icon `Icon::Glyph(Glyph::Calc, Tone::Accent)`, section `"Calculator"`, action `Action::Copy(answer.text)`, `second` = `answer.number.map(|n| (Action::Copy(n), "Copy number"))`, score `2000.0`.
- `mod.rs`: `Action::Copy(text)` writes `text` with `app.clipboard().write_text(text.clone())`, like `CopyAnswer`. Add `Glyph::Calc => "calc"` to the glyph-name match.
- `native.rs`: `heading()` keeps the section for `"Calculator"` as it does for `"Files"` (`item.section != "Files" && item.section != "Calculator"`); update its doc and the matching comment. In `glyph()`, `Some(Glyph::Calc)`: a rounded rectangle from (5,3) to (19,21) radius 2, a display line from (8,7) to (16,7), and four keys as short lines `(8.5,12)-(9.5,12)`, `(14.5,12)-(15.5,12)`, `(8.5,17)-(9.5,17)`, `(14.5,17)-(15.5,17)`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd src-tauri && cargo test`
Expected: all pass (136 before + 5 calc + 2 search).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/palette/search.rs src-tauri/src/palette/mod.rs src-tauri/src/palette/native.rs
git commit -m "Show the calculator's answer first in the palette, and copy it with Enter"
```

### Task 3: The web palette (macOS and Linux) and the device checklist

**Files:**
- Modify: `src/palette.ts` (stroke icon paths ~line 46; heading rule ~line 175)
- Modify: `src/palette.test.ts`
- Modify: `docs/device-checklist.md` (new section after "Palette: files and settings")

**Interfaces:**
- Consumes: glyph name `"calc"`, section `"Calculator"` (Task 2).

- [ ] **Step 1: Write the failing tests** in `palette.test.ts`, alongside "draw a glyph by kind on Linux":

```ts
it("draws the calculator glyph and keeps its heading", async () => {
  // A typed query whose first row is { section: "Calculator", icon: { kind: "glyph", glyph: "calc", tone: "accent" } }
  // renders an svg with paths inside .palette-glyph and a "Calculator" label, not "Top hit".
});
```

Assertions: `document.querySelector(".palette-glyph svg path")` is not null; the first `.palette-label` text is `"Calculator"`.

- [ ] **Step 2: Run it to see it fail**

Run: `npx vitest run src/palette.test.ts`
Expected: FAIL (no `calc` path; heading is "Top hit").

- [ ] **Step 3: Implement**

- Add `calc` to the stroke icon paths, the same shapes as the Windows glyph: `'<rect x="5" y="3" width="14" height="18" rx="2" /><path d="M8 7h8M8.5 12h1M14.5 12h1M8.5 17h1M14.5 17h1" />'`.
- The heading rule keeps `"Calculator"` as it keeps `"Files"`; update the comment.

- [ ] **Step 4: Run the tests to see them pass**

Run: `npx vitest run`
Expected: all pass.

- [ ] **Step 5: Add the checklist section** to `docs/device-checklist.md`:

```markdown
## Palette: calculator

On Windows, a Mac and Linux. Open the palette.

1. **Maths.** Type `12*7+3`. *Expect:* "87" first, under Calculator. Press Enter and paste
   somewhere. *Expect:* 87.
2. **Units.** Type `5 km in mi`. *Expect:* "3.10686 mi", with "5 km in mi" underneath. Ctrl+Enter
   (⌘+Enter on a Mac) copies 3.10686; the footer said "Copy number".
3. **Shorthand.** `30 c in f` gives 86 °F; `2 gb in mb` gives 2000 MB.
4. **Searches stay searches.** `focus 50`, `14 oct` and `2026` show what they did before, with no
   Calculator row.
```

- [ ] **Step 6: Commit**

```bash
git add src/palette.ts src/palette.test.ts docs/device-checklist.md
git commit -m "Draw the calculator row in the web palette, and add it to the device checklist"
```
