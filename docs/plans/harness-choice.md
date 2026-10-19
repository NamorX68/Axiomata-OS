# Harness choice — Opencode, Claude Code or Glut, per role

Status: **draft 2026-10-19**, owner decisions D1–D5 taken, Q1 open (proposal: not in this plan), nothing built.

## Why

The settings let the owner pick a **provider** per role (`agents.chat_provider`, `agents.skill_provider`), and the
model follows from it. The **harness** that drives the turn is not a choice: the chat always runs on Opencode, a skill
on whatever its `backend:` frontmatter says (default `opencode`; `axh` and `ollama` exist). Claude Code drives no chat
or skill since Stufe 2 CP5 — it lives on only as an engine of the Studio's panes and card sessions. The owner wants the
harness to be a setting like the provider: **Opencode**, **Claude Code** or **Glut** (the own harness, `axiomata-harness`
/ `axh`, working name from `docs/plans/harness.md`).

Out of scope: the Studio's engines and roles (`ide engines`, `AGENT.md`) — card sessions already name their engine.

## Decisions (owner, 2026-10-19)

- **D1 — per role.** Two fields, `agents.chat_harness` and `agents.skill_harness`, beside the two provider fields.
  Default `opencode` for both, so an existing `config.toml` behaves exactly as today.
- **D2 — Claude Code serves Anthropic only.** It runs as `claude -p` on the owner's subscription; picking it limits
  the role's provider to `anthropic`. No `ANTHROPIC_BASE_URL` redirect to OpenRouter.
- **D3 — a skill's `backend:` wins when it is set.** Without the field a skill follows `skill_harness`. The bundled
  skills lose their `backend: opencode` line unless they need Opencode (see "Skills that are tied to one harness"),
  and reach existing installs through `skills reseed --force` (seed-if-absent trap, `AGENTS.md`).
- **D4 — Glut learns multi-turn chat in this plan**, as its own checkpoint (it is first on harness.md's "after the
  backend" list anyway).

## Which provider on which harness

| Harness | anthropic | open_router | ollama | Chat | Skills |
|---|---|---|---|---|---|
| Opencode | ✓ | ✓ | ✓ | ✓ (today) | ✓ (today) |
| Claude Code | ✓ (subscription) | — | — | CP3 | CP2 |
| Glut (`axh`) | — (Q1) | ✓ | ✓ | CP4 | ✓ (HS6b) |

`AgentDefaults::validate_for_save` refuses a role whose provider its harness cannot serve, with a message naming both;
the Settings dialog only offers the providers that fit the chosen harness. A config edited by hand into a bad pair is
not "fixed" on load — the run records a clear `Failed` (as HS6b's B1: never a silent hop to another harness).

## Design

- **Config.** `pub enum HarnessId { Opencode, ClaudeCode, Glut }` (`opencode` | `claude_code` | `axh` in TOML — `axh`,
  not `glut`, so it is the same id as the skill frontmatter's and `runs.backend`'s). `AgentDefaults::harness_for(role)`
  beside `provider_for(role)`.
- **Skills.** `registry` keeps `backend: Option<String>` (no `default_backend()` any more); the runner resolves
  `skill.backend.unwrap_or(config.agents.skill_harness)`. Routines' `prompt` target (`execute_prompt`) takes the same
  default. `runs.backend` keeps recording the harness that really ran (`claude_code` new).
- **Chat.** `agents::chat` dispatches on `chat_harness`; `ChatReply` and `chat_turns` stay as they are. A session id
  belongs to one harness: switching the harness starts a new conversation (the dashboard drops the old id), never a
  resume across harnesses.
- **Read access** (`access: read`) on every harness from one list: the read commands and denies of
  `opencode_instance::read_rules` move to a shared place and become Claude Code's `--disallowedTools`/`--allowedTools`
  and Glut's deny/allow rules.
- **Claude Code runner** (`agents/claude_code.rs`): `claude -p --output-format stream-json --verbose --model <m>`,
  cwd = workspace, cleared environment (the provider-hardening `env_clear` rule), the prompt built by
  `skills::runner::build_prompt_at` as for the others, MCP from `~/.axh/mcp.json` written to a per-run file for
  `--mcp-config` with `--strict-mcp-config` (D7 of agent isolation: the same servers everywhere, never the owner's
  `~/.claude.json`), the skill's `allowed_tools` / the chat's mode as `--allowedTools`, the module context as
  `--append-system-prompt`, `--resume <id>` for the chat. Tokens and cost from the final `result` event; the spend cap
  does not meter Anthropic (subscription), as today. The old `--allowedTools` trap (`docs/architecture.md` §5) is why
  the list is built from frontmatter here and tested, not taken as documentation.
- **Glut chat** (CP4): a session is a JSONL file of the conversation under `~/.axiomata/harness/sessions/<id>.jsonl`;
  `axiomata_harness::run` takes the earlier messages, the turn appends its own. No compaction yet (harness.md "token
  cost" stays open); a session over the model's window fails the turn with a message saying to start a new chat.

## Skills that are tied to one harness

- `output: json` skills stay on Opencode (harness.md HS6b D3: no JSON-repair turn on Glut); Claude Code gets one only if
  CP0 shows `--output-format json` gives a clean object — otherwise they keep `backend: opencode` explicitly.
- `backend: ollama` (one completion, no tools) is not a harness and stays a frontmatter-only choice.

## Open questions

- **Q1 — Glut with Anthropic?** Glut has no Anthropic client yet (harness.md H4, "second client behind the same
  trait"). Proposal: not in this plan; the matrix shows `—` and the setting explains why.
- ~~Q2~~ → **D5** below.

## D5 — the app's Claude Code is apart from the owner's (owner, 2026-10-19)

The chat and the skills on Claude Code get **none** of the owner's `~/.claude` setup: no permissions (the allow list
would widen a skill beyond its `allowed_tools` — an unattended run reading mail must not inherit a developer's shell
rights), no hooks, no plugins, no global `CLAUDE.md`. Whatever the app's Claude Code needs (a skill, a plugin) is
installed into **its own** setup, not borrowed. "Claude Code with everything" is the Studio's master pane (agent
isolation D4), which stays as it is. Mechanism, settled by CP0: an own `CLAUDE_CONFIG_DIR` under
`~/.axiomata/claude/` if the subscription login survives there; otherwise the owner's login with `--setting-sources`
without `user`, `--strict-mcp-config`, the app's own `--settings` file, and no write into `~/.claude`. Either way CP0
also checks that the owner's global `CLAUDE.md` stays out.

## Checkpoints

| CP | What |
|---|---|
| **CP0** | Measure Claude Code 2.x headless: login under an own `CLAUDE_CONFIG_DIR`; `--setting-sources`; `--strict-mcp-config` with `~/.axh/mcp.json`'s servers; the `result` event's token/cost fields; `--resume`; `--output-format json` for `output: json`. Findings into this file, D5's mechanism chosen. |
| **CP1** | `HarnessId`, the two config fields, `harness_for`, `validate_for_save` matrix, skill `backend` optional + runner default, `execute_prompt` default. Tests: resolution order, matrix, old config round-trip. |
| **CP2** | Claude Code runner for skills and routines, read access from the shared list, `runs.backend = claude_code`. Tests: argument building (MCP file, allowed tools, read rules), stream-json parsing; one `#[ignore]`d live run. |
| **CP3** | Chat on Claude Code: dispatch, `--resume`, module context, instruct mode. |
| **CP4** | Glut chat: JSONL sessions, resume, dispatch. |
| **CP5** | Settings: a "Harness" select per role above the provider, providers filtered by the matrix (`Settings.svelte`, `core/backend.ts`, `devmock.ts`); bundled skills without `backend: opencode` + reseed note; `docs/architecture.md` §5 "Model providers", `AGENTS.md` backend/provider traps. |

After CP5: the owner's check "is everything in the harness backend" — harness.md's open list (sessions done by CP4;
streaming, the Anthropic client, cost metering, token cost) against what the settings now promise.
