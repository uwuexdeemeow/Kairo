⚠️ DEGRADED: single-context (sub-agents are only started when the user asks for them in this session)

# Critique: the whole app after Raycast parity (1.36.0)

Seen in the running app (Linux, WebKitGTK, Openbox under Xvfb) with demo data, Paper and Ink:
the palette (empty, agenda, calculator history, recent files, join, remind), the dashboard home and
settings, Chrono's Today, Week, Patterns and Focus, and Ledger's tasks. The Windows native palette
can't be seen here.

| # | Heuristic | Score | Key issue |
|---|-----------|-------|-----------|
| 1 | Visibility of system status | 3 | Rows say when things happen and why; footer names Enter's verb |
| 2 | Match system / real world | 3 | Times were 24-hour in the palette, 12-hour elsewhere (fixed) |
| 3 | User control and freedom | 3 | Ctrl+Enter alternatives on most new rows |
| 4 | Consistency and standards | 3 | Clock format (fixed); folds used consistently in settings now |
| 5 | Error prevention | 3 | Remind refuses more than a week ahead; aliases can't loop |
| 6 | Recognition rather than recall | 2 | The palette's many words (`agenda`, `recent`, `:fire`) must be known |
| 7 | Flexibility and efficiency | 4 | Keys, keywords, aliases, scripts, usual-now suggestions |
| 8 | Aesthetic and minimalist design | 3 | Palette and ring settings had become a wall of text (fixed) |
| 9 | Error recovery | 3 | Missing pieces are said ("Say when: …", "has no video link") |
| 10 | Help and documentation | 3 | Folds explain each feature; device checklist covers each |
| **Total** | | **30/40** | **Good** |

## Fixed in this run

- [P1] Palette rows said times in 24 hours ("08:44") while Ledger and Chrono follow the system
  ("8:44 AM"). One `quick_add::clock` now follows the system's region everywhere in the palette.
- [P1] Kairo's settings, Palette and ring: Remember what I copy, Your commands and Scripts each
  carried a paragraph or more. Each now has one plain line, with the rest in a fold, as App ring's
  "How to turn it" already did; a switch's fold sits under its words.
- [P2] The settings search would have matched "backup" in the clipboard note; reworded.

## Left as they are

- Recognition over recall (6): the palette's vocabulary grows with each feature. A "what can I
  type" list (`help` or `?`) would answer it; not built yet.
- The dashboard's home is mostly empty below its first screen: deliberate (quick to leave).
- "File search needs localsearch or plocate" shows once per opening on a Linux without an
  index: a tested choice, said once rather than every keystroke.
