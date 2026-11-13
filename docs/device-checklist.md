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

1. **Hint.** Home. *Expect:* "App ring" in the line under the app chips, followed by its shortcut as
   keys. Clicking its name opens the ring.
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

1. **Pop out.** Press Pop out. *Expect:* a small window (about 400 × 540) titled with the note's
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
7. **Deleted.** With a note popped out, delete it in Ledger. *Expect:* the pop-out goes to its list
   at once, the note not in it (don't leave Notes first). Undo in Ledger. *Expect:* it's back in
   the pop-out's list. With All notes popped out, delete a note in Ledger. *Expect:* gone from the
   pop-out's list at once; `notes` in the palette doesn't find it either.
8. **Deleting in the pop-out.** In a popped-out note, press the bin at the bar's right (in ⋯ when
   the window is narrow). *Expect:* the list, "Deleted “…” · Undo" floating above its foot, the
   note gone from Ledger's Notes too. Undo. *Expect:* the note again, all of it, in both. In the
   list, hover a note and press its bin, or arrow to it and press Delete. Close the pop-out, then
   look in Ledger. *Expect:* the deleted notes stay gone. Delete one, quit Kairo from the tray,
   start it again. *Expect:* still gone.
9. **All notes.** In Ledger's Notes, press Pop out beside + New note. *Expect:* the small window
   titled "Notes": a search box, + New, your notes as cards with two lines of each under its title (pinned ones with the ribbon). Pick one; ☰
   goes back to the list. + New starts an empty note, ready to type, its prompt faint.
10. **Without Ledger.** With Ledger closed, type `new note` in the palette, then `create note
   groceries`. *Expect:* each opens a new note in the small window (the second starting
   "groceries"); Ledger stays closed. The ring's New note does the same. In the palette, `notes`
   leads with "All notes in a small window"; on a note's row, Ctrl+Enter (⌘+Enter) pops it out.

## Ledger, safer and keyboard-first

On Windows, a Mac and Linux. No focus session involved.

1. **Undo.** Delete a task from its panel. *Expect:* it's gone, "Deleted “…” · Undo" floating at the
   window's foot (the list doesn't jump) with the keyboard on Undo; Ctrl+Z (⌘Z) brings it back. Delete two in a row: the first is deleted
   for good. Delete a note: no question, the same Undo line. Leaving the view
   makes a waiting delete final; the keyboard lands on the next task or the search, never nowhere.
2. **Panel.** Scroll down a long list and open a task. *Expect:* the panel opens beside it, the card
   lit, the keyboard in its title; Esc puts it back on the task.
3. **Keys.** Ctrl+1/2/3/4/5 switch views (Tasks, Calendar, Notes, Done, Settings); ↑/↓ move along the tabs; Ctrl+N focuses Add a task (in Notes,
   starts a note); / jumps to the add field or the notes' search.
4. **Priority.** Ordinary tasks have no mark, High a filled amber dot, Low a faint ring.
5. **A failing save** (hard to cause by hand; skip unless the database is busy): the footer turns
   orange, Copy text appears, and opening another note says why it's staying.

## Ledger: the sidebar counts and the Done log

On Windows, a Mac and Linux, in both themes. No focus session involved: don't press Start.

1. **The counts follow what you do.** Note the Notes count. Press Ctrl+N in Notes. *Expect:* the
   count goes up by one at once, not at the next refresh. Delete that note. *Expect:* it goes down
   at once, while the Undo line is still showing, and back up if you press Undo.
2. **The Tasks count ignores Find.** With 5 open tasks, type a word only one has in Find a task.
   *Expect:* the sidebar still says 5.
3. **Done.** Tick two tasks off, then open Done (Ctrl+4). *Expect:* both under Today, newest first,
   each with the time it was done; "This week 2 done" above (with the focus time when a session
   was run on them), and last week beside it.
4. **Reopen.** Press Reopen on one. *Expect:* it leaves Done and is open again at the bottom of its
   group in Tasks; the Tasks count goes up by one.
5. **Delete and Undo.** Press Delete on a done task. *Expect:* it goes, the Undo line shows at the
   foot, Undo brings it back; leaving Done makes the delete final.
6. **Find, Show older, empty.** Type part of a title in Find. *Expect:* only those; nonsense says
   "No done task matches." With old done tasks, Show older adds four weeks at a time. With none
   done at all: "Nothing done yet. Tick a task off and it lands here."
7. **Open a task from Done.** Click a title. *Expect:* Tasks opens with that task's panel.
8. **Narrow window.** At the smallest window size, Find drops under the week line, rows don't
   overflow, and on a touch screen Reopen and Delete are always visible.

## Ledger: steps in a task

On Windows, a Mac and Linux. No focus session involved.

1. **Add steps.** Open a task. Under Remind me, type a step in "Add a step" and press Enter.
   *Expect:* it joins the list, the keyboard stays in the field, the title says 1 step and the task's
   row shows "0/1".
2. **Tick.** Tick it. *Expect:* its line is struck through and quieter, the row says "1/1", and the
   task can still be ticked off with steps left open.
3. **Paste a list.** Copy three lines from a note (some starting `- `, `1.` or `- [ ]`) and paste
   them in the field. *Expect:* three steps, bullets gone. Pasting one line just pastes text.
4. **Edit, move, remove.** Change a step's text and press Tab. *Expect:* kept. Clear a step's text
   and leave it: it goes. Alt+↑/↓ in a step moves it and the keyboard goes with it. Backspace in an
   empty step removes it and lands on the step above.
5. **Thirty.** With 30 steps the field is greyed and says "Up to 30 steps".
6. **Two computers.** On two combined computers, add steps on one. *Expect:* the other shows them
   within a sync round, ticks included; deleting the task removes them.

## Ledger: repeating tasks

On Windows, a Mac and Linux. No focus session involved.

1. **Quick add.** Type `water plants every day` in Add a task. *Expect:* under the field, "Tue 3 Nov ·
   Every day" (today's date), and the task has a repeat arrow on its row. Try `report every fri !h`:
   "High · Fri 6 Nov · Every week". `every weekday` on a weekend starts on Monday.
2. **Set it in the panel.** Open a task with no due day, choose Repeat → Every week. *Expect:* it
   becomes due today and the row shows the arrow. Choose Never: the arrow goes, the day stays.
3. **Tick it.** Tick a task that repeats weekly and is due today. *Expect:* it goes to Done and a new
   one with the same title, priority and reminder time appears due a week later; steps are
   unticked, an attached note is still attached.
4. **Early and late.** Tick one due next Friday today: the next is the Friday after that. Tick one
   that was due last Monday, today: the next is the coming Monday, never in the past.
5. **Undo it.** Untick the done one in Done (Reopen). *Expect:* the new one made by ticking is gone.
   Tick again, tick a step on the new one, then Reopen the old: the new one stays.
6. **Month ends.** A monthly task due on the 31st: ticked in January, the next is 28 Feb (29 in a
   leap year). Known limit: it then stays on the 28th.
7. **Two computers.** On two combined computers, tick the same repeating task on both before they
   sync. *Expect:* one new task, not two.
8. **Palette.** `task rent every month` adds it; Ctrl+Enter on it in `tasks` ticks it and adds the next.

## Ledger: tags, projects and the Calendar

On Windows, a Mac and Linux. Have a Chrono project (Chrono → Settings → Projects) and, to see
meetings, a calendar address connected.

1. **Tags.** Open a task. Under Priority, type `school, urgent` in "Add a tag" and press Enter.
   *Expect:* two chips, the keyboard still in the field, and `#school #urgent` on the task's row.
   Backspace in the empty field takes the last one back; × on a chip removes it.
2. **Quick add.** Type `essay #school fri` in Add a task. *Expect:* under the field, "Fri 13 Nov ·
   #school"; the task has the tag. `fix #42` keeps its `#42` in the title (a number isn't a tag).
3. **Projects.** In a task's Project choice, pick a Chrono project. *Expect:* its name on the row.
   Choose "New project…", type a name, Enter. *Expect:* the task is in it, and the project now
   exists in Chrono (Settings → Projects) with the name as its keyword.
4. **Rename in Chrono.** Rename that project in Chrono. *Expect:* the task in Ledger shows the new
   name. Delete the project: the task shows no project (the name is kept, and comes back if a
   project of that name is made again).
5. **Narrowing.** With a few tasks tagged, press a `#tag` chip above the list. *Expect:* only those
   tasks, the chip pressed, a line under the title like "Kairo #school · 3 open · 2h 10m focused",
   and Show all to let go. A project and a tag together narrow to both. The sidebar count stays.
6. **Calendar.** Open Calendar (Ctrl+2). *Expect:* this month, today ringed, each day with its due
   tasks (two, then "+n more"), overdue days with an amber mark, meetings as "2 meetings" for the
   two weeks either side of today. Arrow keys move the day, Page Up/Down the month, Today comes back.
7. **A day.** Pick a day. *Expect:* its tasks and meetings listed beside the month (below it on a
   narrow window); tick one off, click a title to open it in Tasks. Type `call Sam !h` in "Add a
   task for this day" and press Enter: due that day.
8. **Filter in Calendar.** Press a tag chip. *Expect:* only those tasks on the grid. "n open tasks
   have no due day" says what no cell can show.
9. **Two computers.** Tag a task on one. *Expect:* the other shows the tags and project after a sync.

## Ledger: calmer rows and a slimmer panel

On Windows, a Mac and Linux, in a dark and a light theme, and once at the smallest window (720 × 520).
None of this has been looked at on a real screen yet, so it is the first look.

1. **Rows.** Open Tasks with a few tasks that have a project, tags, steps, a repeat and a reminder.
   *Expect:* the marks on each row are plain quiet text, not pills; the due day is the one in
   stronger ink (warm once overdue); only the first tag shows, with "+2" for the rest (hover shows all);
   a repeating task says "weekly" (or daily, monthly…) beside its arrow; the "25m" button has no edge
   until the row is pointed at or the keyboard is on it (always edged on a touch screen).
2. **Scanning.** Without reading, find what's due soonest in each group. *Expect:* easy; nothing on a
   row competes with the title.
3. **The panel.** Open a plain task. *Expect:* only the title, Mark done, Focus, Due, Priority, an
   "Add…" choice and, at the foot, focus time and Delete. Open a task with a tag, a project and steps:
   those show as fields.
4. **Add….** Choose Repeat, Project, Tags, Steps and Note in turn. *Expect:* each opens its field with
   the keyboard in it and is no longer offered; Reminder makes one at 9 on the due day at once.
5. **Delete stays in view.** Open the fullest task at the smallest window and scroll the panel.
   *Expect:* the foot with Delete stays at the bottom of the panel the whole time.
6. **Filters.** *Expect:* "Project" and "Tag" label their groups; a pressed chip is filled in the
   amber accent, with Show all beside the chips.
7. **Done.** *Expect:* each row ends "done 4:00 PM", with "1h 35m focused ·" before it when there was focus time.
8. **Header.** *Expect:* no small line above "Tasks"; the date starts the summary under the title
   ("Thursday 12 November · 3 due today…").
9. **Contrast and size.** Group headers, panel labels and calendar weekday names are at least 11px;
   the sidebar count and a selected row's day are readable in both themes.
10. **Putting a field away.** Open a plain task, choose Steps from "Add…" and leave it empty.
    *Expect:* a small ✕ beside the field; clicking it hides the field, and Steps is offered in "Add…"
    again. Type a step: the ✕ is gone (the step's own controls are the way out).

## Home: the day first

On Windows, a Mac and Linux, in a dark and a light theme, and once at the smallest window (720 × 520).
None of this has been looked at in the real app yet, so it is the first look. Use a copy that isn't
your own for the focus checks.

1. **Title.** Open Home with two tasks due. *Expect:* the title says "Two things left today."; with
   one, "One thing left today."; with none, "Nothing due today." (and "Add a task" under it). Go to
   Settings and back: the title returns at once, with no flash of another one.
2. **Tasks.** *Expect:* the due tasks as plain rows (a round tick, the name, "Overdue" in warm text,
   and a quiet "Focus" button), not pills. The tick finishes a task; the name opens it in Ledger.
3. **Focus strip.** Press Focus on a task. *Expect:* a strip above the tasks: "Focusing on <task> · 25
   min left" and Stop; the rows lose their Focus buttons. Pause it from Chrono: the strip says
   "paused". Stop ends it and the strip goes away. With none running, no strip at all.
4. **Apps.** *Expect:* Chrono and Ledger as two small chips, each with a coloured dot and today's line
   ("2h 05m today", "1 due today"); the whole chip opens the app. Paused tracking shows "Paused ·
   Resume" in Chrono's chip, and Resume works.
5. **Hints.** *Expect:* under the chips, "Command palette" with its keys and "Try “…”", and "App
   ring" with its keys. Each name opens that app.
6. **No stray ring.** Open the dashboard with the mouse. *Expect:* no outline on the first task or
   chip. Press Tab: the ring appears on the focused control. Open it by shortcut and press an arrow
   or Tab: same.
7. **Smallest window.** *Expect:* nothing is cut off or scrolls sideways; the chips and hints wrap.

## Ledger: reminders at a moment

On Windows, a Mac and Linux. None of this has been tried on a real machine yet. Needs notifications
allowed for Kairo, and Ledger's reminders on (Ledger → Settings). Use a copy that isn't your own for
the focus checks.

1. **Add one.** Open a task, choose "Reminder when…" from "Add…". *Expect:* a field "Remind when"
   with "I open…" and a box for words; the keyboard is in it; "A focus session ends" is the other
   choice. With no words typed, leaving the box saves nothing and the field can be put away with ✕.
2. **An app.** Type `notepad` (or any app you have) and press Enter, then open that app from another
   window. *Expect:* one notification "Reminder: <task>" saying "You opened <app>", within a second or
   two, not a minute. Switching away and back does not repeat it.
3. **A site.** Set the words to a site's name (say `youtube`), open it in the browser. *Expect:* the
   same, saying "You opened youtube.com". On a Mac and Linux this needs the browser extension (or, on
   a Mac, Chrome, Safari or Arc's usual tab reading) as Chrono's sites do.
4. **Several words.** Type `figma, notion`. *Expect:* either app brings the reminder.
5. **A focus session.** Choose "A focus session ends", start a focus session of a minute from
   somewhere else, and let it finish. *Expect:* "Your focus session ended" arrives with the session's
   own notice. Ending a session early (Stop) brings it too.
6. **Again.** Change the words or set the moment again. *Expect:* it reminds again; otherwise once.
7. **Privacy.** Make the app one Chrono ignores (Settings → Categories and limits → Ignore). *Expect:*
   opening it brings no reminder. Hiding its title instead: words found only in the title don't
   match, words in the app's or site's name do. A private or incognito window never matches a site.
8. **Games and away.** *Expect:* no reminder while a game is in front, or while you're away from the
   computer; one comes when you're back and open the app again.
9. **Quiet.** Switch Ledger's reminders off. *Expect:* no moment reminders either.
10. **A repeating task.** Give a repeating task a moment, tick it. *Expect:* the next one has the same
    moment and reminds again.
11. **Two computers.** Set a moment on one. *Expect:* the other shows it after a sync, and only one of
    them reminds when both are in use (the other's mark arrives with the sync).

## Windows: Alt+Space as the shortcut

1. In Kairo's settings, press Change beside the palette's shortcut and press Alt+Space. *Expect:*
   Windows' window menu (Restore, Move, Minimize…) does **not** open, the keys show Alt Space, and
   it saves. Pressing Alt+Space afterwards, anywhere, opens the palette.
2. Press Cancel, or Esc, and then Alt+Space in Kairo's own window. *Expect:* the window menu opens
   as it always did (the hook is on only while recording).
3. If another program already uses Alt+Space (PowerToys Run, Flow Launcher, Raycast…), saving says
   so and the old shortcut stays.

## Ledger and dashboard, second critique round

On Windows, a Mac and Linux, in both themes. No focus session involved: don't press Start.

1. **Ticking keeps your place.** Tick a task with Space, wait for the 30-second refresh. *Expect:*
   the keyboard stays on the same row, never jumping to the top.
2. **Overdue.** A task due yesterday: a short red ribbon over its tick, the "Overdue" heading in
   red, its date chip plain grey.
3. **Start.** Hover a task. *Expect:* "▶ 25m" (your ring's length), not just ▶.
4. **Shorthand and keys.** Under Add a task, open "Shorthand and keys". *Expect:* a small two-column
   sheet. Type `Essay tomorrow !h`: the preview isn't read aloud at every key.
5. **All done.** Tick every task. *Expect:* "All done. Nothing left open." above the Done fold.
6. **Esc.** Open a task, open its due-day calendar, press Esc. *Expect:* only the calendar closes;
   Esc again closes the panel.
7. **Light theme notes.** Click into a note's sheet. *Expect:* its outline clearly visible.
8. **Dashboard opens with the keyboard on Chrono's tile.** Tab moves along the tiles.
9. **Palette tile** reads "Suggestions on" (or off), with "· over full-screen apps" when set.
10. **Settings.** Grouped under Your apps, Kairo, Shortcuts, Data and computers; type in Find a
    setting to filter. Flip Suggestions: "Saved" appears beside it for a moment. Change the
    palette shortcut and press Ctrl+Tab: it says Tab can't be used. Set the ring's Focus length to
    2: "Between 5 and 240 minutes" beside the field, the old value back.
11. **Checkboxes** (ring editor, switches) show an outline in the app's colour under the keyboard.

## Ledger and dashboard, fourth critique round

On Windows, a Mac and Linux, in both themes. Steps 1 and 2 start a focus session that blocks apps:
do them only on a test copy, never on Kairo you're working in.

1. **Stop by keyboard** (test copy). Start a focus from a task, then with the keyboard: Tab to the
   list, → on the running task. *Expect:* Stop has the keyboard; Enter stops it. With strict mode
   on, Stop says "Strict mode: Stop works again in 30 s", counting down, then "press Stop again".
2. **The panel** (test copy). Open a task. *Expect:* "Mark done" and "▶ Focus 25 min" under the
   title, "Blocks distracting apps" under them; while it runs, "Stop focusing". No × beside "No due
   day". The panel opens level with its card.
3. **Start's words.** Hover ▶ 25m. *Expect:* "Focus 25 min: blocks distracting apps".
4. **Ledger's top.** Find a task sits beside the Tasks title (Tasks only). The hint under Add a
   task shows only while adding; "Shorthand and keys" is under the list.
5. **Notes.** Delete on a note in the list deletes it (Undo brings it back). The buttons read "All
   notes in a window" and "This note in a window".
6. **Home.** With a task due: the keyboard starts on it; a round tick beside it ticks it off. At
   18:00 with something overdue, the greeting says "Good evening, 1 overdue." The palette tile
   never suggests "focus 25".
7. **Settings.** Your apps lists Chrono and Ledger only. Type "theme" in Find and press Enter: the
   theme dropdown gets the keyboard. Sections sit closer together.
8. **Ring.** Reorder it, press Reset. *Expect:* "Reset to Kairo's order · Undo"; Undo puts yours
   back. While the ring editor has the keyboard, changing a setting from the palette doesn't take
   the keyboard out of it.
9. **Updates.** Press Check for updates. *Expect:* "Checking…", then the answer beside the button
   as well as in a notification.

## Ledger and dashboard, third critique round

On Windows, a Mac and Linux, in both themes. No focus session involved: don't press Start.

1. **Several deletes.** In Tasks, press Delete on three tasks in a row with the keyboard. *Expect:*
   the keyboard stays on the list each time, the line says "Deleted “…” and 2 more · Undo" and
   fades after about 8 seconds; Ctrl+Z (⌘Z) brings them back one at a time, latest first.
2. **Start shows.** Every open task shows "▶ 25m" in grey, Ledger's colour under the pointer. No
   Rename bubble on task cards; F2 renames.
3. **Reminders.** A task with none shows "+ Add a reminder"; pressing it sets 09:00 on its due
   day (or the next 9 o'clock), with the day, time and × to clear.
4. **Find a task.** Press / in Tasks: the keyboard lands in Find a task; typing narrows the list,
   "No tasks match…" when nothing does, Esc clears it.
5. **Notes.** The list is one Tab stop: ↑/↓ move, F2 renames. The note's toolbar: writing tools,
   then Pop out, Pin and, set apart, Delete. The welcome tips are three short lines.
6. **Ledger Settings.** Flip "Remind me of tasks": "Saved" shows for a moment.
7. **Home's Today.** Tasks due today or overdue, each opening that task in Ledger; "Nothing due
   today. Add a task" with none; the next reminder still to come today under them.
8. **Tiles.** Clicking anywhere on a tile opens its app. Pause tracking from the tray: Chrono's tile
   says "Paused · Resume", and Resume resumes it. The palette tile suggests one thing to try.
9. **Tabs.** Ctrl+1 Home, Ctrl+2 or Ctrl+, Settings; ← → on the tabs.
10. **Settings.** Combine computers shows its two ways and a folded "What's shared". Stop combining
    asks in words, with "Keep combining" focused. Groups: Your apps, Kairo, Palette and ring, Data
    and computers. Changing the theme says "Saved".
11. **Restore** (on a test copy only, with a backup of real data first): the warning names Ledger's
    tasks and notes, and the notes come back from the backup.

## Ledger notes, looking finished

1. **None open.** In Notes with no note open. *Expect:* "Pick up where you left off" with the three
   notes edited last as cards, + New note and a tip under them; with no notes, the book and + New.
2. **Empty note.** Press + New note. *Expect:* a faint prompt ("# A title…") that goes as you type.
3. **Ruled lines.** Write a dozen lines and scroll. *Expect:* a faint rule under every line of text,
   staying under it as you scroll, at both window sizes and in both themes.
4. **Delete.** Press Delete. *Expect:* no question; the note goes, with Undo at the window's foot.

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
8. **Unlink every computer.** Link both again, then on the first, Unlink every computer → Unlink every computer. *Expect:* a new
   code shows at once; at its next sync the second says "This computer was unlinked. Enter a
   new code to combine again." and only its own time shows.

## Any system: backup and restore

1. Kairo's settings → Your data → Back up everything; save the file.
2. Restore it on another computer (even another system). *Expect:* a confirmation, a restart, and
   the history, categories, limits and goals of the backup, with this computer's palette shortcut
   kept.

## Any system: pomodoro cycles

1. **A cycle.** On the Focus page, tick Pomodoro, pick 15m and press Start focus. *Expect:* the
   timer's heading says "Focus · in progress · round 1 of 4". Open an app on the block list during
   the round. *Expect:* it is blocked as in any session.
2. **A break.** Let the round run out (or use a test copy with a short round). *Expect:* a
   notification "Round 1 done", and a "Break · 5m left" card on the Focus page with Skip break and
   End pomodoro. The app from step 1 now opens without being blocked.
3. **The next round.** Let the break run out with you at the computer. *Expect:* "Break over.
   Round 2 of 4." and blocking is back on. Leave the computer for 3 minutes before a break ends.
   *Expect:* the card says "Break over. The next round starts when you're back." and the round
   starts at your first input.
4. **Skip and end.** Press Skip break on a break. *Expect:* round 2 starts at once. End early
   during a round, or press End pomodoro on a break. *Expect:* the cycle is gone and the wrap-up
   shows a Pomodoro part with the rounds, focus time, breaks and nudges.
5. **The palette.** Type `pomodoro 50 10 essay`. *Expect:* "Start a pomodoro" with "50m rounds,
   10m breaks · Labelled “essay”". On a break, `skip` offers "Skip the break" and `end focus`
   offers "End the pomodoro".

## Any system: clipboard history

1. **Copies appear.** Copy three different pieces of text in other programs, then type `clip` in
   the palette. *Expect:* the three, newest first, each with how long ago and its size, then
   "Clear clipboard history". Copy five more. *Expect:* `clip` shows the latest five, then "Show
   all 8 copies". Press Enter on that row, or type `clip` and a space. *Expect:* a Clipboard
   bubble with all eight; typing part of one narrows them; Backspace on an empty input leaves.
2. **Put one back.** Press Enter on the oldest. *Expect:* the palette closes, and pasting gives
   that text. Open `clip` again. *Expect:* it is now at the top, not repeated.
3. **Not in ordinary searches.** Copy a word, then type that word in the palette without `clip`.
   *Expect:* no "Clipboard" rows.
4. **Clear and off.** Choose "Clear clipboard history". *Expect:* `clip` says "Nothing copied
   since Kairo started", and the text still on the clipboard doesn't come back. In Kairo's
   settings (the dashboard), untick Remember what I copy, copy something, and type `clip`. *Expect:* "Clipboard
   history is off". Tick it again. *Expect:* what was copied while it was off is not listed.
5. **Gone on quit.** Copy something, quit Kairo from the tray, start it and type `clip`. *Expect:*
   "Nothing copied since Kairo started".
6. **Linux (Wayland).** Copy text and type `clip`. *Expect:* either the copy, or the empty note if
   the desktop refuses clipboard reads from a background program.

## Any system: system commands

Do these on a machine with nothing unsaved. Don't run them in a test copy on a machine that is
doing something else.

1. **Lock.** Type `lock` and press Enter. *Expect:* the screen locks at once; signing back in finds
   Kairo still running.
2. **Sleep.** Type `sleep` and press Enter. *Expect:* the computer sleeps (a Windows laptop with
   hibernate on still sleeps, it doesn't hibernate) and wakes with a key.
3. **Asking twice.** Type `shut down` and press Enter. *Expect:* nothing shuts down; the row now
   says "Confirm: shut down" in the warm colour with Cancel under it. Press Esc, open the palette
   and type `shut down` again. *Expect:* it's a plain "Shut down" row again. Press Enter, then type
   `x`. *Expect:* the confirmation is gone. Do the same for `restart` and `log out`.
4. **Doing it.** Type `restart`, Enter, Enter. *Expect:* the computer restarts; with a program that
   has unsaved work open, Windows or macOS asks about it first and doesn't force it. Try `log out`
   the same way. On a Mac, the first time: macOS asks to let Kairo control System Events; allow it.
5. **Kill.** Open a browser with several windows and type `kill ` and part of its name. *Expect:*
   its row with how many processes it has; Enter ends them all. Try a program that runs as
   administrator. *Expect:* "Couldn't end <name>. It may be running as administrator."
6. **Never listed.** Type `kill kairo`, `kill system` and `kill webview`. *Expect:* none of them
   offers Kairo, the web view or a system process.
7. **Two letters.** Type `re`, `sh`, `lo` and `sl`. *Expect:* no system rows; three letters of a
   word (like `res` or `shu`) offer them below the apps.
8. **Linux.** `lock`, `sleep` and a shut-down confirmation on a desktop with systemd. *Expect:* as
   above; on a desktop without `loginctl`, the lock row says it couldn't.
