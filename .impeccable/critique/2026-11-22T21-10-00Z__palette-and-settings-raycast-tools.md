⚠️ DEGRADED: single-context (sub-agents are only started when the user asks for them in this session)

# Critique: the Raycast-style tools (palette rows, Window layouts and Scripts settings), 1.32.0

Seen in the running app (Linux, WebKitGTK, Openbox under Xvfb), Ink and Paper themes. WebKitGTK has
no DevTools port, so Assessment B is the source scan only (`impeccable detect` on dashboard.html and
palette.html): 2 advisories, both false positives (default black read from markup scanned without
its stylesheet).

| # | Heuristic | Score | Key issue |
|---|-----------|-------|-----------|
| 1 | Visibility of system status | 3 | Footer now names what Enter does; currency says when rates are coming |
| 2 | Match system / real world | 3 | "Left half", "Save layout", emoji names are plain words |
| 3 | User control and freedom | 3 | Restore undoes a snap; pins unpin; Ctrl+Enter copies instead of pasting |
| 4 | Consistency and standards | 3 | Native and web footers share one verb source |
| 5 | Error prevention | 3 | Aliases can't run themselves; links only http/https/mailto |
| 6 | Recognition rather than recall | 2 | Snap words and `:` for emoji must be known; settings now show an example |
| 7 | Flexibility and efficiency | 4 | Aliases, quicklinks, scripts, Enter types a name that wants words |
| 8 | Aesthetic and minimalist design | 3 | Settings help was a wall of chips (fixed) |
| 9 | Error recovery | 3 | Errors say what to do (Accessibility, Wayland, no windows) |
| 10 | Help and documentation | 3 | Example script header in settings |
| **Total** | | **30/40** | **Good** |

## Priority issues (found, then fixed in this run)

- [P1] Kairo's own copies were credited to the app in front ("from Zenity"). Now "from Kairo".
- [P1] Paste failed about half the time on X11: Ctrl and V pressed in the same instant, and a
  dialog's focus not waited for. Fixed; 22 of 22 pastes landed afterwards.
- [P1] Window layouts and Scripts help: ~10 keyboard chips per paragraph. Rewritten shorter, with
  the script header shown as an example.
- [P2] Enter on a link or script that wants words did nothing. It now types the name for you.
- [P2] "↑↓ Move · Enter Move" on window rows; Enter now says "Snap". Notes show no Enter.
- [P2] The app ring preview's faded segments lay over the list's arrow buttons. Clipped at its sides.

## Left as they are

- Spaces around "·" in row subtitles read tight at 12px (existing pattern across the palette).
- Layout save and restore not seen end to end: the test apps (xclock, xterm) don't publish a
  process id, so they can't be matched to an app.
