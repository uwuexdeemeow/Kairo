# A weekly review in Ledger

Built on what Kairo already records. No new storage engine, network or dependency; one small field on
a task. Built in 1.31.60, with these differences from the draft:
- The nudge is its own Ledger notification beside "Plan the day?" and "Wrap up the day?", not a second
  line on Chrono's recap (a notification opens one place, and the recap's is Chrono). It is off until
  chosen, like those two, and goes with Ledger's reminders switch.
- Review is the seventh Ledger shortcut, Ctrl+7, so Settings keeps its Ctrl+6.
- "Do it" puts a task on the next working day, and Later clears its day.
- The three picks are made in one review; tasks already due next week are listed above the picker.
- The palette's Plan row no longer carries the word "review", which is Review's.

## Goal
Once a week, end the week on purpose: see what got done, face what keeps slipping, and choose what
next week is for. Today the pieces exist apart (Chrono's week recap, Ledger's Done view, Plan for a
single day), and nothing walks you from one to the other.

## What exists, and what this doesn't redo
- **Chrono's week recap** (`recap.rs`): totals against the week before, each day, top apps and
  projects, goals, and a Sunday 18:00 notification. The review shows its numbers in one line and
  links to it; it doesn't recompute or restyle them.
- **Done view** (`ledger-done.ts`): this week against last, by day. The review reuses its `totals`
  and `weekStart`.
- **Plan** (`ledger-plan.ts`): one day, with Tomorrow, Later, Put at and Move all. The review's last
  step hands each chosen task to Plan's own actions, so there is one way to put a task on a day.

## What the person sees
A fourth Ledger view, **Review**, beside Plan, Tasks and Done. It is a single page of four short
steps, top to bottom, not a wizard; nothing is required and nothing blocks leaving.

1. **The week.** "This week: 14 tasks done, 6h 20m of focus; last week 11 and 5h." Under it, one
   link, "The week in Chrono", which opens Chrono's recap for the same Monday to Sunday. A week with
   nothing done says so in a plain line and skips the comparison.
2. **What slipped.** Open tasks that were overdue at some point this week or moved to another day two
   or more times, each with how many times ("Moved 3 times"). Per task: **Do it** (puts it on a
   day), **Later** (clears its day), **Drop** (deletes, with the page's Undo). A week with none says
   "Nothing slipped."
3. **Next week.** Up to three tasks picked as the week's focus, from the open tasks, with a search
   field to find one. A picked task gets a day this week (Mon to Fri chips); the choice is the
   task's due day, so Plan and the dashboard already show it. Nothing is picked for you.
4. **Finish.** One button, "Done reviewing", which marks this week reviewed and shows a closing
   line ("Next week has 3 things."). Marking is only for the nudge below; the page stays usable.

Keyboard: the page is plain buttons and fields in reading order; picked tasks and the day chips are
reachable by Tab, and Enter acts. Every result a click shows is also said in a line for screen
readers, as Plan's actions are.

### When it appears
- **Sunday evening and Monday morning**, the existing recap notification gains a second line when the
  week hasn't been reviewed: "Review the week?", and clicking it opens Ledger on Review, as the
  "Plan the day?" notification opens Plan. No new notification: one that already fires says more.
- A switch in Ledger's settings, **"Offer a weekly review"**, on by default, turns the line off.
- The palette: typing `review` or `weekly review` shows an Open Review row, like the other Ledger
  views.

## Data
- **A task remembers how many times it moved.** One new field, `moved`: a count that goes up by one
  whenever Tomorrow, Later, Put at, Move all or a day edit changes a task's due day to a later one.
  It starts at 0 for existing tasks (so the first review has little under "What slipped" beyond
  overdue ones), and is shared with the other fields in `taskmore`, as `repeat` and `steps` are.
  Ticking a task or moving it to an earlier day doesn't change it.
- **Reviewed weeks** are one setting, `review_done_for`, holding the Monday's date of the last
  review, like `recap_sent_for` holds the last recap.
- "Slipped" is computed when the page opens, from `due_day`, `moved` and today; nothing else is
  stored. The week's numbers come from `totals` (tasks) and the week recap command (time).

## Privacy
Everything stays on the device, unless computers are combined, where `moved` travels with its task
as the other task fields do. The page shows task titles and counts only, never window titles.

## Non-goals
- Scores, streaks, grades or a "productivity" number.
- A review for months or years (Chrono's recap has those).
- Writing a journal or reflection prompts; the note a person wants goes in Notes.
- Choosing the week for you, or moving tasks without a click.
- Calendar events, other people's tasks, or reminders on picked tasks (a task keeps the reminder it
  has).
- A review of past weeks; the page is always this week (Sunday's evening and Monday's morning count
  as the week just ending).

## Approaches
- **A Ledger view that reuses Done's totals, Plan's actions and the recap's numbers** (chosen): the
  new part is the "what slipped" list and the week's picks; everything else is existing code.
- **A longer Chrono recap with tasks added**: the recap is about time and is read-only; choosing
  what next week is for belongs next to the tasks.
- **A modal wizard that walks the steps**: heavier, and forces an order on a task that is quicker as
  one page.
- **Storing every move as a history row**: would answer more questions, but the count is all the
  page needs, and a column is lighter than a table.

## Edge cases
- No tasks at all: the page shows the week line and an empty-state sentence, and no steps 2 to 3.
- Review done, then more tasks slip: the page updates as before; "Done reviewing" can be pressed
  again, and the nudge stays off for that week.
- Week boundaries follow the Done view's Monday start in local time.
- Combined computers: two devices that review the same week keep the later `review_done_for`.
- A task moved on one computer and ticked on another keeps `moved` from the later change.

## Built by
`tasks.rs` (`moved`, bumped where due days change; `taskmore` sharing), `ledger-review.ts` (the view:
the four steps, built from `ledger-done.ts` `totals`/`weekStart` and `ledger-plan.ts` actions),
`ledger.ts` and `ledger.html` (the tab and the view), `recap.rs` (the second notification line, the
click target, `review_done_for`), `palette/search.rs` and `palette.ts` (the Open Review row),
`kairo-settings.ts` and `dashboard.html` (the switch, with the Ledger settings), `styles.css`, with
tests beside each TypeScript module and Rust function. Windows, macOS and Linux all get it; the
native palette needs only the new row.

## Check on a real machine before shipping
The Review tab at the narrow window size, the notification's click on Windows and macOS (as the
Plan nudge was checked), and `moved` counting through every route that changes a day: Tomorrow,
Later, Put at, Move all, the date field and the palette's task edits.
