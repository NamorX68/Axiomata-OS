---
name: reminders-digest
description: Reads Apple Reminders lists and open tasks via whichever reminders tool is available (the apple-reminders MCP server today) and reports them as one JSON object for the Reminders dashboard module to read back from this skill's last run.
backend: opencode
allowed_tools: mcp__apple-reminders__reminders_lists mcp__apple-reminders__reminders_tasks
timeout_secs: 600
---

# Reminders Digest

Your entire reply must be **exactly one JSON object** — this is parsed by
software, not read by a person. Output nothing before or after it: no
introduction, no summary, no bullet points, no markdown code fence, no second
copy of the object. If you cannot finish a step, output what you did collect
and set `"error"` to a short reason — an object containing only `"error"` is
better than an empty reply.

Do exactly this and nothing more:

1. List every reminder list using whichever reminders tool is available right
   now (the `apple-reminders` MCP server's `reminders_lists` tool, action
   `read`). Collect every list's name, even ones with no open tasks. One call
   is enough — never re-fetch.
2. Read every open (incomplete) reminder using that same tool's
   `reminders_tasks` (action `read`) — leave `showCompleted` at its default
   (`false`) so only open tasks come back, and leave `filterList` unset so you
   get every list's tasks in one call. For each task capture:
   - `id` — the reminder's own identifier, exactly as the tool returns it
   - `title` — the reminder's title
   - `list` — the list it belongs to; must match one of the names from step 1
   - `notes` — its notes (or a URL if that's what it carries), or `null`
   - `dueDate` — ISO 8601 (`YYYY-MM-DD` if it's a date with no time), or
     `null` if it has none
   - `priority` — `"none"`, `"low"`, `"medium"`, or `"high"` (normalise
     whatever the tool returns — a number or a word — onto exactly one of
     these four)
3. Sort tasks by `list`, then by `dueDate` ascending (tasks with no due date
   sort last within their list), then by `title`.
4. Reply with exactly one JSON object of this shape, and nothing else:

   `{"lists": ["Name A", "Name B"], "tasks": [{"id": "...", "title": "...", "list": "...", "notes": null, "dueDate": null, "priority": "none"}]}`

If the reminders tool is not available at all, reply with exactly this and
nothing else:
`{"lists": [], "tasks": [], "error": "no reminders tool available"}`
