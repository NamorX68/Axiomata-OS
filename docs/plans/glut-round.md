# Glut rund — web, fewer tokens, read access, streaming, identity

Status: **approved 2026-10-19**, owner decisions W1–W7 taken, CP0 measured.

## Why

After `docs/plans/harness-choice.md` the owner ran the same skills on all three harnesses (2026-10-19). Glut worked
but is not yet one to keep a role on: it has no web tools (`news-web` cannot run on it), it used 230k input tokens for
the calendar where Opencode counted 54k, it ignores `access: read`, it has no streaming (a stalled provider is only
noticed at the run's limit), and the model does not know where it runs (DeepSeek in the Glut chat called itself
"Claude"). The model is **Pi** (`badlogic/pi-mono`): a small fixed core, everything else clearly separate and switchable
— but the web belongs in Glut itself, natively, not as an MCP server the owner has to set up.

How the others do it (researched 2026-10-19):

| | Fetch a page | Search |
|---|---|---|
| Claude Code | `WebFetch`: local fetch, HTML→Markdown, then a small model extracts what the call asked for ("lossy by design"); 100k characters per call, 15 min cache | `WebSearch`: Anthropic's server-side tool, titles + URLs only; Anthropic API only |
| Opencode 2 | `webfetch`: one URL as Markdown/text/HTML, no model; long output cut, the full text kept | `websearch`: a third-party integration — Exa, Firecrawl, Parallel, Tavily, TinyFish — chosen once; on 429 another one |
| Pi | not in the core (`read`, `write`, `edit`, `bash`) | extensions, mostly `pi-web-access` (Exa without a key, DuckDuckGo as last resort) |

## Decisions (owner, 2026-10-19)

- **W1 — `web_fetch` is raw**, as Opencode's: the page as Markdown, cut at a limit, read on with an `offset`; no second
  model, nothing silently left out, no extra cost.
- **W2 — `web_search` asks Exa first, then TinyFish, then Tavily.** Exa's hosted MCP endpoint
  (`https://mcp.exa.ai/mcp`, tool `web_search_exa`) works without a key, rate limited (a 429 means the free limit);
  TinyFish (`https://api.search.tinyfish.ai`, free key, 30/min) and Tavily (`POST https://api.tavily.com/search`,
  `Bearer tvly-…`, 1,000 free credits a month, basic search = 1 credit) take over on a 429, an error or a missing key.
  A provider without a key is skipped, never an error. For chat turns and skill runs alike.
- **W3 — no unbounded spend:** a daily search limit in `config.toml` (proposal: 50 searches a day across all
  providers, counted in the database like the spend meter); over it, `web_search` answers with an error the model
  reads, the run goes on.
- **W4 — "Glut rund" is five things:** web (W1–W3), fewer tokens, `access: read`, streaming, identity in the prompt.

## Design

- **Tools, permissions.** `web_fetch` and `web_search` are core tools of the harness beside the file tools and `bash`,
  named as the model sees them; rules in Claude's spelling as the rest of Glut's: `WebFetch(domain:example.com)`,
  `WebSearch`. The gate judges `web_fetch` as `Access::Network { host }` — the same rules the sandbox proxy already
  asks (`Network(crates.io)`), so one allow list serves the shell and the fetch. A skill's `allowed_tools` names them
  like any tool; a chat turn in auto mode may use them unless `~/.axh` denies.
- **`web_fetch` safety** (as Claude Code): `http`→`https`; no `localhost`, no dot-less host, no private/loopback/link-
  local address (checked after DNS, so a public name pointing inward is refused too); a redirect to another host is
  returned as text for a second call, never followed; text types only (HTML, plain, JSON, XML); a time limit; the
  fetched text is untrusted data in the tool result, framed as such.
- **`web_search` providers** behind one trait (`SearchProvider::search(query, options) -> Vec<Hit>`; a hit is title,
  URL, snippet, and the page text when the provider gives it). Keys in `config.toml`
  (`[agents.web] exa_api_key`, `tinyfish_api_key`, `tavily_api_key`, `daily_search_limit`), never on a command line,
  redacted to the webview like the provider keys. Exa is reached through the harness's own MCP client (HTTP).
- **Fewer tokens** — measured first (CP0): where the calendar's 230k came from (each of 6 steps resends the whole
  conversation; the MCP outputs and the model's own large tool inputs ride along every time) and how Opencode counts
  (cached input left out?). Then, as far as the measurement says: older tool results shortened in the history once
  the model has answered on them (their head kept, a note how much was cut), a cap on what one tool result may add,
  and for long chats a summary of older turns instead of the turns (harness.md "token cost": OChaT's hybrid memory).
- **`access: read`** on Glut: `Mode::Plan` (edits refused even where a rule allows them, commands only by rule) plus the
  shared read commands of `opencode_instance::READ_COMMANDS` as allow rules and `READ_DENIED_COMMANDS` as denies —
  the same list Opencode and Claude Code use.
- **Streaming:** `stream: true` on the OpenAI-compatible client, the SSE parsed into the same `Reply`; text deltas as
  `Event::Delta` for sinks that want them; a **stall limit** (no chunk for N seconds → the model call fails with a clear
  reason) — the class of hang the calendar showed on Opencode. A provider that answers with an error inside the stream
  (DeepInfra's 400 over 131k, measured) fails the call at once.
- **Identity:** the harness's system prompt names itself and the model: "You run in Glut, Axiomata's own harness, on
  `<model>` via `<provider>`." The core passes provider and model in.

- **W5 — the web keys in the Settings dialog, first** (owner, 2026-10-19; was Q2): a "Web search" block under AI
  providers with the TinyFish and Tavily keys (and an optional Exa key) as the provider keys are — kept, redacted,
  re-enterable any time — and the daily limit. Built first, so the owner can enter the keys before the tools exist.
- **W7 — Tavily is last, and capped at its free credits** (owner, 2026-10-19: the account needed a credit card): a
  monthly Tavily count, at most 1,000 (its free credits); past it Tavily is skipped, never billed.
- **W6 — the chat window does not stream in this plan** (was Q1): the harness streams for its stall limit and events;
  showing the answer grow in the dashboard is a later step across all three harnesses.

## CP0 — measured (2026-10-19)

- **Exa keyless MCP works:** `initialize` → `Mcp-Session-Id` header, `notifications/initialized`, `tools/call`
  `web_search_exa {query, numResults}`; 3 results in 1.4 s, each with title, URL, date, author and "Highlights" — page
  text ready for the model (7.6 KB for 3). Also offers `web_fetch_exa {urls, maxCharacters}`. Exa's own result for
  the Opencode docs shows TinyFish's key as optional there.
- **Where Glut's calendar tokens go** (`harness run --skill calendar-digest --json`, Ling via OpenRouter, the two
  calendar tools allowed): 43 tools offered; **17.2k input tokens at turn 1** — nearly all of it the schemas of every
  MCP tool (27 of `apple-mail`, 5 of `apple-reminders`), though the skill may use 2; the model first called `skill`
  twice (list, then the calendar skill — its body already *is* the task); the events (~17.7k tokens) then ride along
  in every later turn (35.8k → 37.6k). 7 turns, 180k input, 16k output.
- **How the counts compare:** the app counts only Opencode's `tokens.input`, not `tokens.cache.read` — run #1703
  had 54.3k fresh **plus 80.9k from the cache**, so ~135k processed. Glut counts every prompt token at the full price
  (the OpenAI-compatible `usage.prompt_tokens_details.cached_tokens` is not read).
- **So CP4 is, in this order:** (1) offer the model only the tools the run may use — with a declaration, exactly the
  declared ones (core and MCP), as HS8 already does for `task`; (2) no `skill` tool in a skill run whose body is the
  task (or the catalog without the running skill); (3) read cached tokens and meter them at the cached price;
  (4) then, if still needed, shorten old tool results in the history. Estimate for the calendar: 70–80k instead of
  180–230k. A chat turn (no declaration) still offers every tool — a tool search (load schemas on demand, like
  Claude Code's deferred tools) is a later step.

## Checkpoints

| CP | What |
|---|---|
| **CP0** | Measure: Exa's keyless MCP (`web_search_exa` parameters and answer, the 429), TinyFish and Tavily with the owner's keys; where the calendar's 230k went on Glut (one run with every request's token count logged) and how Opencode counts input. Findings into this file. |
| **CP1** | `[agents.web]` in `config.toml` (keys, daily limit) and the Settings block (W5); identity in the system prompt; `access: read` on Glut (`Mode::Plan` + the shared read lists). |
| **CP2** | `web_fetch`: fetch, HTML→Markdown, offset, the safety rules, `Access::Network` through the gate; a crate for HTML→Markdown (dependency audit). |
| **CP3** | `web_search`: the provider trait, Exa (MCP, keyless/optional key), TinyFish, Tavily, the order and the fallback on 429/error/no key, the daily limit. |
| **CP4** | Fewer tokens, as CP0 says. |
| **CP5** | Streaming with the stall limit and in-stream errors. |
| **CP6** | `news-web` on Glut live; docs (`harness.md`, `architecture.md`, `AGENTS.md`). |

Work happens in a git worktree and reaches `main` only while no skill, routine or chat turn runs (a Rust edit rebuilds
the owner's `cargo tauri dev` and cuts a running agent off — 2026-10-19).
