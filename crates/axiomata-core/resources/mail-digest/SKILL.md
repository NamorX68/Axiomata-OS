---
name: mail-digest
description: Reads recent mail via whichever mail tool is available (the apple-mail MCP server today), picks out messages the agent judges important plus ones matching the owner's configured topics, summarises each, and reports them as one JSON object for the Mail dashboard module to read back from this skill's last run.
backend: claude-code
allowed_tools: mcp__apple-mail__get_needs_response mcp__apple-mail__search_emails mcp__apple-mail__list_inbox_emails
---

# Mail Digest

Report a short, curated list of noteworthy emails as a single JSON object —
this is read by a dashboard module afterwards, not by a person, so the
output format below must be followed exactly. This is deliberately **not**
a full inbox listing: only messages that are either clearly important or
match one of the owner's configured topics belong in the output.

Do exactly this and nothing more:

1. Look for a file named `Mail/.topics.md` in your current directory (the
   workspace root). If it exists, read it: each non-empty line is one topic
   of interest (e.g. "Fotografie", "Development", "KI/AI/LLM"). If the file
   does not exist, or every line is blank, there are simply no configured
   topics right now — proceed with step 3 only, skip step 4 entirely.
2. Do not create, edit, or otherwise touch `Mail/.topics.md` — read-only.
3. Build a broad candidate pool for the last 3 days, using whichever mail
   tool is available right now (the `apple-mail` MCP server today) — do
   **all** of these, not just whichever comes up first:
   - List recent inbox mail directly (e.g. `list_inbox_emails`) so you have
     subject/sender/date for everything, not just what a keyword search
     happens to surface.
   - Pull messages that need a response (`get_needs_response`) and a
     flagged-mail search — two reliable, precise importance signals worth
     having on top of the plain listing.
   - For **every** configured topic from step 1, also run a dedicated
     `search_emails` call (the topic itself, plus closely related terms if
     it's broad — e.g. "KI/AI/LLM" implies searching all three). This is
     not optional and not just a fallback for ambiguous cases: newsletters
     and mailing-list mail (marketing digests, course/newsletter senders)
     are exactly the messages most likely to be topic-relevant yet absent
     or buried in a plain inbox listing, so skipping this step
     systematically misses them — confirmed live, this is the actual
     reason real topic-matching newsletters (Real Python, a Leica
     photography newsletter, a Rust course mailer, Medium's digest) were
     going unreported.
4. Classify every candidate you now have — from the listing, the
   needs-response/flagged pulls, and every topic search — yourself, using
   **judgement, not exact string matching**. A topic like "Fotografie"
   should match an email about camera gear, a photography meetup, or
   editing software even if the word "Fotografie" itself never appears in
   it; "important" is a judgement call about the message's actual content
   and context, not a fixed keyword list. Concretely:
   - **Important** — needs a response, is flagged, asks a direct question,
     names an explicit deadline, is an invoice/receipt/bill, is an
     invitation (an event, a meeting, an RSVP request), or is part of a
     thread the owner has visibly participated in before (a reply chain
     with the owner's own prior message in it). Automated notifications,
     newsletters, and no-reply marketing do **not** count as important —
     except an invoice/receipt/bill, which does, even from a no-reply
     sender.
   - **Topic** — matches the *substance* of a configured topic from step 1.
     Unlike "important", a newsletter or mailing-list email absolutely can
     be a topic match (a photography newsletter about new camera gear IS a
     "Fotografie" match) — the important/newsletter exclusion above applies
     only to the "important" bucket, never to topic matching. Don't limit
     yourself to messages containing the topic word verbatim.
   - A message may fit more than one topic, or be both important and
     topic-matched; report it once, reason `"important"` taking priority
     if both apply, under whichever topic you'd call it if `"topic"`.
5. For every message you're including (skip anything already added — a
   message doesn't need to be found twice), capture:
   - `id` — the message's own identifier, exactly as the tool returns it
   - `sender` — the display name if there is one, otherwise the address
   - `subject` — the message's subject line
   - `date` — ISO 8601 timestamp of when it was received
   - `reason` — exactly `"important"` or `"topic"`
   - `topic` — the exact topic string from `Mail/.topics.md` that matched,
     or `null` when `reason` is `"important"`
   - `summary` — 2-3 plain-text sentences summarising the message's actual
     content (not just restating the subject) — this is the whole point of
     this skill, spend real effort on it
6. Sort the collected messages by `date`, descending (newest first).
7. Reply with **exactly one JSON object and nothing else** — no markdown
   code fence, no explanation before or after it, no trailing text:

   `{"emails": [{"id": "...", "sender": "...", "subject": "...", "date": "...", "reason": "important", "topic": null, "summary": "..."}]}`

If no mail tool is available at all, reply with exactly this and nothing
else: `{"emails": [], "error": "no mail tool available"}`
