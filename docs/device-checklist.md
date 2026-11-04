# Checking Kairo on real machines

Kairo's tests and CI builds prove the code compiles and its logic holds on Windows, macOS and
Linux, but how each system grants permissions, delivers keys and moves files only shows on a real
machine. This is what to try on each, with what should happen. Windows was gone through on
27 Oct 2026 (1.9.0): sites from Brave's address bar, the full-screen shortcut block and its
setting, the tray's Command palette, CSV export, backup and restore, and combining two copies
through a folder all worked, after three fixes (borderless maximized full-screen windows, folder
switching, sub-second CSV rows).

If something goes wrong, run Kairo from a terminal so its messages show, and copy what it prints:

- **Mac:** `/Applications/Kairo.app/Contents/MacOS/Kairo`
- **Linux:** `kairo` (from the `.deb`) or the `.AppImage` file itself
- **Windows:** messages aren't shown in release builds; describe what you saw instead.

Kairo goes idle after 2 minutes without input (30 in games and video), so keep using the machine
while checking tracking.

## Tray and dashboard

On Windows and a Mac:

1. **Click.** Left-click the tray icon (menu bar icon on a Mac), then try a double-click.
   *Expect:* the dashboard opens either way, once, never two windows. Right-click (or, on a Mac,
   any other click) shows the menu.
2. **Menu.** *Expect:* Open dashboard, Check for updates, Command palette (with its
   shortcut), Pause tracking and Quit Kairo, in that order. Open dashboard opens the dashboard, and
   if it's already open, brings it forward. Check for updates shows a notification saying Kairo is
   up to date, or that it's installing the newer version.
3. **Settings.** On the dashboard's Home, press Chrono's Settings while Chrono is closed.
   *Expect:* Chrono opens on its Settings page, which starts with a line pointing to Kairo's
   settings; its link opens the dashboard's Settings tab. Change the theme there with Chrono open.
   *Expect:* Chrono repaints without a restart.

On Linux the tray doesn't report clicks, so open the menu instead: *expect* the same five items,
and Open dashboard and Check for updates work as above.

## Mac

1. **Install.** Open the `.dmg`, copy Kairo to Applications, run `xattr -cr /Applications/Kairo.app`
   once, open it. *Expect:* a Screen Recording request (for window titles). Allow it, then quit and
   reopen Kairo when macOS asks.
2. **Tracking.** Use a couple of apps for a few minutes. *Expect:* they show on Today with their
   names and icons, and window titles in Recent sessions.
3. **Sites in Safari or Chrome** (no extension). Visit a site for a minute. *Expect:* macOS asks
   once whether Kairo may control the browser; after allowing, the site shows under Sites.
4. **Sites in Firefox or Opera** (no extension). Visit a site for a minute. *Expect:* one
   Accessibility request (System Settings → Privacy & Security → Accessibility); after allowing,
   the site shows under Sites. If it never does, run Kairo from the terminal and note what it says.
5. **Palette.** Press Option+Space. *Expect:* the palette opens; Esc closes it.
6. **Full screen.** Put an app in full screen (the green button) and press Option+Space.
   *Expect:* nothing. Turn on Kairo's settings → Command palette → "Open it over games and full-screen
   apps too" and press it again. *Expect:* it opens. The menu bar's Kairo menu → Command palette
   opens it either way.
7. **Controller.** Connect a gamepad, open a non-game app, and for 3 minutes touch only the
   controller (move a stick fully, press buttons). *Expect:* the sidebar never says Idle, and macOS
   doesn't ask for Input Monitoring. A controller left alone should still let Kairo go idle.

## Linux, GNOME on Wayland (Ubuntu's default)

1. **Install** the `.deb` (or run the AppImage). *Expect:* a notification asking to log out and
   back in. Do so.
2. **Extension on.** `gnome-extensions list --enabled | grep kairo` *Expect:*
   `kairo@uwuexdeemeow.github.io`.
3. **Tracking.** Use Firefox, Files and the Terminal (native Wayland apps) for a few minutes.
   *Expect:* each shows on Today with its icon.
4. **Idle.** Leave the machine alone for 3 minutes. *Expect:* the sidebar says Idle.
5. **Palette.** Press Ctrl+Space. *Expect:* the palette opens. Settings → Keyboard → View and
   Customise Shortcuts → Custom Shortcuts lists "Kairo command palette". Change the shortcut in Kairo's settings; *expect* the
   GNOME shortcut to follow.
6. **Full screen.** Press F11 in Firefox, then Ctrl+Space. *Expect:* nothing, unless the
   full-screen setting is on.
7. **Focus blocking.** Mark an app distracting in Categories, start a 25-minute focus session and
   switch to it. *Expect:* it minimizes and Kairo's blocked window shows.
8. **Game (Steam, Proton or native).** Play for a minute. *Expect:* it shows under the game's
   name, filed as distracting, as on Windows.
9. **Controller.** As on the Mac, step 7.
10. **Update.** When a newer version is out, use Check for updates in Kairo's settings. *Expect:* the
    `.deb` asks for your password, then Kairo restarts on the new version.

## Linux, KDE Plasma on Wayland

1. **Install** and open Kairo; no logout needed.
2. **Tracking.** Use Dolphin, Konsole and a browser. *Expect:* each shows on Today.
3. **Idle.** Leave it alone 3 minutes. *Expect:* Idle. (If Kairo never goes idle on Plasma, say
   so: its idle time comes from Plasma's screen locker, whose units weren't certain.)
4. **Palette.** Press Ctrl+Space. *Expect:* it opens; System Settings → Shortcuts lists Kairo's
   "Open the command palette".
5. **Focus blocking**, as on GNOME, step 7.

## Palette: files and settings

On Windows, a Mac and Linux. Open the palette with Ctrl+Space (Option+Space on a Mac). Below,
"Ctrl+Enter" means ⌘+Enter on a Mac.

1. **Find a file.** Type part of a file's name from your Documents folder (at least 2 letters).
   *Expect:* the file shows under Files, after the other results, within a second. Up to 5 files
   show.
2. **Open and show.** Press Enter on a file row. *Expect:* the file opens. Search again and press
   Ctrl+Enter. *Expect:* its folder opens (Explorer, Finder or your file manager) with the file
   selected, and the footer said "Show in folder" while the row was selected.
3. **Flip a switch.** Type "login" and press Enter. *Expect:* the row under Settings shows On or
   Off and flips, and the palette stays open. It's titled "Start with Windows", "Open at login"
   (Mac) or "Start at login" (Linux). Open the dashboard's Settings. *Expect:* its switch matches.
4. **Jump to a setting.** Type "evening" and press Ctrl+Enter. *Expect:* Chrono opens on its
   Settings page, scrolled to Evening summary, focused and highlighted for about a second and a
   half. Do the same with "theme". *Expect:* the dashboard opens on Theme the same way (Theme
   isn't a switch, so Enter jumps there too).
5. **Flip a switch with its page open.** Open Chrono's Settings page, then the palette, type
   "nudges" and press Enter. *Expect:* the Focus nudges row flips between On and Off, and the
   switch on the open Settings page changes to match without reopening it.
6. **Strict mode holds.** In Chrono, turn on Strict blocking and start a focus session. In the
   palette, type "focus blocking" and press Enter. *Expect:* the row keeps its On or Off, its
   second line says "Strict mode is on, so these stay as they are until the focus session ends.",
   and it stays selected. End the session afterwards.
7. **Turn file search off.** In the dashboard's Settings → Command palette, turn off "Search
   files", then type the same file name. *Expect:* no Files section.

**Windows only:** from a terminal, run `kairo.exe --search-files <word>`. *Expect:* it prints what
file search finds, a quick way to check Windows Search by itself.

**Linux only:**

- On a system without localsearch (tracker) or plocate/locate, type 2 or more letters. *Expect:*
  one row, "File search needs localsearch or plocate".
- On a system with plocate. *Expect:* results come only from the home folder.
- On a system with plocate, type a short query that matches many system files, like "conf" or
  "lib", with a file in your home folder named so too. *Expect:* that home file still shows.

## Ledger

On Windows, a Mac and Linux.

1. **Open.** From the dashboard's Ledger tile, the tray's Open Ledger, and the palette's `ledger`.
   *Expect:* the same Ledger window each time, never two, with tasks you had on Chrono's Focus
   page already in it (as Medium, with no day).
2. **Quick add.** Type `essay draft !h fri`. *Expect:* "High · Fri …" under the field as you type;
   Enter adds "essay draft" under Upcoming with a red dot and the day. Add `call "tue"`.
   *Expect:* a task called "call tue" under No date.
3. **Groups.** Give one task today's date and another yesterday's (click its title for the panel).
   *Expect:* Overdue (red) at the top, then Today, Upcoming and No date; within each, High first.
   A task's Low button takes its dot away.
4. **Start focus.** Press Start focus on a task. *Expect:* a session for the length last picked on
   Chrono's timer; the row says "Focusing now", and Chrono's Focus page says "Focusing on “…”".
5. **Done.** Tick a task. *Expect:* it folds under "Done (1)"; unticking brings it back.
6. **Due reminder.** In Ledger's Settings, set the morning time a couple of minutes ahead, with a
   task due today. Keep using the computer. *Expect:* within a minute of that time, "Due today:
   …" as a notification, once. On a Mac the first one may ask to allow notifications.
7. **Exact reminder.** Give a task a reminder two minutes ahead. *Expect:* "Reminder: …" at that
   time. Set one, then leave the computer alone past it for 3 minutes. *Expect:* nothing until you
   come back, then the reminder.
8. **Chrono.** On Chrono's Focus page, the Tasks block. *Expect:* a pick of the open tasks in
   Ledger's order with Start focus, and Open Ledger, which opens it.
9. **Palette.** Type `task buy milk !l tmr`. *Expect:* the row says "Low · <tomorrow> · In Ledger";
   Enter adds it to Ledger. Type `tasks`. *Expect:* your open tasks with their due days.
   `focus 25 ess` still says "For your task “essay draft”".
10. **Two computers.** With two combined computers (one on 1.27 or later), give a task a day and
    High on one. *Expect:* within about 5 minutes the other shows the same; a reminder sent on one
    isn't sent again on the other.

## New look

On Windows and a Mac, in light and dark (Kairo's settings → Appearance).

1. **Dashboard.** *Expect:* a greeting for the hour ("Good morning, *let's begin.*" until noon), the
   date in small capitals, and Chrono's dial, Ledger's notebook and the palette's key in a row,
   lined up. The dial's ring shows today's productive (blue), other and distracting (orange) time,
   with today's total in its middle. Hovering lifts each; clicking opens its app.
2. **Paper.** *Expect:* dark is deep blue ink, light is cream paper, both with a faint grain; text
   in Instrument Sans, titles and big numbers in Fraunces. Each window's buttons and links in its
   app's colour: Chrono blue, Ledger amber, the palette violet.
3. **Chrono.** Today: a small dial beside the big number. Focus: a watch whose ring unwinds as a
   session runs; let a short session end with the Focus page open. *Expect:* one pulse.
4. **Ledger.** Tick a task. *Expect:* an ink stroke drawn through it, then it moves to Done. An
   overdue task has a red ribbon in the margin; a pinned note a gold one. Rows line up whatever
   they show (day, bell, note, time).
5. **Palette.** On Windows press Alt+Space. *Expect:* the new paper, the search field like a
   raised key, small-capital section labels, the selected row lit violet. The same on a Mac.
6. **Blocked window.** Start a focus session and open a blocked app. *Expect:* a watch with the
   minutes left over "<app> is blocked".
7. **Reduced motion.** Turn on the system's reduce-motion setting. *Expect:* no settling, lifting,
   ink stroke or pulse; everything still works.

## One window

On Windows and a Mac.

1. **Switching.** Open the dashboard, then Chrono from its dial. *Expect:* the same window shows
   Chrono (no second window), titled "Chrono"; ← Kairo at the top of its sidebar goes back to the
   dashboard. The same with Ledger.
2. **From elsewhere.** With the dashboard open, open Ledger from the palette, then Chrono from the
   ring, then a reminder's task. *Expect:* always the one window, on the right screen.
3. **Size.** Resize the window on Ledger, go back to the dashboard and to Chrono, quit and reopen.
   *Expect:* the same size and place throughout.
4. **Typing kept.** Start typing a note, press ← Kairo within a second, come back. *Expect:* the
   note has what you typed.
5. **Tasks.** *Expect:* cards with chips (due day, 🔔 time, 📝, focus time); ▶ shows under the
   pointer and starts focus; Rename shows on hover; the panel slides over the list and its × or
   Esc closes it. Make the window narrow. *Expect:* chips shrink, then move under the title;
   nothing overlaps or cuts a title to nothing.
6. **Notes.** *Expect:* cards with a preview; the open note on a sheet with B, I and ☐ (each acts on
   the text), Pin, Delete, and "Saving…" then "Saved" underneath. Narrow the window. *Expect:*
   the list, then the note with ← Notes.
7. **Add a task.** Type something without pressing Enter and wait 20 seconds. *Expect:* it's still
   there.

## Ring

On Windows and a Mac (⌥ for Alt).

1. **Hold and turn.** Hold Alt, press Q. *Expect:* a small ring at the pointer, the last segment
   picked lit at the top (Chrono the first time) with its logo, name and line ("2h 05m today").
   Still holding Alt, press Q twice. *Expect:* it turns to the next two. Let go of Alt. *Expect:*
   that app opens and the ring goes.
2. **Back.** Hold Alt, press Q, then Shift+Q. *Expect:* it turns back one.
3. **Tap.** Press Alt+Q and let go at once. *Expect:* the ring stays. ← → and the scroll wheel turn
   it; Enter opens; 1–7 open a segment; Esc, or clicking elsewhere, closes it.
4. **Drag.** Open it with a tap, press on the ring and drag round. *Expect:* it follows and snaps;
   letting go leaves it there. Click the lit one, or press Enter. *Expect:* it opens. A plain
   click on a side segment opens that one.
5. **Edges.** Open it with the pointer in each corner of the screen, and on a second screen.
   *Expect:* always wholly on the screen.
6. **Actions.** New task: Ledger with "Add a task" ready. New note: a new note. Focus 25: a
   25-minute session; with one running, it's dimmed and opening it says one is running.
7. **Elsewhere.** The tray's App ring and the palette's `ring` open it too. In Kairo's settings,
   change its shortcut; trying the palette's keys says they open the palette.
8. **Light and dark.** *Expect:* paper or deep blue ink, nothing around the ring but the screen.
9. **Games.** In a full-screen game, Alt+Q does nothing unless "Open over games and full-screen apps
   too" is on.

## Ring: arranged your way

On Windows, a Mac and Linux. Kairo's dashboard.

1. **Tile.** Home. *Expect:* an App ring tile, its picture an arc of segments, its line the shortcut
   as keys and "· 7 items". Clicking the picture opens the ring.
2. **Arrange.** Settings → App ring → What's on it. Tick This week, All notes and Pause tracking;
   move All notes to the top with ↑. *Expect:* the preview redraws each time; the ring then opens
   with All notes first (if it was the last one used, on it) and the new ones drawn with their own
   marks: a sheet behind a sheet, a week's bars, two bars.
3. **Limits.** Untick until three are left. *Expect:* the last three can't be unticked, and the line
   says "At least three stay on the ring." The ring shows three segments, none twice. Tick until
   ten. *Expect:* the rest can't be ticked. Reset goes back to the first seven.
4. **New items.** Pause tracking: the tray's item says Resume tracking, and the ring's segment shows
   a triangle, "Resume tracking", "Paused". All notes opens the pop-out's list; This week opens
   Chrono's Week; Settings opens Kairo's settings.
5. **Focus length.** Set Focus to 45. *Expect:* the segment says "Focus 45" and starts 45 minutes
   (only in a copy that isn't your own, as with every focus check).
6. **App settings.** Settings → App settings. *Expect:* Chrono and Ledger with Open (Ledger's opens
   on its Settings view), Command palette and App ring with Show, which brings their section into
   view.

## Ledger notes

On Windows, a Mac and Linux. Open Ledger's Notes.

1. **New.** Press + New note and type `# Essay plan`, Enter, `- [ ] intro`, Enter. *Expect:* the
   heading bold and blue with its `#` faint, and a second `- [ ] ` started for you. Enter again on
   the empty item. *Expect:* the list ends. The note is titled "Essay plan" in the list.
2. **Writing.** Type `**bold**`, `*italic*`, `` `code` ``, `[site](https://example.com)`. *Expect:*
   each styled, the caret always on the letter you're typing (no drift, also on long wrapped
   lines). Select a word and press Ctrl+B (⌘+B on a Mac). *Expect:* `**` around it; Ctrl+Z undoes it.
3. **Boxes.** Click inside `[ ]`. *Expect:* `[x]`, the line struck through; click again to untick.
4. **Saving.** Type, then close Ledger at once and reopen it. *Expect:* the text is all there.
5. **List.** Make two more notes, pin one. *Expect:* the pinned one on top with 📌, then the newest
   edited. Search for a word inside one note's text. *Expect:* only that note.
6. **Delete.** Delete a note. *Expect:* "Delete “…”? Tasks attached to it lose the link." with
   Delete and Keep; Keep leaves it, Delete removes it.
7. **Tasks.** On Tasks, open a task's panel: + Attach a note, pick "New note: <task>". *Expect:*
   Notes opens on a note headed with the task's name; the task's row shows 📝, and the note says
   "Attached to: <task> ↗", which goes back to the task.
8. **Palette.** `note buy milk` opens Ledger on a new note "buy milk". `notes` lists them; a word
   from inside a note's text offers it under Notes.
9. **Two computers.** With two combined computers, write a note on one. *Expect:* within about
   5 minutes (sooner, 30 seconds after typing stops) it's on the other. Edit the same note on both
   before either syncs. *Expect:* both end with the note and one "(conflicted copy)" of the edit
   that lost. Delete a note on one. *Expect:* it goes from the other too, and doesn't come back.

## A note popped out

On Windows, a Mac and Linux. No focus session involved. Open a note in Ledger's Notes.

1. **Pop out.** Press Pop out. *Expect:* a small window (about 360 × 480) titled with the note's
   title, its text in Ledger's editor, B / I / ☐ on the left, 📌 (pressed) and ↗ on the right,
   "Saved · N words" under it. It stays above other apps you click into.
2. **Both windows.** Type a line in the pop-out, then click into Ledger. *Expect:* the line is
   already there. Tick a box in Ledger, click back into the pop-out. *Expect:* ticked, the caret
   where it was. Rename the note's first line in either. *Expect:* the pop-out's title follows.
3. **Floating.** Press 📌. *Expect:* unpressed; other apps can cover the window. Close it and pop
   the note out again. *Expect:* still unpressed. Turn it back on.
4. **Another note.** Pop out a second note. *Expect:* the same window switches to it; no second one.
5. **Remembered.** Move and resize the window, close it, pop out again. *Expect:* where you left it.
6. **Apart from Kairo.** Close Kairo's window. *Expect:* the pop-out stays (on a Mac, Kairo stays in
   the Dock until it closes too). ↗ opens Ledger on the note.
7. **Deleted.** With a note popped out, delete it in Ledger. *Expect:* the pop-out goes to its list.
8. **All notes.** In Ledger's Notes, press Pop out beside + New note. *Expect:* the small window
   titled "Notes": a search box, + New, your notes (pinned ones with the ribbon). Pick one; ☰
   goes back to the list. + New starts an empty note, ready to type, its prompt faint.
9. **Without Ledger.** With Ledger closed, type `new note` in the palette, then `create note
   groceries`. *Expect:* each opens a new note in the small window (the second starting
   "groceries"); Ledger stays closed. The ring's New note does the same. In the palette, `notes`
   leads with "All notes in a small window"; on a note's row, Ctrl+Enter (⌘+Enter) pops it out.

## Ledger notes, looking finished

1. **None open.** In Notes with no note open. *Expect:* "Pick up where you left off" with the three
   notes edited last as cards, + New note and a tip under them; with no notes, the book and + New.
2. **Empty note.** Press + New note. *Expect:* a faint prompt ("# A title…") that goes as you type.
3. **Ruled lines.** Write a dozen lines and scroll. *Expect:* a faint rule under every line of text,
   staying under it as you scroll, at both window sizes and in both themes.
4. **Delete.** Press Delete. *Expect:* the question sits inside the sheet, clear of its edges.

## Palette: time charts on Enter

1. Type an app's name, then `<app> this week`. Point at the "<app> this week" row and move over
   the others. *Expect:* no chart, nothing moves. Press Enter (or click it). *Expect:* the chart,
   the row saying "Enter again to open this week in Chrono"; moving the pointer leaves it. Enter
   again opens the week in Chrono. Arrow to another row. *Expect:* the chart goes.
2. Type `yesterday` and `focus 25`. *Expect:* their cards show as the row is selected, as before.

## VirusTotal

On Windows, a Mac and Linux. Needs a free key from virustotal.com → profile → API key.

1. **Key.** Settings → VirusTotal: paste the key and press Test key. *Expect:* "✓ Works…". The field
   then shows "Saved…", never the key. A wrong key says VirusTotal didn't accept it.
2. **No key.** Remove it (clear the field), then type `scan` in the palette. *Expect:* the window says
   to add a key, with a button to Kairo's settings.
3. **A known file.** With the key back, drop any installer from Downloads on the scan window.
   *Expect:* "Checking…", then "● N of M engines flag it" and Full report ↗ opening virustotal.com.
4. **A hash.** In the palette, `vt 275a021bbfb6489e54d471899f7db9d1663fc695ec2fe2a2c4538aabf651fd0f`
   (the harmless EICAR test file). *Expect:* the window opens on it, red, with detection names.
5. **A link.** Paste `https://example.com` and Check. *Expect:* green, "0 of N".
6. **An unseen file.** Make a text file with a few random words and drop it. *Expect:* "VirusTotal
   hasn't seen this file…" with Upload and scan / Not now; nothing is sent until Upload. Upload it.
   *Expect:* "Scanning…", then a verdict within a minute or two.
7. **The limit.** Check five things quickly. *Expect:* "VirusTotal's free limit: try again in a
   minute" rather than a failure.

## Speed test

On Windows, a Mac and Linux. Uses up to about 100 MB of data.

1. **Run.** Type `speed test` in the palette. *Expect:* a Speed test window that starts at once:
   "Measuring ping…", then "Testing download…" with Download filling in and its box outlined,
   then upload, then "Done", ping and jitter in ms, "Cloudflare · " and a place code, and "Used
   about N MB" (at most 100).
2. **Again.** Run again is greyed while a test runs; once done it clears the figures and runs
   another. Typing `speed test` again while one runs doesn't start a second.
3. **Offline.** Turn off Wi-Fi and Run again. *Expect:* "Couldn't reach the speed test. Check your
   connection." Closing the window mid-test stops it.

## Palette: web search

On Windows, a Mac and Linux. Open the palette.

1. **Shortcuts.** Type `yt lofi beats` and press Enter. *Expect:* "Search YouTube for “lofi
   beats”" first, and YouTube's results open in your browser. Try `g`, `ddg`, `w`, `gh` and
   `maps` the same way.
2. **Fallback.** Type something nothing matches, like `zzqx tickets`. *Expect:* one row, "Search
   the web for “zzqx tickets”", which opens Google.
3. **Apps first.** With an app whose name starts like a shortcut (GitHub Desktop for `gh`),
   type just `gh`. *Expect:* the app, not a search.

## Palette: windows

On Windows, a Mac and Linux. Open a few windows first: a document, a browser, a folder.

1. **Find one.** Open the palette and type a word from a window's title. *Expect:* it shows
   under Windows, with its app's name and icon. Press Enter. *Expect:* that window comes to the
   front.
2. **Minimized.** Minimize that window, then find it again and press Enter. *Expect:* it comes
   back.
3. **By app.** Type an app's name, like "explorer" or "finder". *Expect:* the app first, its
   windows just under.
4. **All of them.** Type `windows`. *Expect:* your open windows, most recently used first, at
   most 8, and none of Kairo's own.
5. **Close.** Type something into a document without saving, find its window, press Ctrl+Enter
   (⌘+Enter on a Mac). *Expect:* the footer said "Close window", and the app asks whether to
   save rather than closing outright.
6. **Commands win.** With a window titled like a command, `focus 25` and `12*7` still lead with
   the focus session and the answer.

**Windows only:** from a terminal, `kairo.exe --list-windows` prints what the palette would
list, without touching any window.

**Mac only:** open an app's Settings window and close it, then type "settings". *Expect:* no
row for the closed window. Open two Finder windows on the same folder, pick the second's row
and press Enter. *Expect:* that one comes forward, not the first.

**Mac only:** turn Kairo off under Privacy & Security → Accessibility. *Expect:* Enter still
brings the window's app forward, and window rows have no "Close window".

**Linux, X11:** type `windows`. *Expect:* no panel or desktop among them.

**Linux:** on GNOME's Wayland session, the Windows section appears after the first log out and
back in following the update. On KDE Plasma's, it works straight away.

## Palette: calculator

On Windows, a Mac and Linux. Open the palette.

1. **Maths.** Type `12*7+3`. *Expect:* "87" first, under Calculator. Press Enter and paste
   somewhere. *Expect:* 87.
2. **Units.** Type `5 km in mi`. *Expect:* "3.10686 mi", with "5 km in mi" underneath. Ctrl+Enter
   (⌘+Enter on a Mac) copies 3.10686; the footer said "Copy number".
3. **Shorthand.** `30 c in f` gives 86 °F; `2 gb in mb` gives 2000 MB.
4. **Searches stay searches.** `focus 50`, `14 oct`, `2026-10-30` and `2026` show what they did
   before, with no Calculator row.

## Two computers combined

Use two machines (any systems) with the same cloud drive.

1. On each, Kairo's settings → Combine computers → Use a folder instead, and pick the **same** synced folder.
2. Wait 5–10 minutes for the drive to sync. *Expect:* each lists both computers, and Today on
   each adds up both.
3. **Showing switch.** In the sidebar, pick "This one". *Expect:* only that computer's time.
4. **Shared choices.** Change a site's category on one. *Expect:* within about 10 minutes, the
   other shows the same category.
5. **Use another folder** on one computer. *Expect:* its folder leaves the old location, and the
   other computer drops its time on its next sync.
6. **Stop combining** on one. *Expect:* each shows only its own time again.

## Linking with a code

Use two machines (any systems), both online, neither combining through a folder.

1. **Show a code.** On the first, Kairo's settings → Combine computers → Link a computer.
   *Expect:* a code like `K7QX-49MP` in a mono face, "Type this on your other computer. It works
   once, for 10 minutes." and a countdown.
2. **Type it.** On the second, Enter a code, type it in lowercase with a space instead of the
   dash, and Link. *Expect:* "Linking…", then "Linked with" the first computer's name; the
   first says "Linked with" the second's. Within a minute both list both computers, and Today
   on each adds up both. The sidebar's Show menu appears on both, reading All computers, and
   opening it lists This one and the other computer by name, without clicking away and back.
3. **Shared choices.** Change a site's category on one. *Expect:* the other takes it within
   about 5 minutes.
4. **A wrong code.** Link a computer on the first again, and on a third machine (or the second
   after Stop combining) type the code with its last character changed. *Expect:* "That code
   didn't match, or it's been used. Make a new one on the other computer." on the one typing,
   and "That code didn't match. Make a new one." on the one showing it. The right code no longer
   works either.
5. **Running out.** Show a code and wait 10 minutes. *Expect:* "The code ran out. Make a new
   one."
6. **Offline.** Turn the network off on one for a sync round (5 minutes). *Expect:* tracking
   carries on, and Settings says "Last synced … Can't reach Kairo's relay right now." until
   the network is back.
7. **Stop combining** on one. *Expect:* each shows only its own time after the other's next
   sync.
8. **Start over.** Link both again, then on the first, Start over → Start over. *Expect:* a new
   code shows at once; at its next sync the second says "This computer was unlinked. Enter a
   new code to combine again." and only its own time shows.

## Any system: backup and restore

1. Kairo's settings → Your data → Back up everything; save the file.
2. Restore it on another computer (even another system). *Expect:* a confirmation, a restart, and
   the history, categories, limits and goals of the backup, with this computer's palette shortcut
   kept.
