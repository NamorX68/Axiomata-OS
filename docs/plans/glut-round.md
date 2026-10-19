# Glut rund — web, fewer tokens, read access, streaming, identity

Status: **approved 2026-10-19**, owner decisions W1–W8 taken, CP0 measured, CP1–CP3 and W8 built, reviewed and audited; CP4 and CP5 built.

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
- **W8 — skills name their rights** (owner, 2026-10-19): a skill without `allowed_tools` may only read on Glut
  (HS6b B3), while Opencode allows everything and Claude Code edits, shell, web and MCP. The bundled skills that write
  or use the web (`news-rss`, `news-web`, `model-news`, `inbox-sort`, `todo-to-kanban`) get an explicit
  `allowed_tools` so they run alike on all three harnesses; B3 stays. Nothing is closed off for a coming Glut TUI:
  there the owner sits in front of the run, and what a skill does not name can be *asked* (the gate's default mode
  with an asker other than `Nobody`) instead of refused.
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

## CP1 — built (2026-10-19)

`[agents.web]` (`WebSettings`: three keys, `daily_search_limit` 50, `tavily_monthly_limit` 1000; keys redacted in
`Debug` and never sent to the webview, a blank key clears); the Settings block "Web search (Glut)"; the system prompt
names Glut and the model; `access: read` on Glut = `Mode::Plan`, reads, the declared tools without shell rules and
the shared read commands (stricter than Opencode's read instance: what the skill does not name is refused).

## CP2 — built (2026-10-19)

`web_fetch` (`axiomata-harness/src/tools/web.rs`, always in the toolbox): https only on 443/8443, no userinfo, no
local, private, link-local, shared, documentation or reserved address — IPv4, IPv6 and every IPv4 inside IPv6
(mapped, compatible, NAT64, 6to4) — checked after DNS; the connection is pinned to the checked addresses with the
URL carrying the normalised host (a trailing dot would have missed the pin); no environment proxy; same-host
redirects followed (5, each checked again), a cross-host one handed back as data; text types only, 5 MB, 45 s for the
whole fetch; HTML → Markdown with `htmd` 0.5.6 (Apache-2.0); 25,000 characters a call, read on with `offset`; the page
framed as data with an end mark. Rules: `WebFetch` / `WebFetch(domain:…)` are a kind of their own that speaks about
`web_fetch` alone — never about the shell's proxy — while `Network(…)` serves both. Read skills get **no** web unless
they name it (security audit: reads + untrusted input + the network would be a way out), on Glut and Claude Code
alike. Live: Ling fetched `opencode.ai/v2/docs/websearch` and answered from it in one call.

## CP3 — built (2026-10-19)

`web_search` (`axiomata-harness/src/tools/search.rs`): Exa through its hosted MCP endpoint (keyless, an optional key
as `x-api-key`), then TinyFish (`X-API-Key`, measured: no key → 401), then Tavily (`Bearer`, basic = 1 credit); a
provider without a key is skipped, a 429/error hands over, the answer names the provider and is framed as data with
an end mark; 5 results by default, at most 10, 1,200 characters each. A `SearchBudget` from the caller holds the
limits: the app's (`axiomata-core/src/web_search.rs`) counts in the new table `web_searches` (migration 33) — the
local day's searches against `daily_search_limit` (closed for every provider), Tavily's of the local month against
`tavily_monthly_limit` (Tavily skipped); a budget that cannot be read closes the search; a limit of 0 offers no tool.
`axh` takes the keys from `EXA_API_KEY`/`TINYFISH_API_KEY`/`TAVILY_API_KEY` without limits. Rule `WebSearch` (Claude's
spelling) names the tool. Live: Ling searched via Exa and answered "OpenCode 2 is v2.0.32" with its source; the search
was counted.

## W8 — built (2026-10-19)

The five bundled skills name their rights in Claude's spelling, read alike by Glut and Claude Code (Opencode keeps
its unattended allow): the news skills `Read(News/**) WebFetch Edit(News/<their file>.md) Bash(mkdir -p News) Bash(date:*)` —
they read and search only `News/` and write only their own file, so untrusted pages plus the web are no way out for
the rest of the vault, nor a way in for what other skills read; `inbox-sort` `Read Bash(axiomata-cli inbox move:*)`; `todo-to-kanban`
`Read Bash(axiomata-cli board list:*) Bash(axiomata-cli board add:*)`. Their texts name tools generically ("the web
fetch tool"). A rule can name only a literal command — Glut's gate refuses `$AXIOMATA_CLI …` as an expansion — so the
app puts the folder of the `axiomata-cli` beside it (never an `AXIOMATA_CLI` override's) first on its `PATH`
(`login_path::adopt`); a skill calls `axiomata-cli` by name. Tests send each skill's real commands through Glut's
gate. An installed copy gets the new texts with `skills reseed --force`.

**Review and security audit of CP3 + W8 (2026-10-19), what changed:**
- `Bash(mv -n:*)` let `mv -n -f a b` and any destination through (measured by the audit): `inbox-sort` now has
  `axiomata-cli inbox move <file> <folder>` — a note (`md`, `txt`, `pdf`, `canvas`, images) of `Inbox/` into an
  existing folder of an area (a top-level folder with its `AGENTS.md`, not `Inbox`/`Kanban`/`News`/`Mail`), no link
  on the way, never a file the agents read as instructions (`AGENTS.md`, `CLAUDE.md`, `AGENT.md`, … without case),
  never overwriting (a hard link fails on an existing target, then the inbox name goes); it runs before the core
  starts, so Glut's sandbox (which hides `~/.axiomata`) does not stop it. A second audit of the fixes added the case,
  the note types and the area check.
- A search (`grep`, `glob`) is judged as a read of its glob's fixed folder (`News/**` → `News/`), so `Read(News/**)`
  allows searching `News/` and nothing wider; the shell's `grep` is no longer declared.
- `web_search`: `SearchBudget::reserve` checks and counts in one `BEGIN IMMEDIATE` transaction *before* the request
  (a billed failure counts; two runs cannot both take the last search), `release` gives a keyless Exa search back
  when it did not answer, a budget that cannot count closes the search; 20 searches a run, queries ≤ 400 characters,
  20 s per provider in all, no redirects, no environment proxy, answers ≤ 2 MB, key headers sensitive, provider
  errors with the keys blanked and cut; an answer without a `results` list hands over instead of "No results"
  (TinyFish's shape measured: `results[].title/url/snippet`); "generate" no longer reads as a rate limit.
- Page and search text reach the model through one `as_data`: no control or invisible characters, no copy of the
  frame's end mark.
- A day or month starting in a DST gap begins at its first real hour, no longer at "now" (the limits failed open).

**`todo-to-kanban` on Glut** (owner, 2026-10-19: "Glut soll diesen Skill ausführen können"): the sandbox hides
`~/.axiomata`, where the CLI's database is, from every command (the owner's keys live there too). So the shell starts a
`DirectProgram` of the caller's — the app's own `axiomata-cli` at its absolute path, with the app's `AXIOMATA_HOME` —
directly, without shell or sandbox, its words as arguments, when the command is nothing but words (no assignment,
redirection, second command or expansion) **and** every narrowing of the run names it by a pattern of its own
(`Gate::declares_command`: `Bash(axiomata-cli board add:*)`, never a bare `Bash`) **and** it is one of the program's
own direct commands (`board list`, `board add`, `inbox move` — a declaration as wide as `Bash(axiomata-cli:*)` reaches
no other). A chat turn declares nothing, so no command of it is trusted; anything else stays in the sandbox. A third
audit (of this path) found no way past the declaration but the CLI's reach: the direct process carries
`AXIOMATA_HARNESS_RUN`, and `board add` then sets no plan, role or flow field and uses neither the proposal nor the
review column (a card in a self-running plan would have become an agent's prompt). `Bash(:*)` is no rule any more (it
panicked the matcher).

**The audits' last points, cleared (owner, 2026-10-19: "Beseitige die Audits")**: the harness mark rides on every
command an app harness runs — Opencode's sessions (`session_env`) and `claude -p` (`app_command`) as well as Glut's
direct CLI — so `board add` takes no plan, role or flow field from any skill or chat turn (the owner's terminal and
master pane do not carry it; planner and card sessions add cards through MCP); a broken `config.toml` is reported by
line and the parser's message, never the line itself; the providers' error words in `web_search` are framed as data;
`inbox move` needs the vault's own router beside `Inbox/` and checks after linking that no folder on the way became
a link and the note was not swapped, else it takes the new name back; the direct start stops with the run and keeps at
most 256 KB of output; a few more invisible characters are dropped from page and search text. A fourth audit then: under the mark the owner's gates (`owner_only`: approving, starting, integrating, deleting,
reseeding …) and the agents' steps (claim, report, verdict) are refused too, and `board edit`/`board move` keep a
card plain like `board add`; `inbox move` walks, links and unlinks through folder handles (`openat` without following
a link, `linkat`, `unlinkat`), takes back a new name that is not the checked note and leaves a swapped inbox file
where it is; a config error is cut before the first quoted value. The chat carries the mark as skills do: approving
belongs to the owner's own click, while routines, skills and module actions stay open to it. On Opencode's
unattended shell a model could drop the mark (`env -u`) or open the database itself, so there it is a second line,
not a boundary — a real one would need an owner's proof the model's shell cannot read. A fifth round, after the fourth audit found `board move` approving a proposal by moving it: the mark says the
kind of run (`AXIOMATA_HARNESS_RUN=skill|chat`; anything but `chat` reads as `skill`). A **skill run** — nobody watches
it — gets a short list from the CLI only: `status`, `list-skills`, `list-runs`, `get-run`, `board list`, `board add`
(a plain card) and `inbox move`; everything else is refused before the core starts. A **chat turn** keeps the CLI but
not the owner's gates, the agents' steps (claim, report, verdict, note, input, fail, cancel) or `board dep`; it edits
and moves only plain cards (no plan, role, claim or acceptance, not in a flow column) and only between plain open
columns (no proposal, review or done). Opencode's skill runs and `claude -p` runs carry `skill`, their chat turns
`chat`; Glut's direct CLI the run's own. `inbox move` folds names as APFS does (`AGENTſ.md` is `AGENTS.md`). The Kanban
shell actions behind `module-action` (the assistant's and the agents' channel; the board itself calls Tauri directly)
keep to the same rule (`core/kanbanAgentRules.ts`): no card added to or moved into a flow or done column, no card of
the flow moved. `board dep list` stays open; adding or removing a dependency is the owner's. What stays, by the
harnesses' own design: on **Claude Code** reads inside the working folder need no rule (a news skill there can read
the vault — Glut is the stricter harness for the news skills), and **Opencode** keeps its unattended allow, so a
skill's `allowed_tools` bind Glut and Claude Code only.

## CP4 — first part built (2026-10-19)

The toolbox offers the model only what the gate could allow (`Gate::offers`, `Tool::offered_as`): with a
declaration, the tools some declared rule could name, patterns left aside (`Edit(News/**)` offers `edit_file`); no
tool a pattern-less deny refuses whole; no edit tool in Plan mode; a call of a tool not offered is refused. A skill
run gets no `skill` tool (its body is the task — CP0 saw the model load the running skill again). 

**Cached tokens (owner, 2026-10-19: "bitte so umsetzen")**: `input_tokens` now counts every prompt token processed on
all three harnesses (Opencode's `tokens.input + cache.read + cache.write`, as Claude Code and Glut already did), and
the part read from the cache is kept apart (`cached_input_tokens`: Glut from `usage.prompt_tokens_details.cached_tokens`
or DeepSeek's `prompt_cache_hit_tokens`, Opencode `cache.read`, Claude Code `cache_read_input_tokens`), stored in
`runs` and `chat_turns` (migration 34) and metered at `agents.costs.<model>.cached_input_per_m` — unset, at the input
price, so the meter never counts lower than the owner said; the start-up re-metering reads the column, so a row from
before (NULL) is metered as before. Studio sessions keep A27 (cache reads left out of their limits). Still open:
shortening old tool results, if a measurement still asks for it.

## CP5 — built (2026-10-19)

The OpenAI-compatible client asks for `stream: true` with `stream_options.include_usage` and reads the server-sent
chunks into the same `Reply`: text pieces, tool calls whose id, name and arguments arrive in parts (by their `index`;
one without an id gets `call_<index>`), the usage of the last piece. A provider that ignores `stream` answers in one
JSON body, read as before. The **stall limit**: 120 s without a byte — a provider's keep-alive comment counts —
fails the call with "the model sent nothing for 120 s"; an `error` inside the stream (DeepInfra's 400 over its context
length) fails it at once; a stream without any choice is no answer. Each text piece goes to the sinks as
`Event::TextDelta { turn, text }` while it arrives, the whole text after it as before (`ChatModel::complete_streaming`;
a model that does not stream keeps the default and hands nothing over). The readable `harness run` lines skip the
pieces; `--json` has them. The chat window still shows the answer whole (W6). Live: todo-to-kanban on Glut via
OpenRouter streamed, its usage read from the last piece.

After the review: tool calls are put into slots by `index` **and** id (a provider that numbers every call `0`, or
none, gets separate calls), a name repeated in each piece is not doubled, an empty id never replaces a real one;
`"error": null` is no error (in one body either); a stream that just stops — no `[DONE]`, no `finish_reason` — is cut
off, not an answer; 300 s of keep-alive comments without a piece fail the call (`PIECE_LIMIT`), reqwest's 10-minute
limit says so; an empty `usage` at the end keeps the count before it; a stream under another content type (`text/plain`)
is still read as one. Tests cut a stream at every byte (inside a line and inside `€`).

## CP6 — docs and the live check (2026-10-19)

`docs/architecture.md` §5 "Glut rund" and §7, `docs/plans/harness.md`'s road map, `AGENTS.md`. **news-web on Glut,
live** (run #1712): it ran — 4 new items, the failing sites named — in 18 steps and 145 s, but on **1.7 M input
tokens** (1.46 M from the cache, $0.012): every fetched page rides along in every later step. That is CP4's open
point (4), now measured: shorten an old tool result in the history once the model has answered on it.

Next: CP4 (4), shortening old tool results (measured in CP6); the
Settings block could show today's count (`web_search::usage`).
