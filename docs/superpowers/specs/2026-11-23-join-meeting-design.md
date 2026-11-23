# Joining the next meeting

Built in 1.34.0, after the owner agreed to it as the next feature. Kairo already read the
calendars and said "Next meeting: … in 20m"; this opens the call.

## What

- `join` (or `join meeting`, `join call`, or loosely "zoom", "meet", "teams") shows the meeting on
  now, or else the next one: "Join “Standup”", "Starts in 3 min · Google Meet". Enter opens the link.
- From 5 minutes before a meeting starts until 10 minutes in, and only when it has a link, that row
  leads the empty palette under Suggested.
- With no link: "“Standup” has no video link", with when it is. With no meeting: "No meetings
  coming up on your calendars".
- A usual focus session (usual.rs) that the meeting would cut short isn't suggested.

## Where the link comes from

Read with the calendar, every half hour, from the event's `X-GOOGLE-CONFERENCE`, `CONFERENCE`,
`URL`, `LOCATION`, `DESCRIPTION` and `X-ALT-DESC`, in that order: the first http(s) address whose
host is a known call service (Zoom, Google Meet, Microsoft Teams, Webex, Whereby, Jitsi, Amazon
Chime, GoTo Meeting, Around), a subdomain counting (`us02web.zoom.us`) but not a look-alike
(`evilzoom.us`, `zoom.us.example.com`). Kept in `calendar_events.link`; cleared with the title
when old meeting titles are blanked.

## Not in this

Declined events: a calendar's secret address doesn't say which attendee is you, so Kairo can't
tell. Phone dial-ins. Joining in the app rather than the browser (the link does that itself for
Zoom and Teams when installed).
