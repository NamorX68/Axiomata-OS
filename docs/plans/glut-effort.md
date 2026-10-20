# Glut effort — how hard the model reasons

Status: proposed and approved by the owner 2026-10-21 (E1–E6, "Ja mit E4 so bauen"); **built** 2026-10-21 on branch
`glut-effort`.

## Why

Glut sent no reasoning setting at all: `model`, `messages`, `tools`, `max_tokens`. The owner wants to set the effort
level — and Glut becomes an application of its own (owner, 2026-10-21: "alles fein wenn wir Sachen in Axiomatas
Einstellungen setzen aber Glut wird eine eigene Anwendung"), so every source Axiomata offers sits on top of one Glut
already reads itself.

## Decisions

- **E1 — where it is set.** Glut's own first, Axiomata's on top. From the strongest to the weakest:
  1. a command file's `effort:` (`~/.axh/commands/*.md`), over a skill's `effort:` (`SKILL.md`) — as their `model:`;
  2. the run's own: `axh --effort`, `axiomata-cli harness run --effort`, the chat's `/effort <level>` for one
     conversation (Glut chat only; `/effort default` goes back, a new conversation too);
  3. Axiomata's `[agents.glut] chat_effort` / `skill_effort` (Settings → Harness & provider, shown when the role runs
     on Glut);
  4. `"effort"` in the owner's `~/.axh/settings.json` — a project's `.axh/settings.json` level is left out unread
     (a typo there stops nothing): it would spend the owner's money;
  5. nothing: no field is sent, the provider's default holds.

  A trusted project's own commands and skills (`.axh/commands`, `.axh/skills`) may name an `effort:` as they may name
  a `model:` — deliberately the same: the owner starts them by name, in a folder they trusted.
- **E2 — the levels** are OpenRouter's: `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`
  (`axiomata_harness::Effort`). An unknown level fails where a person wrote it: the flag, a command file, the settings
  file, the save in Settings; a harness skill (`axh --skill`) with one is left out with the reason, an app skill's run
  fails at its start and is recorded so. In `config.toml` a typo (or a value that is no text) leaves the level unset
  with a warning — it must not keep the whole config from loading.
- **E3 — on the wire.** OpenRouter (host `openrouter.ai`) gets `reasoning: { effort }`, everyone else the
  OpenAI-compatible `reasoning_effort`. A provider that knows fewer levels may refuse one; its error is the run's.
- **E4 — the model's thinking goes back.** An assistant message keeps what the provider sent of its reasoning, under
  the provider's own names (`reasoning`, `reasoning_content`, `reasoning_details`; streamed blocks merged by `index`
  and `type`), in the conversation and in the journal. It is sent back only after the latest user message — the model
  is still on that prompt, and models that think before a tool call expect their thinking beside it; older reasoning
  is left out (those providers refuse or ignore it, and it costs tokens). Beside its blocks the plain text is the same
  thinking twice, so only the blocks go; the same text under both plain names (vLLM) goes once. Once a newer prompt
  follows, the reasoning is dropped from the conversation itself (`opening_messages`), so a chat's file and a run's
  journal carry at most the current turn's; one message keeps at most 1 MiB of it, more is dropped whole. Within a
  turn the thinking goes back on every step — that is the price of E4, metered like any input. A turn resumed after
  the owner changed provider sends the new one the old one's blocks; it may refuse them, which fails the run visibly.
- **E5 — cost.** Reasoning tokens are part of `completion_tokens` and are metered as output already; the daily cap
  holds as it is. Compaction and the commit-message button use their role's effort like any call of that role.
- **E6 — not here.** Opencode and Claude Code keep their own effort settings.

## Built (2026-10-21)

- `config.rs`: `Effort` (parse, `as_str`, serde lowercase), `ModelEndpoint.effort`.
- `model.rs`: `put_effort` (E3), `api_messages` (E4: older reasoning out, blocks over text), `Message.reasoning*`, the
  stream's `merge_block`, `parse_reply` drops an empty reasoning.
- `permissions/settings.rs`: the file's `effort`, the owner's only (`Loaded.effort`, `BuiltGate.effort`).
- `commands/mod.rs`, `skills/mod.rs`, `setup.rs`: `effort:` frontmatter; the command's wins over the skill's.
- `bin/axh.rs`: `--effort`, kept in the journal (`axh resume` goes on with it).
- Axiomata: `[agents.glut] chat_effort|skill_effort`; `AgentRequest.effort` from a core skill's `effort:` (the field
  existed, unused); `ChatRequest`/`ChatExtras.effort` from `/effort`; `agents::axh::endpoint` resolves run → role,
  `Harness.effort` falls back to `~/.axh/settings.json`; `axiomata-cli harness run --effort`; the Tauri settings
  view/update and `assistant_send`'s `effort`; the Settings selects and `/effort` (`core/commands.ts`, `turnEffort`).

## Measured (2026-10-21, DeepSeek V4.1 Flash on OpenRouter)

- OpenRouter's model list says per model whether it reasons and which levels it knows (`/api/v1/models`,
  `reasoning.supported_efforts`): V4.1 Flash knows `max`, `high`, `low`, `none` and **reasons at `high` by default** —
  every Glut run so far, the lean measurements included, ran at `high`. A level the model does not list (`medium`,
  `xhigh`, `minimal`) is taken to a near one, not refused.
- Direct, one question: `none` 0 reasoning tokens, `high` 106, `max` 123.
- `axh run`, two file reads and a prime question: `--effort none` 217 output tokens, `--effort max` 482; with `max`
  the step after the tool calls carries ~36 more input tokens — the thinking sent back beside the calls (E4), taken
  by the provider without complaint.
