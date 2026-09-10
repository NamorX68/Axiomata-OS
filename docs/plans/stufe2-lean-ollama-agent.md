# Plan: Stufe 2 — a lean local agent for connector digests

Status: **not started.** Follows Stufe 1 (drop `module-context.md` from skill runs, shipped
in commit `42fc45d`). Pick up with a fresh budget.

## Context

The connector digests (`mail-digest`, `calendar-digest`, `reminders-digest`) run as full
`claude -p` agent turns. That is fatal for any local Ollama model:

| model | speed | executes the SOP? |
|---|---|---|
| qwen3:30b, granite4.2:8b, gemma4:12b-mlx | timeout >600 s (~8 steps × ~35–44K-token ctx) | yes |
| lfm2.5:8b | fast | no — greets 2/2 |
| gemma4:e4b-mlx | ~30 s | no — greets ~3/4, treats the whole scaffold as "my briefing" |

Stufe 1 (drop `module-context.md`, ~1.6K tokens) did **not** move the needle locally — the
Claude Code system prompt + the user's `~/.claude/CLAUDE.md` dev-standards still dominate and
the small model never recognises the `SKILL.md` body as its task. Those two we cannot strip
from `claude -p`.

**Outcome wanted:** a code path that sends a local model *only* the skill's SOP + the tool
schemas it needs — no Claude Code system prompt, no dev CLAUDE.md, no module manifest — and
runs a bounded tool-call loop directly. Then a small fast model has a real shot, and the
`mail`-summarization quality question becomes testable on its own.

This is the deliberate exception to CLAUDE.md's "connector = provider = skill, not code" —
but only for the *local* case, and with a clear reason.

## Shape

New `AgentBackend` variant, selected per skill by `SKILL.md` frontmatter
`backend: ollama-agent` (opt-in; `claude-code` stays the default and the fallback). It
bypasses the per-role provider system entirely — always talks to local Ollama at
`config.agents.providers.ollama.base_url`.

```
system  = <preamble> + SKILL.md body
tools   = MCP tool schemas for the servers referenced by the skill's `allowed_tools`
loop (max ~12 iterations, within timeout_secs):
  POST /api/chat {messages, tools}      (ollama-rs, native tool calling — no Anthropic shim)
  -> model returns tool_calls?  dispatch each via the MCP client, append `tool` messages, repeat
  -> model returns content, no tool_calls?  that's the final output -> RunRecord.stdout
```

Model resolution: reuse the `resolve_skill_model` convention — `SKILL.md` `model:` wins,
else `providers.ollama.skill_model`, else `ollama_model`.

## The real work: an MCP client in Rust

Axiomata speaks MCP **nowhere** today; `claude -p` does it. Stufe 2 needs Axiomata to spawn
the `apple-mail` / `apple-reminders` MCP servers itself and speak the protocol.

- **CP1 decision:** `rmcp` (official `modelcontextprotocol/rust-sdk`, stdio transport) vs. a
  minimal hand-rolled stdio JSON-RPC client. Lean toward `rmcp` unless it drags in too much.
- **Server definitions:** where from? Claude Code's `~/.claude.json` MCP block is the de-facto
  source but its format is undocumented. Safer: a new Axiomata-owned `[mcp_servers]` table in
  `config.toml` (`name -> {command, args, env}`), seeded from the current Claude config once.
- **Per run:** derive the needed servers from the skill's `allowed_tools` prefixes
  (`mcp__apple-mail__…` -> `apple-mail`), spawn only those (stdio subprocess), `initialize`
  handshake, `tools/list`, then `tools/call` in the loop, kill on completion. No pooling in v1.

## Checkpoints

- **CP1 — MCP client.** Dependency decision + a thin wrapper: spawn a stdio server,
  initialize, list tools, call a tool, shut down. `[mcp_servers]` config schema + a one-time
  import helper. Unit tests against a trivial mock server (or `rmcp`'s test fixtures).
- **CP2 — the backend + loop.** `AgentBackend::OllamaAgent { model }`; `AgentBackend::resolve`
  maps `"ollama-agent"`; new `run_on_backend` arm in `skills/runner.rs`; `provider_label` ->
  `None` (local, unmetered). Tool-call loop via `ollama-rs` chat + tools. Preamble text.
  Unit tests with a fake Ollama client + fake MCP client (tool_call -> result -> final JSON).
- **CP3 — wire the digests.** `backend: ollama-agent` in the 3 read-only digest `SKILL.md`s
  (leave `cleanup` on `claude-code` — it edits files). End-to-end: run each under a scratch
  `AXIOMATA_HOME` against real Ollama + real MCP servers; assert valid JSON, under
  `timeout_secs`. Compare `mail-digest` summary quality against the OpenRouter baseline
  (run #235-style output) with 2–3 candidate models (gemma4:e4b-mlx, and re-test
  granite4.2:8b / lfm2.5:8b now that the framing is gone).
- **CP4 — docs + default.** `docs/architecture.md` §6 "Agent backends" (three variants now)
  and the connector-module note; CLAUDE.md's backend list. Decide: flip the digests' default
  to `ollama-agent` (with `claude-code` fallback if Ollama is down), or keep it opt-in via a
  config toggle. Update `docs/plans/per-role-provider.md`'s "Stufe 2" pointer.

## Non-goals (v1)

Streaming; parallel tool calls; MCP server pooling; a lean loop against *cloud* providers
(possible later — cheaper minimal-prompt calls — but out of scope); `cleanup` on the new
backend.

## Risks

- MCP server config discovery — plan assumes we own `[mcp_servers]` rather than parse
  Claude's file.
- Small-model tool-arg formatting: the one gemma4:e4b-mlx run that engaged (session run #230)
  reported "tool calls failed". Native `ollama-rs` tool calling removes the Anthropic-shim
  variable; whether the model itself formats args cleanly is a CP3 finding.
- Summarization quality on a 4–8B model is still unproven — but for the first time testable
  without the instruction-following wall in the way. If no local model summarizes mail
  acceptably, the fallback stays `skill_provider = open_router` and Stufe 2 still pays off
  for calendar/reminders (pure tool-call + shape).
