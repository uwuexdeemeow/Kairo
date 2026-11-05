# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Desktop app built with Tauri: the windows are web pages in the system's web view (WebView2 on
Windows, WKWebView on macOS, WebKitGTK on Linux). The command palette and the app ring on Windows
are drawn natively (Direct2D/DirectWrite), not in a web view.

## Users

For now, one person: the author, using Kairo every day on their own computers. Others may come
later, but decisions are made for a single daily user who knows the product well.

## Product Purpose

Kairo (from *kairos*, the opportune moment) is a desktop productivity hub. Its heart is the command
palette, which opens apps, answers questions about your time and runs Kairo's actions. Around it sit
its apps: **Chrono**, a screen time tracker that shows when you focus best and where time slips
away; **Ledger**, tasks and notes; and the **dashboard**, Kairo's home and settings. The **app
ring**, opened by its own shortcut, jumps between them. Success is a tool that is quick to reach,
honest about time, and quiet when not needed.

## Positioning

Everything stays on the user's device, unless they combine computers, and then it travels
encrypted end to end (or through a cloud drive they choose). Kairo reads time from the computer
itself (foreground window, idle, the browser's address bar) rather than asking the user to log it,
and turns it into insights only when there's enough data behind them.

## Operating Context

- Runs in the tray or menu bar all day; windows are opened briefly and put away.
- Reached by shortcuts (palette, ring), the tray, launchers and the Start menu.
- Used alongside games and full-screen apps, which it must not interrupt.
- Windows is the main platform; macOS and Linux are previews.
- Releases ship from tags through GitHub Actions; installed copies update themselves.

## Capabilities and Constraints

- Windows: dashboard (`kairo`), Chrono (`index.html`), Ledger, palette, app ring, blocked page.
- Minimum window size 720 × 520; default 1120 × 780.
- Eight themes, five dark and three light, or following the system (Ink or Paper), chosen in Kairo's settings.
- The command palette opens full (suggestions, today so far) or compact (the search bar alone until you type).
- No editor or UI framework: plain TypeScript and one stylesheet (`src/styles.css`).
- The Windows palette and ring are native and must keep matching their web twins.

## Brand Commitments

- Names: Kairo, Chrono, Ledger, command palette, app ring.
- Each app has its own accent colour and logo (`src/logos.ts`).
- Voice: plain, short, everyday English ("3 due today", "Back to work"), no jargon.

## Evidence on Hand

- Real usage data lives only on the user's device; demo data can be loaded from Categories.
- No testimonials, users or metrics exist beyond the author's own use; do not invent any.

## Product Principles

1. Quick to reach, quick to leave: every window should open, answer and get out of the way.
2. Honest about time: show only what the data supports.
3. Private by default: nothing leaves the device unless the user chooses it.
4. Never in the way of play: games and full-screen apps come first.

## Accessibility & Inclusion

Hold to WCAG 2.2 AA: text contrast 4.5:1 (3:1 for large text), focus indicators and control
boundaries 3:1, full keyboard use, and reduced motion respected with an intentional alternative.
