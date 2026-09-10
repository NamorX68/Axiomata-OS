---
name: mail-digest
description: Reads recent mail via whichever mail tool is available (the apple-mail MCP server today), picks out messages the agent judges important plus ones matching the owner's configured topics, summarises each, and reports them as one JSON object for the Mail dashboard module to read back from this skill's last run.
backend: claude-code
local_backend: ollama-agent
prepend_files: ["Mail/.topics.md"]
allowed_tools: mcp__apple-mail__get_needs_response mcp__apple-mail__search_emails mcp__apple-mail__list_inbox_emails
timeout_secs: 600
---

# Mail Digest

Report a short, curated list of noteworthy emails as a single JSON object —
this is read by a dashboard module afterwards, not by a person, so the
output format below must be followed exactly. This is deliberately **not** a
full inbox listing: only messages that are either clearly important or match
one of the owner's configured topics belong in the output.

Keep the run tight — every tool call is a slow round-trip against Mail.app.
Make **at most ~6 tool calls total**: one inbox listing, one
needs-response pull, and one `search_emails` per *topic group* (not per
keyword). Don't re-fetch a message you already have.

Do exactly this and nothing more:

1. Configured topics, if any, appear at the very top of this message under
   "Context file: Mail/.topics.md" — one topic per line. If that block is
   absent, there are no configured topics: skip the per-topic `search_emails`
   calls and the topic classification in step 4. Never try to open the file
   yourself.
2. Build the candidate pool for the **last 2 days**, using the mail tool
   available now (the `apple-mail` MCP server):
   - `list_inbox_emails` once, for subject/sender/date of recent inbox mail.
   - `get_needs_response` once — a precise importance signal.
   - For the configured topics, run **one `search_emails` per topic**, using
     the topic plus its obvious near-synonyms in a single query (e.g.
     "KI/AI/LLM" → one search for `KI OR AI OR LLM`). This is what surfaces
     topic-relevant newsletters and mailing-list mail that a plain inbox
     listing buries — don't skip it, but don't fan it out into many calls
     either.
3. Classify every candidate yourself, by **judgement, not string matching**
   ("Fotografie" matches an email about camera gear or a photo meetup even
   without the word itself):
   - **Important** — needs a response, asks a direct question, names a
     deadline, is an invoice/receipt/bill, or is an invitation/RSVP.
     Automated notifications, newsletters and no-reply marketing are **not**
     important (an invoice/receipt/bill still is, even from a no-reply
     sender).
   - **Topic** — matches the substance of a configured topic. A newsletter
     or mailing-list email *can* be a topic match (the important/newsletter
     exclusion applies only to the "important" bucket).
   - If both apply, report the message once with reason `"important"`.
4. Cap the output at the **12 most relevant messages** (prefer important,
   then most recent). For each, capture:
   - `id` — the message's identifier, exactly as the tool returns it
   - `sender` — display name if present, else the address
   - `subject` — the subject line
   - `date` — ISO 8601 timestamp received
   - `reason` — exactly `"important"` or `"topic"`
   - `topic` — the exact topic string from `Mail/.topics.md` that matched,
     or `null` when `reason` is `"important"`
   - `summary` — **1–2 plain-text sentences** on the message's actual
     content (not a restatement of the subject)
5. Sort by `date`, newest first.
6. Reply with **exactly one JSON object and nothing else** — no markdown
   fence, no text before or after:

   `{"emails": [{"id": "...", "sender": "...", "subject": "...", "date": "...", "reason": "important", "topic": null, "summary": "..."}]}`

If no mail tool is available at all, reply with exactly this and nothing
else: `{"emails": [], "error": "no mail tool available"}`
