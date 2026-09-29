---
name: mail-digest
description: Reads recent mail via whichever mail tool is available (the apple-mail MCP server today), picks out messages the agent judges important plus ones matching the owner's configured topics, summarises each in a bit more depth (including any links the email contains), and reports them as one JSON object for the Mail dashboard module to read back from this skill's last run.
backend: opencode
prepend_files: ["Mail/.topics.md"]
allowed_tools: mcp__apple-mail__search_emails mcp__apple-mail__get_needs_response mcp__apple-mail__get_email_source
timeout_secs: 600
output: json
---

# Mail Digest

Your entire reply must be **exactly one JSON object** — this is parsed by
software, not read by a person. Output nothing before or after it: no
introduction, no summary, no bullet points, no markdown code fence, no second
copy of the object. If a tool call errors, keep going with the rest; if you
cannot finish, output what you did collect and set `"error"` to a short
reason — an object containing only `"error"` is better than an empty reply.

Report a short, curated list of noteworthy emails as a single JSON object —
this is read by a dashboard module afterwards, not by a person, so the output
format must be followed exactly. This is deliberately **not** a full inbox
listing: only messages that are either clearly important or match one of the
owner's configured topics belong in the output.

Keep the run tight — every tool call is a slow round-trip against Mail.app.
Make **at most ~6 tool calls for the candidate pool**: one dated inbox search,
one needs-response pull per account (at most 2), and one `search_emails` per
*topic group* (not per keyword). On top of that, **at most 3
`get_email_source` pulls** to recover links the rendered body hides (see
step 4). Don't re-fetch a message you already have. **Never call
`list_inbox_emails`**: it has no date filter and reads the whole inbox, which
takes longer than the run may. A call that errors or takes unusually long is
**skipped, not retried** — a timed-out `get_needs_response` is not a reason to
give up, leave that signal out and keep going with what you have.

Do exactly this and nothing more:

1. Configured topics, if any, appear at the very top of this message under
   "Context file: Mail/.topics.md" — one topic per line. If that block is
   absent, there are no configured topics: skip the per-topic `search_emails`
   calls and the topic classification in step 4. Never try to open the file
   yourself.
2. Build the candidate pool for the **last 2 days** — today and the day
   before, counted from the date stated at the top of this message — using
   the mail tool available now (the `apple-mail` MCP server). Every search
   takes `date_from` = that first day (`YYYY-MM-DD`), `output_format="json"`,
   `sort="date_desc"`, `include_content=True` and `max_content_length=1500`,
   so you have body text for the summaries and the URLs in them without
   extra round-trips:
   - `search_emails` once over the inbox of **all accounts** (leave `account`
     unset, `mailbox="INBOX"`, `limit=40`) — the recent mail itself. Note the
     account each message belongs to; the next calls need it.
   - `get_needs_response` once per account that appeared (at most 2), with
     `days_back=2` and `max_results=10` — a precise importance signal.
   - For the configured topics, run **one `search_emails` per topic**, with
     the topic plus its obvious near-synonyms as `subject_keywords` in a
     single call (e.g. "KI/AI/LLM" → `["KI", "AI", "LLM"]`), `limit=15`. This
     surfaces topic-relevant newsletters and mailing-list mail — don't skip
     it, but don't fan it out into many calls either. Don't use `body_text`:
     searching bodies reads every message and is far too slow.
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
   - `sender` — the display name exactly as the header shows it if there is
     one, otherwise the bare email address. When a display name exists,
     output it **alone** — never append the address in angle brackets
     (`Adobe Acrobat`, not `Adobe Acrobat <mail@adobe.example>`). This
     string becomes part of the summary note's file name, so any variation
     creates a duplicate note.
   - `subject` — the subject line
   - `date` — ISO 8601 timestamp received
   - `reason` — exactly `"important"` or `"topic"`
   - `topic` — the exact topic string from `Mail/.topics.md` that matched,
     or `null` when `reason` is `"important"`
   - `summary` — **2–4 plain-text sentences** on the message's actual
     content (not a restatement of the subject): what it is about, the key
     facts, and any date, amount or question it contains.
     **Links belong in the summary**: if the email body contains a URL that
     matters to the reader (registration, document, article, tracking, …),
     append the URL(s) at the end of the summary as plain text, each on its
     own line prefixed `Link: ` — at most 3 links, only real `http(s)` URLs
     taken from the email, never invented ones. The rendered body drops
     hidden hrefs and shows a `�` placeholder instead; in that case pull
     the raw RFC 822 source with `get_email_source` for that one message
     (within the 3-pull budget; give its `account` and its `message_id`,
     and `max_bytes=65536`) and take the hrefs from there. If no usable
     link surfaces, just leave the links out.
5. Sort by `date`, newest first.
6. Reply with **exactly one JSON object and nothing else** — no markdown
   fence, no text before or after:

   `{"emails": [{"id": "...", "sender": "...", "subject": "...", "date": "...", "reason": "important", "topic": null, "summary": "..."}]}`

If no mail tool is available at all, reply with exactly this and nothing
else: `{"emails": [], "error": "no mail tool available"}`
