# Calendar, Mail and Reminders

How the “Calendar”, “Mail” and “Reminders” tiles are connected to Apple's apps, when they refresh and what that costs.

## What it is

The three tiles do not fetch their data themselves but through a **digest skill** (`calendar-digest`,
`mail-digest`, `reminders-digest`), which reads through the MCP servers `apple-reminders` and `apple-mail` and hands
back the result as an overview. Each tile runs once when it appears (app start or freshly placed), and after that
only on ↻ — **every refresh is a real agent turn** and costs money on a paid provider. Creating, checking off and
deleting go to the agent as a silent one-shot instruction. Repeating calendar events are created by the app directly
through macOS, because the tool cannot do recurrence.

## Where the buttons are

- **All three, in the header:** when it last ran (“never run” or a relative time) and ↻ (“Refresh”). Flip side:
  “Skill name” — which skill feeds the tile.
- **Calendar:** filter “All calendars”, “+” (“New event”), mini month with dots, agenda with ✕ per event.
  Form: title, calendar, date, “All day”, times, “Location (optional)…”, “Repeat” (“Does not repeat”,
  “Daily”, “Weekly”, “Monthly”, “Yearly”; then “Every …” and “until …”), “Create”. Flip side: “Days in the agenda”
  (1–14), “Clock next to the calendar” with “Digital” or “Day arc”.
- **Mail** (read only): “curated / last 3 days”, the bar “Today's mix”, the sections “Important” and “By
  topic”; a click opens the summary as a note in `Mail/`. Flip side: “Topics (one per line)” → “Save” —
  stored in the vault file `Mail/.topics.md`.
- **Reminders:** list choice, “+” (“New reminder”), per task a checkmark (done), ✕ and notes. Form: title,
  list, due date, “Notes (optional)…”, “Create”.

## Common tasks

1. **Refresh:** ↻ in the tile's header.
2. **Create an event:** Calendar → “+” → form → “Create” (with “Repeat” for a series).
3. **Refresh regularly:** set up the digest skill as a [routine](routinen.md), for example every morning.

## Traps

- No refresh in the background: what you change outside the app only appears after the next ↻ or
  routine run.
- Calendar only loads this month and next; the tile does not page further.
- Reminders has no “all lists” view; the chosen list is remembered.
- Once the daily limit is reached, a refresh fails too; the tile then shows the last usable state with
  a notice.
- On first access macOS asks for the right to Calendar, Reminders or Mail; after a new build of the
  installed app it may ask again.

## See also

[Skills](skills.md) · [Routines](routinen.md) · [Costs](kosten.md) · [Providers](provider.md)
