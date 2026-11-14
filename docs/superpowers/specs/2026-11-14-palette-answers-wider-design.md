# Wider answers in the palette: more periods, "vs", and conversion shorthand

Small change. The palette already converts units (`5 km in mi`, `30 c in f`) and answers time
questions (`youtube last week`) with a comparison against the period before. This closes three gaps.

## Goal
Everyday phrasings that today show nothing, or a wrong answer, give the right one, offline, on
every platform. No new dependency, nothing stored.

## What changes
1. **More periods.** `last 7 days` (also `past 7 days`, `past week`), `last 30 days`
   (`past 30 days`, `past month`) and `this year`. The first two chart day by day; this year charts
   month by month (a daily limit isn't drawn on monthly bars). Each compares with the same-length
   stretch before (this year: last year up to the same point).
2. **`X <period> vs <period>`** (also `versus`). The second period replaces the default comparison:
   `youtube this month vs last week` says "+1h 10m vs last week". When the second is already the
   natural one before (`this week vs last week`) it is the ordinary answer. The title carries the
   comparison and Copy the answer includes it.
3. **Conversion shorthand.** `5km in mi`, `72f in c` (number stuck to the unit) and heights:
   `5'10 in cm`, `5'10" in cm`, `6ft 2 in cm`, `6 ft 2in to m`. Today `6ft 2 in cm` is read as
   6 ft × 2 and answers wrongly.

## Non-goals
Currency (offline only), arbitrary date ranges, feet-and-inches in the answer, a new row type.

## Approaches
- Add `Period` variants and an optional "against" period carried with the answer (chosen): the
  card, copy text and title all read one value; small.
- Parse `vs` into a new row type: more UI and no gain over the card's existing comparison line.
- A date-range parser (`oct 3 - oct 9`): larger, not asked for.

## Built by
`search.rs` (periods, `split_compare`), `cards.rs` (spans, bars, labels), `mod.rs` (copy),
`calc.rs` (rewrite), with unit tests for each.
