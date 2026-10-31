# Routines and their schedule

How to have a skill or a prompt run on a schedule, how the interval is set and what happens when the app is closed.

## What it is

A **routine** starts a skill or a free prompt at fixed times — for example the mail digest every morning. It runs
on the harness for skills from the settings unless you give it a backend of its own. Every run is an agent turn like
a skill started by hand and counts against the daily limit.

## Where the buttons are

- **Front of the “Routines” tile:** table “Time | Routine | Status”. “Time” shows how long until the next run;
  below the name stands the target (`/skill` or “prompt”). “Status” is “next” (the next one, with a burning fuse),
  “queued” or “off”. Per row: ✎ (“Edit”), ✕ (“Delete”) and the switch “on”/“off”.
- **Back (gear):** “Show disabled routines”, “Sort” (“soonest first” / “by name”) and the form “Add
  routine”.
- **The form:** name; interval “every N minutes”, “every N hours”, “daily”, “weekly” or “custom (cron)”; a preview
  such as “daily at 09:00”; target “skill” (skill name) or “prompt” (text); backend (“default (skill harness)” or
  one of its own); “Add”.
- ⌘K finds routines (Enter shows the tile). When the tile is open, the chat can also list, create and change
  routines.

## Common tasks

1. **Create a routine:** flip the tile → “Add routine” → name, interval, target → “Add”.
2. **Pause:** on the front set the switch to “off”.
3. **Change the time:** ✎ → adjust the interval → “Save”.

## Traps

- **Only while the app runs.** The app fires routines itself; when it is closed, nothing runs. At the next start a
  missed time is noted as “Missed” and moved to the next one — **not** made up.
- ✕ deletes the routine **at once**, without asking.
- “custom (cron)” has **six fields, seconds first**: `0 0 9 * * *` is daily at 09:00 — not the usual five-field
  crontab format.
- A routine on a digest does not replace the tile's ↻, it adds to it: the tile always shows the newest usable run,
  no matter who started it.

## See also

[Skills](skills.md) · [Connections](verbindungen.md) · [Costs](kosten.md)
