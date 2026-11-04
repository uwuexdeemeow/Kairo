---
target: dashboard
total_score: 28
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 2
target_identity: "file:D:\\Kairo\\dashboard.html"
target_fingerprint: "sha256:7b011651c55cee786100aa3a9a67147620fac225ad89e3551ebfd9c846595d6c"
target_path: "D:\\Kairo\\dashboard.html"
timestamp: 2026-11-04T16-03-23Z
slug: dashboard-html
---
Method: dual-agent (A: design review · B: detector)

## Dashboard, 28/40 (Good)
| # | Heuristic | Score | Key issue |
|---|---|---|---|
| 1 | Visibility of status | 3 | Theme change unconfirmed; update result only as OS notification |
| 2 | Real world | 3 | Palette tile line describes settings, not today |
| 3 | Control/freedom | 3 | Ring Reset has no undo |
| 4 | Consistency | 2 | Three destructive-confirm patterns; Home/Settings toggle unlike Ledger's tabs |
| 5 | Error prevention | 3 | Modifier required, minutes bounded |
| 6 | Recognition | 3 | Find a setting, App settings rows |
| 7 | Flexibility | 2 | No shortcut to switch Home/Settings; Find doesn't highlight |
| 8 | Minimalist | 3 | Prose blocks (Combine computers ~85 words) |
| 9 | Error recovery | 3 | Many messages are raw String(err) |
| 10 | Help | 3 | Generous but all shown at once |

Detector: clean on dashboard.html (1 false positive: unstyled scan).

Priority issues
1. [P1] Settings explains everything before any choice (dashboard.html:127,148). clarify, distill
2. [P1] Three confirm patterns; Stop combining press-twice is unannounced and says the consequence only after. harden
3. [P2] Groups wrong: VirusTotal and Search files under Shortcuts; "Your apps" is one section. layout
4. [P2] Only the drawing opens an app; tile lines weak; "Paused" has no Resume. clarify
5. [P2] Home/Settings switch is faint, no tablist, no shortcut. polish

Persona red flags: Alex (no Settings shortcut, Find matches prose); Sam (armed state not announced, tiles announced twice, recorder captures Tab); Jordan (Combine prose, VT key under Shortcuts, update result off-window).
