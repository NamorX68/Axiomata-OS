# Harness choice — Opencode, Claude Code or Glut, per role

Status: **in progress 2026-10-19**, owner decisions D1–D7 taken, CP0 measured, CP1–CP2 built.

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
| Glut (`axh`) | — (D6) | ✓ | ✓ | CP4 | ✓ (HS6b) |

`AgentDefaults::validate_for_save` refuses a role whose provider its harness cannot serve, with a message naming both;
the Settings dialog only offers the providers that fit the chosen harness. A config edited by hand into a bad pair is
not "fixed" on load — the run records a clear `Failed` (as HS6b's B1: never a silent hop to another harness).

## Design

- **Config.** `pub enum HarnessId { Opencode, ClaudeCode, Glut }` (`opencode` | `claude_code` | `axh` in TOML — `axh`,
  not `glut`, so it is the same id as the skill frontmatter's and `runs.backend`'s). `AgentDefaults::harness_for(role)`
  beside `provider_for(role)`.
- **Skills.** `registry` keeps `backend: Option<String>` (no `default_backend()` any more); the runner resolves
  `skill.backend.unwrap_or(config.agents.skill_harness)`. Routines' `prompt` target (`execute_prompt`) takes the same
  default — it used to be the local `ollama` completion, so a stored prompt routine without a backend moves to the
  skill harness and its provider (the owner had none, 2026-10-19; the daily cap still holds). Between CP1 and CP5 the
  dialog has no harness control: a hand-set harness that does not fit a provider picked in the dialog makes the save
  refuse with the matrix message. `runs.backend` keeps recording the harness that really ran (`claude_code` new).
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

## Decided later (owner, 2026-10-19)

- **D6 — Glut gets no Anthropic client in this plan** (owner, 2026-10-19): later, with the owner's check of the
  harness backend. The matrix shows `—`, the setting says why.
- **D7 — signing in to Anthropic from the app** (owner, 2026-10-19). Settings show the login state of the app's
  Claude Code folder (`claude auth status`, JSON) beside the Claude Code harness, with a **Sign in** button that runs
  `claude auth login --claudeai` under `CLAUDE_CONFIG_DIR=~/.axiomata/claude` (the browser opens; if the CLI asks for a
  code, a field passes it to the process's stdin) and **Sign out** (`claude auth logout`). The command line is fixed in
  Rust — the webview only says "sign in"/"sign out", never a command or a folder (`AGENTS.md`: nothing stored or sent
  by the webview starts a process). At app start, when a role's harness is Claude Code and the folder is not logged
  in, a notice offers the same sign-in. A full first-run setup (workspace, providers, keys) is not part of this plan.

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
| **CP5** | Settings: a "Harness" select per role above the provider, providers filtered by the matrix (`Settings.svelte`, `core/backend.ts`, `devmock.ts`); the Anthropic sign-in (D7: status, Sign in/out, the start notice; `security-auditor`, a new process-starting Tauri command); bundled skills without `backend: opencode` + reseed note; `docs/architecture.md` §5 "Model providers", `AGENTS.md` backend/provider traps. |

After CP5: the owner's check "is everything in the harness backend" — harness.md's open list (sessions done by CP4;
streaming, the Anthropic client, cost metering, token cost) against what the settings now promise.

## CP0 — measured (Claude Code 2.1.303, 2026-10-19)

- **Own `CLAUDE_CONFIG_DIR` = not logged in.** `claude -p` under an empty config folder answers `is_error: true`,
  `"result": "Not logged in · Please run /login"` (`terminal_reason: api_error`, 39 ms, nothing billed). The
  subscription login is per config folder.
- **The owner's login with `--setting-sources "" --strict-mcp-config` works and keeps the owner's setup out:** the
  global `CLAUDE.md` is not in the context (asked for its line-length / `cargo clippy` rule: unknown), no `mcp__`
  tools, no plugin tools (`LSP` absent); reading `~/.claude/CLAUDE.md` by tool was refused (no allow rule). JSON
  result carries `session_id`, `total_cost_usd`, `usage` (input/output/cache tokens), `num_turns`, `is_error`,
  `permission_denials`.
- **But it writes into `~/.claude`:** the session transcript (and an empty `memory/`) land in
  `~/.claude/projects/<cwd-slug>/` — the vault's chats would show up in the owner's own `claude --resume` list.
  `--no-session-persistence` stops that, but also stops `--resume`, which the chat needs.
- Flags present: `--append-system-prompt[-file]`, `--resume <id>`, `--no-session-persistence`, `--strict-mcp-config`,
  `--setting-sources`, `--settings`; `claude setup-token` makes a long-lived subscription token
  (`CLAUDE_CODE_OAUTH_TOKEN`).
- **D5 mechanism chosen (owner, 2026-10-19): own `CLAUDE_CONFIG_DIR=~/.axiomata/claude`, logged in once** by the owner
  with `CLAUDE_CONFIG_DIR=~/.axiomata/claude claude auth login --claudeai` (works from a `!` line; the token lives in
  the keychain under that folder, Claude Code refreshes it). The owner's own login stays as it was.
- **Measured on that folder** (Haiku 5.5, cleared env, `--strict-mcp-config --mcp-config ~/.axh/mcp.json` — the file
  is already Claude's `mcpServers` shape, usable as it is): an MCP tool named in `--allowedTools`
  (`mcp__apple-reminders__reminders_lists`) ran without a prompt; the owner's global `CLAUDE.md` was not in the context;
  `--resume <id>` continued the conversation under the **same** `session_id`; asked for "only a JSON object" the
  `result` was exactly `{"codewort": "HEXGLUT"}` — so `output: json` skills can run on Claude Code (the runner still
  validates and records a `Failed` otherwise). Transcripts go to `~/.axiomata/claude/projects/`, nothing into
  `~/.claude`. Cost ~0.3 ct per Haiku turn (`total_cost_usd`, notional on the subscription).
- Consequence for CP2: the app's runs set `CLAUDE_CONFIG_DIR`; a run whose folder is not logged in (`result` "Not
  logged in") is a recorded `Failed` naming the login command above; Settings shows the login state
  (`claude auth status`, JSON) next to the Claude Code harness.

## CP2 — built and measured (2026-10-19)

`agents/claude_code.rs`; `AgentBackend::ClaudeCode` resolves from `claude_code`; the runner records it under provider
`anthropic` with Anthropic's skill model (or the frontmatter's), whatever the skill role's provider is.

- Measured on the app's folder before writing the rights: with the default setting sources the folder's `CLAUDE.md`
  is read and its `.claude/settings.json` **allow** rules are ignored with a notice on stdout ("this workspace has not
  been trusted") — so the result is the last JSON line, not the whole stdout. `--setting-sources ""` drops `CLAUDE.md`
  too; `--setting-sources project` keeps it. Unallowed shell, `Write` and an output redirect under
  `Bash(git log:*)` were refused; `Bash(git * --no-i*)`-style denies with an inner `*` hold.
- Live: a read probe (`access: read`) read `AGENTS.md`, ran `ls`, and was refused `Write` and `echo > f` — nothing
  written (run #1697). The reminders digest with `backend: claude_code` called its MCP tool and answered one clean
  JSON object (run #1698, Haiku 5.5, 33 s, 0.8 ct notional).
- **Review finding, measured: a folder's `.claude/settings.json` runs its hooks and sets its `env` even untrusted.**
  Under `--setting-sources project` a `SessionStart` hook of the folder ran and its `env.ANTHROPIC_BASE_URL` sent
  the call elsewhere. So the runner now reads **no** settings file (`--setting-sources ""`, hooks and env measured
  gone) and hands the folder's `AGENTS.md` (plus a caller's context) over itself as one
  `--append-system-prompt-file` — only the last of several counts, measured. The MCP config goes as a 0600 file in a
  0700 run folder under `~/.axiomata/claude/runs/`, removed after the run, never on the command line (`ps`). A part
  of `allowed_tools` must start with a letter (a variadic flag's values: `--permission-mode` would read as a flag);
  a read run drops declared shell rules; the child leads its own process group, killed at the limit and swept after
  the end; a result must be `type: result` and a `subtype` other than `success` fails the run. Re-run live: #1699
  (read probe, router known without reading, writes refused) and #1700 (reminders digest, MCP from the file).
- Open, low: `execute_and_record_skill` guards the daily cap by the skill role's provider before the backend is
  resolved, so a frontmatter `backend: claude_code` skill can be held by an OpenRouter cap it does not spend from.
- Glut still ignores `access: read` (the gate is the skill's `allowed_tools` alone); for the owner's later check of
  the harness backend.
