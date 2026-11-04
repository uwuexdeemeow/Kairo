---
target: dashboard
total_score: 25
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:D:\\Kairo\\dashboard.html"
target_fingerprint: "sha256:ed182287439f6a86d93e9bf7f0aec4ec353d32109de7606aea867c49e075de8b"
target_path: "D:\\Kairo\\dashboard.html"
timestamp: 2026-11-04T14-37-10Z
slug: dashboard-html
---
Method: dual-agent (A: design review · B: detector evidence). Source-only. First run.

| # | Heuristic | Score | Key issue |
|---|---|---|---|
| 1 | Visibility | 3 | Palette toggles and autostart save silently; tile line blank if first read fails |
| 2 | Real world | 3 | Objects excellent; "Show"/"Open" programmer-speak |
| 3 | Control | 2 | Stop combining one click, affects other computers |
| 4 | Consistency | 2 | .btn vs .link-btn; grey App ring title; only Start over confirms |
| 5 | Error prevention | 2 | Restore looks like Export; Stop combining unconfirmed |
| 6 | Recognition | 3 | No index; App settings last |
| 7 | Efficiency | 2 | No tab/app keys; ring editor loses focus per change |
| 8 | Minimal | 3 | Home calm; Settings prose-heavy |
| 9 | Recovery | 2 | Raw errors; failed reads leave defaults posing as settings |
| 10 | Help | 3 | Plenty of inline help |
| **Total** | | **25/40** | Acceptable |

## Priority issues
1. [P1] Ring editor rebuilds its list on every change (ring-editor.ts list.replaceChildren, twice per save), dropping focus. Fix: restore focus to the same item's control or update in place. /impeccable harden
2. [P1] Home tiles fall 3+1 (auto-fill 200px in 740px); App ring title grey; no 4th delay. Fix: 4 in a row / 2×2 narrow, ink the title, add delay. /impeccable layout
3. [P1] Settings one flat 9-section scroll; VirusTotal top-level; App settings last; tiny headings. Fix: ~4 groups with real headings, App settings first or an index. /impeccable layout
4. [P2] Sequential settings load: an early failure leaves later controls showing defaults; tile lines blank on failure. Fix: allSettled per section, "Couldn't read this" + disabled controls, tile fallback. /impeccable harden
5. [P2] Stop combining unconfirmed; Restore looks safe. Fix: ask-twice pattern, set Restore apart. /impeccable clarify

## Minor
Home tile lines are live regions rebuilt every 30 s; settings/ring checkboxes lack app focus ring; 11–12px headings; light pewter ~4.3:1 small; unstyled kbd in VirusTotal copy; palette tile filler line; minutes field validates late, away from field.
