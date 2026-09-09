---
name: calendar-digest
description: Reads upcoming calendar events from whichever calendar tool is available (Apple Calendar via the apple-reminders MCP server today) and reports them as one JSON object for the Calendar dashboard module to read back from this skill's last run.
backend: claude-code
allowed_tools: mcp__apple-reminders__calendar_calendars mcp__apple-reminders__calendar_events
timeout_secs: 600
---

# Calendar Digest

Report the user's upcoming calendar events as a single JSON object — this is
read by a dashboard module afterwards, not by a person, so the output format
below must be followed exactly.

Do exactly this and nothing more:

1. List the available calendars using whichever calendar tool is available
   right now (the `apple-reminders` MCP server's `calendar_calendars` tool).
   Collect every calendar's display name, even ones with no events coming up.
2. List events from the **first day of the current month** through the
   **last day of next month**, across every calendar found in step 1, using
   that same tool's `calendar_events`. (This wider window lets the dashboard
   module page the mini-month and slice out any 7-day agenda without a fresh
   run — it filters client-side.) For each event capture:
   - `id` — the event's own identifier, exactly as the tool returns it
   - `title` — the event's title
   - `start` — ISO 8601 timestamp (date only, `YYYY-MM-DD`, for an all-day event)
   - `end` — ISO 8601 timestamp (same date-only rule for all-day events)
   - `calendar` — the calendar's display name; must match one of the names
     from step 1 exactly
   - `location` — the event's location, or `null` if it has none
   - `allDay` — `true`/`false`
3. Sort events by `start`, ascending.
4. Reply with **exactly one JSON object and nothing else** — no markdown code
   fence, no explanation before or after it, no trailing text:

   `{"calendars": ["Name A", "Name B"], "events": [{"id": "...", "title": "...", "start": "...", "end": "...", "calendar": "...", "location": null, "allDay": false}]}`

If no calendar tool is available at all, reply with exactly this and nothing
else: `{"calendars": [], "events": [], "error": "no calendar tool available"}`
