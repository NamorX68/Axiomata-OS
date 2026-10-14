# Axiomata's own harness (board card #37, M7.4)

Status: HS1–HS8 built 2026-10-13/14. Each further step is planned on its own and confirmed by the owner before it is built.

## Why

Every agent of Axiomata runs on a foreign harness today: Opencode (skills, routines, the Studio) and Claude Code
(the Studio). An own, lean harness should run skills with a small system prompt and little overhead, run Studio
agents that need no TUI, and be built so it can later be lifted out into an app of its own (working name "Glut" —
note the clash with GLUT/freeglut; check before publishing).

## Decisions (owner, 2026-10-13)

- **H1 — name.** Crate `axiomata-harness`, binary `axh`. The app's name comes when it is lifted out. `Harness::Mini`
  in the engine catalog is renamed when the harness reaches the Studio.
- **H2 — on its own.** The crate does not depend on `axiomata-core` (`cargo tree -p axiomata-harness | grep
  axiomata-core` is empty). It may use `axiomata-files`, later `axiomata-roster`. Configuration, keys, cost metering
  and permission asks come from the caller as values and traits.
- **H3 — form.** A library (the loop) plus the binary `axh`, which runs headless and writes events as JSON lines.
- **H4 — models.** First a client for the OpenAI-compatible `POST {base}/v1/chat/completions` with `tools`
  (OpenRouter, Ollama). Anthropic's own API follows as a second client behind the same trait. No streaming yet.
- **H5 — keys.** From `~/.axiomata/config.toml` (`providers[..].api_key` / `base_url`); the core side hands them in,
  `axh` reads its key from `AXH_API_KEY`, never from an argument. Never in a log (`Debug` redacted), event or prompt.
- **H6 — a small system prompt.** A few fixed lines (role, folder, date, "use the tools, answer briefly") plus the
  caller's instructions; tools describe themselves through the API's `tools` field.
- **H7 — permissions** (step HS3). Rules allow / deny / ask per tool, with patterns in the role-file style
  (`Bash(git status:*)`); "ask" goes through a trait to the caller — in Axiomata a permission request through
  `permission_asks` (#88), headless without an answer refused.

## Road map

Backend, one checkpoint and commit each:

| Step | What |
|---|---|
| **HS1** ✅ | Crate, OpenAI-compatible client, loop, events, file tools, `axh`, `axiomata-cli harness run` |
| **HS2** ✅ | Shell: `bash` (foreground and background), `bash_output`, `kill_shell` — S1–S6 below |
| **HS3a** ✅ | Permissions: rules, modes, settings files, asking (P1–P8 below) |
| **HS3b** ✅ | Sandbox for `bash`: macOS Seatbelt plus a network proxy with a domain list (B1–B6 below) |
| **HS4** ✅ | Slash commands: registry, Markdown command files, built-ins (`/help`, `/model`, `/clear`) |
| **HS5** ✅ | MCP client (stdio, JSON-RPC lines; template: the removed client of `54173bf`), tools as `mcp__server__tool` |
| **HS6** ✅ | Skills in the harness: SKILL.md parser, a skill as a run, the `skill` tool — K1–K6 below |
| **HS6b** ✅ | `backend: axh` in the core: skill runs through the own harness, `runs` table, spend metering — B1–B4 below |
| **HS7** ✅ | Hooks: SessionStart / PreToolUse / PostToolUse / Stop as shell hooks with JSON on stdin (Claude-compatible) — B1–B4 below |
| **HS8** ✅ | Subagents: a `task` tool starting a nested loop with its own prompt, tools and limits — B1–B3 below |

After the backend: sessions over several turns, streaming, the Anthropic client, cost metering (`studio_spend` /
`spend.rs`), `axh` as an engine in the Studio, a settings page — and the front ends: a **TUI** and an **app** of its
own, both on the event stream of H3.

## HS8 — subagents (decided 2026-10-14)

Full plan with tests and build order: `docs/plans/harness-hs8.md`. A `task` tool starts a nested run of the same
loop: the model says `task {"description": …, "prompt": …}` and gets the subagent's answer back as the tool's
result — one level deep, no parallel calls.

- **B1 [owner] — `task` is judged like a change.** The call is `Access::Other` under the tool name `task`: rules
  `allow/ask/deny task` first, then the mode (`default`/`accept_edits` ask and are refused headless, `auto`
  allows, `plan`/`dont_ask` refuse). Switched on by `allow task`, `--allow task`, `--permission-mode auto` or an
  interactive answer; a narrowed run still needs `task` declared, and a declaration alone never allows (HS6b-B3).
  No new `Access` variant.
- **B2 [owner] — the subagent gets exactly the parent's toolbox, minus `task`.** Same gate, model, hooks, sessions
  and sandbox — and one level only: a subagent that hallucinates `task` gets the ordinary
  `error: there is no tool named "task"`.
- **B3 [owner] — the core offers `task` only to skills that declare it.** `backend: axh` sets
  `RunConfig.subagents` from the skill's frontmatter (`allowed_tools` naming `task`, read with the new
  `Rule::names_tool`); the same declaration is the gate's whole permission set (HS6b-B3), so offering and allowing
  come from one reading of the frontmatter, and `~/.axh` rules never feed the core's gate. `axh` and the CLI keep
  the default (`true`) — there the gate alone decides (B1).

Defaults (planner, D1–D10 in the plan file): the tool lives in a private `task.rs` (`description` and `prompt`
both required, otherwise `error: the task needs a non-empty description and prompt`); the loop's `task` arm
replaces only the `call_until` step — judge first, the parent drains the shared gate log before the nested run and
wraps the subagent's leftovers as its own events, its usage is summed into the run's, a stop without an answer maps
to `error: the subagent stopped without answering: <reason>`, recursion through `Box::pin`; `run()` keeps its
signature behind `run_inner(..., depth)`; the subagent runs `PreToolUse`/`PostToolUse`/`Stop` but never
`SessionStart`, with its own `session_id`; its own `subagent_prompt`, the parent's `instructions` dropped; limits
`subagent_max_turns = 15` and `subagent_timeout = 5 min`, the nested timeout capped by the parent's remaining
time; `RunConfig.subagents` default `true`; every event of a nested run arrives wrapped as
`Event::Sub { depth, event }` (JSON `{"event":"sub","depth":1,"inner":{…}}`), the CLI indents `"  " × depth` and
prints a subagent's `Finished` as `↳ subagent · {turns} turns · {in}/{out} tokens`, the core's `Notes` prefixes
`subagent: `; `claude_alias("task") = Some("Task")`; `Rule::names_tool` is `pub`.

## HS6b — `backend: axh` in the core (decided 2026-10-14)

Full plan with tests and build order: `docs/plans/harness-hs6b.md`. The step is the allowed dependency direction
(core → harness; H2 stays: the harness knows nothing of core). A skill's `SKILL.md` with `backend: axh` runs its
prompt through `axiomata_harness::run` instead of an opencode session; the runner records it like every other run
(`runs.backend = "axh"`, no migration) and meters it from the token counts.

- **B1 [owner] — no silent fallback.** The endpoint is `config.agents.providers[agents.skill_provider]` as it is.
  A provider without a `base_url` (Anthropic, the current skill default — the harness has no client for it yet,
  H4) or a keyless OpenRouter fails the run with a message naming it: a recorded `Failed` run, never a hop back to
  opencode. A run says `axh` in `runs.backend` or it did not run.
- **B2 [owner] — MCP only from the harness's own config.** `~/.axh/mcp.json` + the project's `.mcp.json`, read via
  `mcp::config::load` — never `opencode.json`. Owner servers run without approval; a project server is never asked
  about headless (`permitted(…, can_ask=false)`, left out with the M2 note). Connector skills need their `apple-*`
  servers copied into `~/.axh/mcp.json` once, by the owner.
- **B3 [owner] — unattended, deterministic permissions.** The gate is built fresh per run: `Mode::Default`, asker
  `Nobody`, and the skill's declared `allowed_tools` (parsed with the harness's own `rules_of`) as both its allow
  rules and its narrowing (`restrict` + `restrict_name` = the skill's name, so a refusal reads
  `the command /mail-digest allows only: …`). What the skill names is allowed, the rest refused before anything
  is asked; a skill that declares nothing keeps mode default (reads free, changes ask and are refused). The
  `~/.axh` permission rules and mode do **not** feed the gate; `settings::load` still word on the sandbox only
  (settings can only narrow).
- **B4 [owner] — no bundled skill switches.** `backend: axh` is opt-in per `SKILL.md`, written by the owner.

Defaults (planner, all built as written): **D1** the core keeps building the prompt (`build_prompt_at` → the run's
user message; the harness's `Skill::start_run` is *not* used — no double inlining; `RunConfig.instructions` stays
`None`). **D2** the model travels **bare** (frontmatter `model:` else `skill_model`, no `provider/` prefix —
`resolve_run_model`'s `Axh` arm), so `metered_cost_usd` and `runs.model` key on it unchanged; a skill on axh names
its model as the API does. **D3** no JSON-repair turn on axh (`output: json` skills stay on opencode). **D4**
`system_prompt_file` ignored (as opencode's run path ignores it). **D5** the shared 4-slot `agent_slots()` taken
before tools/MCP. **D6** `AgentRequest::timeout` into `RunConfig.timeout`; `FinishReason::Timeout` →
`AgentTimeout { backend: "axh" }` → the usual `Failed` record. **D7** one collector sink keeps `Event::Error`,
failed MCP servers and refused calls → joined into `stderr`; reply → `stdout`, usage → token columns, turns →
`num_turns`. **D8** `provider_label(Axh)` = the skill provider (it bills through that key — the daily cap keeps
working). **D9** `runs.backend` verbatim `"axh"`.

## HS7 — hooks (decided 2026-10-14)

Full plan with tests and build order: `docs/plans/harness-hs7.md`. Claude Code's four events — `SessionStart`,
`PreToolUse`, `PostToolUse`, `Stop` — configured in the owner's `~/.axh/settings.json` in Claude's shape, their
payload as JSON on stdin, their answers Claude's JSON (or the exit codes).

- **B1 [owner, rev. 2026-10-14] — owner hooks only.** Hooks come from `~/.axh/settings.json` alone. A project's
  hooks are parsed — a settings file holding them must not fail the load — but never run: the repo rule "nothing
  a stored file says may start a process" holds for hooks. `Loaded.project_hooks_skipped` carries the fact; the
  CLI prints a stderr note, the core folds the same note into the run's `stderr` through `Notes`.
- **B2 [owner, rev. 2026-10-14] — hooks veto, the gate decides.** A `PreToolUse` hook may only block (`deny` or
  exit 2 → `error: <reason>` as the tool result, nothing runs). `allow` and `ask` parse faithfully and are
  no-ops: rules, mode and `allowed_tools` stay the sole decider, HS6b-B3's determinism untouched, no bypass path
  in the `Toolbox`.
- **B3 [owner] — fail-open.** A crash, an exit other than 0/2 or a timeout is an `Event::Hook` note and the run
  goes on; only an explicit block blocks. The run loop itself never fails because of a hook.
- **B4 [owner] — unattended core runs execute the owner's hooks** (`backend: axh`): with B2+B3 a hook can only
  make a run stricter or leave it unchanged, never looser. The gate construction in `agents/axh.rs` is untouched.

Defaults (planner, D1–D8 in the plan file): the config types are Claude's shape — only `type: "command"`, unknown
event names ignored, unknown entry fields tolerated, an unparsable matcher or an unknown type a
`HarnessError::Settings` at load, 60 s default timeout; a matcher is an unanchored regex against the tool's name
**and** its Claude alias (`bash` → `Bash`, `read_file` → `Read`, `list_dir` → `Ls`, …; MCP tools keep their
`mcp__server__tool` spelling; `Stop` has no matcher); the payload carries `session_id` (`axh-<pid>-<epoch-ms>`),
`transcript_path: ""`, `cwd`, `hook_event_name` and the event's own fields (`source` / `tool_name` +
`tool_input` (+ `tool_response`) / `stop_hook_active`); hooks run through one lazy per-run `Session` (shell,
snapshot, cleaned env, sandbox, network proxy) — a session that cannot be built is an `Event::Error` and the run
goes on without hooks; `SessionStart` context becomes a system message right behind the harness's own; a blocked
call gets `error: <reason>` instead of running, a blocked `PostToolUse` replaces what the model gets (the
`ToolResult` event already shows what the tool really answered), a blocked `Stop` feeds the reason back as a user
message and takes another turn with `stop_hook_active: true`; the effective limit is `min(hook timeout, time
left to the run's deadline)`, cancel honoured by dropping the wait (which kills the hook's process group).

## HS6 — skills (decided 2026-10-13)

- **K1 — two ways to use a skill.**
  1. *A skill as a run:* `axh run --skill <name> [-- "extra prompt"]` and `axiomata-cli harness run --skill <name>`.
     The `SKILL.md` body becomes the run's instructions (appended to the small system prompt of H6); the prompt is
     the extra text, or `"Run the skill."` when there is none.
  2. *Like Claude Code:* in every run the model sees every available skill's `name` and `description` in the
     description of one tool, `skill`; calling `skill {"name": "…"}` returns that `SKILL.md`'s body and the list of
     files next to it (K4) as the tool result, and the model follows it. Loading text is no action
     (`Access::Own`): what the model then does is judged as usual.
- **K2 — the caller says where skills are.** The harness itself knows `~/.axh/skills/` (the owner's) and
  `<project>/.axh/skills/` (only in a trusted folder, as the project's allow rules). A caller adds more folders:
  `RunConfig.skill_dirs`, `axh` a repeatable `--skill-dir <path>`; `axiomata-cli harness run` passes
  `~/.axiomata/skills` (it knows where that is; the harness does not). Each skill is a folder
  `<dir>/<name>/SKILL.md`. On equal names: project beats caller folders beats the owner's — the more specific place
  wins, as with commands.
- **K3 — frontmatter.** YAML (`gray_matter`, as the roster reads `AGENT.md`). Read: `name`, `description` (both
  required — a skill without them is skipped with a note on stderr, never fails the run), `model`, `allowed_tools` /
  `allowed-tools` (a list or one space- or comma-separated string of rules, e.g. Axiomata's `allowed_tools:
  mcp__apple-mail__…`), `timeout_secs`, `prepend_files`. Every other key (`backend`, `output`, `license`,
  `metadata`, …) is ignored: skill files come from other tools too. `name` must equal the folder name and match
  `[a-z0-9-]{1,64}`; otherwise skipped with a note. Limits (named consts): 200 skills, 256 KiB per `SKILL.md`, 8 KiB
  of frontmatter, 200 files listed per skill, 3 folders deep.
- **K4 — files next to a skill.** A skill folder may hold more files (scripts, references). The `skill` tool's
  result ends with a list of them as `skill:<name>/<rel>` paths, and the file tools understand those as a second
  read-only root per skill **outside** the working folder (`read_file`, `list_dir`, `grep`, `glob`), each through
  `axiomata_files::Root` with `LinkPolicy::Contained`. Never writable, whatever the run's rules allow. The `Read`
  rules judge them as `skill:<name>/<rel>`.
- **K5 — what a skill's frontmatter does in a run** (a skill *run*, K1.1, only):
  - `allowed_tools` narrows the run like a command's (`Gate::restrict` of HS4), never widens; a skill loaded
    through the `skill` tool does not change the rules (Claude Code's model-invoked skills behave the same: the
    existing permissions decide).
  - `model` overrides the run's model; ignored when loaded through the tool.
  - `timeout_secs` becomes the run's overall timeout, capped by the existing maximum.
  - `prepend_files: ["rel/path.md"]` (Axiomata's bridge, `AGENTS.md` "Backend ids"): each file relative to the
    **working folder** is inlined before the body as `## Context file: <path>` + content; a missing file — with a
    missing folder in front of it — is skipped; an absolute path or one with `..` fails the run with a clear error;
    read through a contained root, capped at 256 KiB per file.
- **K6 — Axiomata's core is not touched in HS6.** `backend: axh`, runs in the `runs` table and spend metering are
  step HS6b, planned on its own. HS6 only adds `--skill`/`--skill-dir` to the CLI's existing `harness run`.

## HS5 — MCP (decided 2026-10-13)

- **M1 — servers from the owner and the project, like Claude Code.** `~/.axh/mcp.json` (the owner's) and
  `<project>/.mcp.json` (Claude Code's file, so a repo's servers work as they are). Format of both:
  `{"mcpServers": {"<name>": {"command": "…", "args": […], "env": {…}}}}` for stdio and
  `{"mcpServers": {"<name>": {"type": "http", "url": "https://…", "headers": {…}}}}` for HTTP. Unknown fields →
  error (`deny_unknown_fields`), except `type: "stdio"` which is the default. Server names `[a-zA-Z0-9_-]{1,32}`.
  On a name in both files the owner's wins.
- **M2 — a project's server runs only after the owner approved *that* server.** A project's server starts
  processes or sends data out, so trusting the folder is not enough: each server is approved on its own, keyed by
  project path + server name + SHA-256 of its JSON config; a changed command, argument, env or URL asks again.
  Approvals live in `~/.axh/mcp-approved.json` (written atomically, mode 0600, like `Trust` in `settings.rs`). The CLI
  shows the server's command line / URL (through `permissions::printable`) and asks y/n; headless (`axh`, no
  terminal) skips unapproved project servers and says so on stderr. The owner's own servers need no approval.
- **M3 — transports stdio and HTTP.**
  - stdio: the server is a child process; JSON-RPC 2.0, one message per line on stdin/stdout; stderr discarded.
    Environment: the cleaned tool environment (`axiomata_files::toolenv`) plus the server's `env`. Its own process
    group, killed with the run (`kill_on_drop`, as in `tools/shell.rs`).
  - HTTP ("Streamable HTTP"): every message is a `POST` to `url` with `Content-Type: application/json` and
    `Accept: application/json, text/event-stream`. The answer is either JSON or an SSE stream (`text/event-stream`):
    read `data:` lines until the response with the request's `id` arrives. Keep the `Mcp-Session-Id` header from the
    `initialize` answer and send it on every later request. No OAuth in HS5: `headers` values may
    contain `${VAR}`, replaced from the harness's own environment (missing variable → that server fails to start
    with a clear message); the URL must be `https://` or loopback `http://` (same rule as `model.rs` uses for keys).
    Same 16 MiB answer cap and no redirects as `model.rs`.
- **M4 — servers run outside the sandbox, like Claude Code.** They are configured by the owner (or approved,
  M2) and often need the network or apps (`apple-mail`). What protects the owner is the permission on their tools
  (M5).
- **M5 — permissions like Claude Code.** A tool appears to the model as `mcp__<server>__<tool>`. Rules:
  `mcp__server__tool` (one tool) or `mcp__server` (every tool of that server), in allow / ask / deny like any rule,
  and taking no pattern (`mcp__mail(send)` would silently match nothing). Without a rule: `auto` allows,
  `default` and `accept_edits` ask, `plan` and `dont_ask` refuse.

## HS4 — slash commands (decided 2026-10-13)

- **C1 — where the files are, and who may put them there.** `~/.axh/commands/**/*.md` (the owner) and, in a folder
  the owner trusted once (P5), `<project>/.axh/commands/**/*.md`. Read through `axiomata_files::Root` with
  `LinkPolicy::Contained`, rooted at the `commands` folder itself, so a link cannot lead out of it. At most 200
  files, 64 KiB each, 3 folders deep; beyond that — as with a name that is no command name — the run does not start,
  rather than running with a half-known command list. A project's command of a name the owner already has wins; a
  built-in always wins, and the file shadowed by one is skipped, no error. A name is `[a-z0-9_-]` per segment (lower
  case), segments joined by `:` — `git/commit.md` is `git:commit`.
- **C2 — the format.** An optional `---` block of `key: value` lines, then the body. Known keys: `description`,
  `argument-hint`, `allowed-tools` (comma-separated rules, e.g. `Bash(git status:*), Read`), `model`; an unknown key
  is refused, in the spirit of the settings files' `deny_unknown_fields`. Values may be wrapped in one pair of
  `"…"`, which is stripped. In the body only `$ARGUMENTS` (the whole rest, trimmed) and `$1`, `$2`, … (its
  whitespace-separated words, a missing one empty) are replaced; nothing else is.
- **C3 — `allowed-tools` only narrows.** The gate takes the command's rules as its `restrict`: a call those rules do
  not name is refused (`the command /name allows only: …`), and a call they do name is judged as usual against the
  run's own rules and its mode — such a rule never allows anything by itself. `bash_output` and `kill_shell` stay
  always allowed. So `allowed-tools: Read` refuses `bash` and `write_file`, and `allowed-tools: Bash(git status:*)`
  in `plan` mode still has `git status` refused by `plan`.
- **C4 — what is deliberately not there yet.** `!command` lines in a body are refused by name (`/name uses !command
  lines, which the harness does not run yet`): they would need the gate and the sandbox. An `@path` in the body is
  left in the text, not read — the model can call `read_file`.
- **C5 — the built-ins are code, not files.** `/help` lists every command (name, source, argument hint,
  description); `/permissions` shows the run's mode, its allow / ask / deny rules and the sandbox's four settings,
  each line through `permissions::printable`. Both are answered without a model. The names `clear`, `model` and
  `compact` are reserved — they answer that they are not available in a single run yet, so no file can take them
  before the TUI brings them.
- **C6 — headless expansion, and the `command` event.** `/name …` is expanded after the gate is built (same home,
  folder and trust): no slash is an ordinary prompt, an unknown name or a `!` body is an error without a model call,
  a built-in is text for the front end, a file command is the run's prompt — with its `model` where it names one and
  its `allowed-tools` narrowing the gate. `Event::Command { name, source, prompt_chars }` is emitted before the first
  model call, tagged `event` like every other event.

## HS3b — the sandbox (decided 2026-10-13)

- **B1 — Seatbelt, like Claude Code and Codex.** Every `bash` command runs under `sandbox-exec -p <profile>`, the
  profile built per run from resolved paths (`/tmp` is `/private/tmp`) and passed as an argument, so nothing on disk
  can change it between two calls. Where there is no `sandbox-exec` (not macOS) the rules of HS3a alone judge, and the
  CLI says so. Linux (bubblewrap/Landlock) comes when the harness runs there.
- **B2 — writing.** Only the working folder, the temporary folders (`$TMPDIR` and its parent, `/private/tmp`), the
  package caches (`~/.cargo/registry|git`, `~/.npm`, `~/.cache`, `~/Library/Caches`, …) and the repository's git
  folders. Never the protected folders and files of the working folder (`.git` aside, P5's list), never git's
  `config`, `config.worktree`, `hooks`, `info` — code or settings the owner's own git would run — neither of the
  repository nor of a submodule, another worktree or any git folder below the working folder. No `.git` — folder or
  file, at any depth — and no git folder below one (`modules/…`, `worktrees/…`) is created, moved or removed: a
  prepared folder moved into place would bring its own config, since Seatbelt judges a rename by its target only
  (so `git init`, `git submodule add` and `git worktree add` fail in the sandbox, by design). No `.envrc` at any
  depth. Commits work.
  Accepted on purpose: the package caches are writable, so a command could plant something a later build of the
  owner's picks up (cargo does not re-check an unpacked crate) — the price of builds that work in the sandbox.
- **B3 — reading.** Everything, except the owner's secrets: keys and credentials (`~/.ssh`, `~/.aws`, `~/.gnupg`,
  `~/.netrc`, `~/.git-credentials`, `~/.npmrc`, `~/.pypirc`, cargo's credentials, gcloud/Azure/Terraform, keychains,
  1Password, other agents' credentials, …), Axiomata's and the harness's own configuration (`~/.axiomata/config.toml`
  holds API keys, `~/.axh`), shell histories, browser, mail, Slack and Discord data, and every `.env`, `.env.*`,
  `*.env` and `.envrc*` in the working folder.
- **B4 — the network, through a proxy with a domain list, like Claude Code.** A sandboxed command reaches nothing
  but the run's own proxy on `127.0.0.1` — not the other services on this machine (the Opencode service, a database,
  a dev server) — and `HTTP(S)_PROXY`/`ALL_PROXY` (both spellings) point it there; proxy and profile start with the
  first sandboxed command. The owner can open localhost (`"allowLocalBinding": true`, like Claude Code: a test's own
  socket, a dev server), which opens every local service with it. For each connection the proxy asks the gate about the host — rules `Network(crates.io)`
  (one host) or `Network(*.github.com)` (everything below, not the domain itself), then the mode: `auto` lets a host
  through, `default`/`accept_edits` ask, `plan`/`dont_ask` refuse — and answers 403 with the reason when refused.
  HTTPS is a `CONNECT` tunnel (the proxy sees the host, never the content); plain HTTP is forwarded one request per
  connection, with the judged host as `Host` (no fronting another site on the same server). Names are resolved by the
  proxy, outside the sandbox, and an address on this machine, the local network or link-local (a cloud's metadata
  service) is never connected to, whatever name led there. Every judgement is a `permission` event like any other; an
  ask whose command has ended is dropped. At most 64 connections at once, 30 s for a request head and a connection.
- **B5 — signals and other applications.** A command signals only processes of its own sandbox, never the owner's
  others; it cannot start `osascript`, `osacompile`, `open`, `automator` or `shortcuts` (`tell app "Terminal" to do
  script` would run anything outside).
- **B6 — the escape: `dangerously_disable_sandbox`, always asked.** The `bash` tool takes the flag; such a call is
  asked in every mode, whatever a rule allows, and refused where nothing can be asked. The owner may switch the
  sandbox off, add writable places or open localhost in `~/.axh/settings.json` (`"sandbox": {"enabled", "allowWrite",
  "denyRead", "allowLocalBinding"}`); a project's settings may only add `denyRead`. A sandbox switched on but
  impossible to set up (no absolute `HOME`) stops the run; where the system has none, `axh` and the CLI say so.

## HS3a — permissions (decided 2026-10-13)

- **P1 — rules in Claude Code's format.** `Tool` or `Tool(pattern)` in three lists, `allow`, `ask`, `deny`; across
  every source `deny` beats `ask` beats `allow`. `Bash(cargo test:*)` is a command prefix (`:*` = followed by
  arguments or nothing), `*` matches anywhere (`Bash(git * push *)`); `Read(src/**)` / `Edit(**/*.rs)` are globs
  relative to the working folder. `Read` covers `read_file`, `list_dir`, `grep`, `glob`; `Edit` (and `Write`) covers
  `write_file` and `edit_file`; `Bash` covers `bash`. A role's existing `permissions` (`AGENT.md`) are such rules.
- **P2 — reading a command (reworked after two security reviews).** A small lexer reads the command as the shell
  would — quotes, escapes, `&&`/`||`/`;`/`|`/`&`/line breaks, redirections, `2>&1`. Whatever it cannot vouch for makes
  the command *opaque*: a comment, a here-document, any `$` expansion (`$X`, `$(…)`, `${…}`), backticks,
  `<(…)`/`=(…)`, `$'…'`, grouping and braces, `!`, keywords (`if`, `for`, `repeat`, …), a command word that is not
  literal (`=rm`, a glob, `~/…`), builtins that run or define other commands (`eval`, `source`/`.`, `trap`, `alias`,
  `export`, …), a shell running a script or its input, `sh -c '…'`, git's own configuration (`git config`, `-c core.*`),
  a carriage return, a redirection without a command. An opaque command is never allowed by a rule nor by `auto` — it
  is asked (refused where the mode asks nothing). Deny and ask rules are held against every part as words — with
  assignments in front (`X=`, `X+=`, `a[0]=`) dropped, every word sequence after a wrapper (`env`, `sudo`, `command`,
  `xargs`, `noglob`, `nocorrect`, `-`, `nohup`, `time`, `nice`, `timeout`, `exec`, `stdbuf`, …), the command's folder
  (`/bin/rm`), git's global options (`git -C . push`), inside `sh -c` strings; the command word without case on macOS
  (`RM` is `rm`). What a command writes (`>`, `>>`, `&>`) — or reads (`<`) — is held against the protected names, also
  through a glob (`.ax?`), refused; a write outside the folder is asked; a write inside it lands where the shell is
  (after an earlier `cd`) and is held against the `Edit` rules; a `cd` together with a write is asked. A part with
  `NAME=value` in front is never allowed by a rule. Best effort, the sandbox of HS3b holds the rest: `find -delete`, a script, a program's own
  config still do more than their words say; `Read` rules bind the file tools, not what a command reads (as in Claude
  Code).
- **P3 — modes, like Claude Code.** `default`: reads free, edits and commands asked; `accept_edits`: edits free too,
  commands asked; `plan`: reads only, edits and commands refused (an allow rule still lets a command through);
  `dont_ask`: whatever no rule allows is refused, nothing is asked — an `ask` rule refuses too; `auto`: whatever no
  rule denies is allowed (the owner's auto mode of #89), except an opaque command or one naming a protected path,
  which is asked. `plan` never writes, whatever a rule says, and never asks. No bypass mode. `bash_output` and `kill_shell` touch only the run's own jobs: always
  allowed.
- **P4 — settings files, the harness's own.** `~/.axh/settings.json` (the owner's), `<project>/.axh/settings.json`
  (checked in) and `<project>/.axh/settings.local.json` (not checked in), each
  `{"permissions": {"allow": [], "ask": [], "deny": [], "defaultMode": "default"}}`. A file that does not parse — an
  unknown field, a misspelled rule name (`bash(…)`), more than 1 000 rules — stops the run: a deny list silently
  ignored would be worse than no run. The mode comes from the flag, else local, project,
  user, else `default`.
- **P5 — trusting a project, like Claude Code.** A project may always tighten: its `deny` and `ask` rules count in
  every folder. What loosens — its `allow` rules and its mode — counts only in a folder the owner trusted once
  (`~/.axh/trusted.json`), and its mode never past the owner's own (`plan` < `dont_ask` < `default` < `accept_edits` <
  `auto`). The CLI shows what trusting would add, then asks; without a terminal it is left out, and the run says so.
  Agents never write `.axh/` — neither with the file tools (any case, also through a link) nor with a redirection.
- **P6 — who answers an ask.** The CLI asks in the terminal (`y` / the word `always` for this run / no; the command
  shown as plain text, at most 300 characters plus a count, invisible Unicode removed); headless (`axh`, no
  terminal) refuses. The app answers through `permission_asks` (#88) once the harness runs in the Studio, with an
  agent's identity — the trait is there for it. "Always" adds the rule for the rest of the run only.
- **P7 — the flags.** `--permission-mode`, `--allow`, `--ask`, `--deny` (repeatable) on `axh run` and
  `axiomata-cli harness run`; `--allow-bash` goes — the shell is always offered and its calls are judged.
- **P8 — visible.** Every judged call is an event (`permission`: tool, what, allowed / asked / refused, and why: which
  rule, the mode, or the answer), so a log, the TUI or the Studio can show why something did or did not run.

## HS2 — the shell (decided 2026-10-13)

- **S1 — off unless asked for.** Until the permissions of HS3 exist, the shell tools are offered only when the caller
  says so (`axh --allow-bash`, `axiomata-cli harness run --allow-bash`); the shell reaches whatever the owner's user
  reaches. With HS3, rules take the switch's place.
- **S2 — the owner's shell, with a profile snapshot, like Claude Code.** `$SHELL` (zsh on the Mac; `/bin/sh` when unset
  or unknown). Once per run, an interactive login shell writes a snapshot of the profile — functions, aliases, `PATH` —
  and every command sources it, so a command finds what the owner's terminal finds without `~/.zshrc` running each
  time. The profile is the owner's own code; it runs once per run. If the snapshot fails, commands run without it.
- **S3 — a fresh process per command, the folder carried over.** Each call is a new shell; after it the harness reads
  where it ended (`pwd -P`) and the next call starts there — but only inside the run's folder: a `cd` outside is set
  back, and the model is told so. Variables do not carry over.
- **S4 — limits.** 2 minutes per command by default, at most 10; stdin is empty (nothing waits for input); stdout and
  stderr together, cut like every tool result; the exit code always comes back, and a non-zero one marks the call as
  failed. On a timeout, a stop or the end of the run the whole process group is killed, children included.
- **S5 — environment.** The cleaned tool environment of `axiomata-files` (`toolenv`, the one the language servers get)
  plus the snapshot's `PATH` — no stray variables, so not the harness's own key either.
- **S6 — background jobs.** `bash` with `run_in_background` starts a job and returns its id at once; `bash_output`
  reads what it wrote since the last look and whether it still runs; `kill_shell` ends it. At most 8 jobs per run,
  the last 1 MiB of each job's output kept; every job ends with the run.

## HS8 — what exists

- `task.rs` (new, private module): `NAME`, `spec()` (the `description` + `prompt` JSON schema) and `prompt_of`
  (both required, empty refused).
- `config.rs`: `DEFAULT_SUBAGENT_MAX_TURNS` (15), `DEFAULT_SUBAGENT_TIMEOUT` (5 min) and the `RunConfig` fields
  `subagents` / `subagent_max_turns` / `subagent_timeout` with those defaults (`subagents: true`).
- `run.rs`: `run()` delegates to `run_inner(..., depth)` — depth 0 offers `task` (when `config.subagents`) and
  runs `SessionStart`, depth ≥ 1 speaks `subagent_prompt` and has neither; `run_task` judges the call
  (`Access::Other`, event first), drains the shared gate log *before* the nested run, copies the config
  (`instructions: None`, its own limits, timeout capped by the parent's remaining time), runs
  `Box::pin(run_inner(..., depth + 1))` through a `SubSink`, drains the subagent's leftovers wrapped, sums the
  usage into the run's and maps the outcome back to a tool result; `SubSink` wraps every event in `Event::Sub` at
  its depth.
- `event.rs`: `Event::Sub { depth: u32, event: Box<Event> }` — serde names the box `inner`.
- `prompt.rs`: `subagent_prompt(cwd, date)`; `hooks.rs`: `claude_alias("task")` = `"Task"`;
  `permissions/mod.rs`: `Rule::names_tool` (`pub`); `tools/mod.rs`: `cap` `pub(crate)` (the subagent's answer is
  cut like any tool result).
- Core `agents/axh.rs`: `declared_rules` parses the frontmatter once, for `gate_for` and for
  `run_config.subagents = declared.iter().any(|rule| rule.names_tool("task"))` (B3); `Notes` folds a `Sub` event
  into its line as `subagent: <line>`.
- CLI `harness_cmd.rs`: `render`/`error_of` split — `Sub` events print indented by depth, a subagent's `Finished`
  as `↳ subagent · …`, a `Usage` stays unprinted and errors stay stderr's (also inside a `Sub`, keeping their
  indent).
- Tests: the arguments' shape answering without a nested call; the happy path (answer, `Sub` events, exactly one
  unwrapped `Finished`, usage summed 27/5, `task` offered to the parent's turns only, the parent's instructions
  absent from the subagent's conversation); B1's nobody/deny/allow trio; the narrowed run (not declared →
  refused, declared → still judged, declared + allowed → runs); the owner's `PreToolUse` veto holding inside the
  subagent with `SessionStart` exactly once; turn limit (1) and time limit; a cancel from inside the subagent
  ending the whole run; `subagents: false`; the CLI's indent and `↳` line; and the core's request body carrying
  `task` only for a skill that declares it.

## HS7 — what exists

- `hooks` (new module, `crates/axiomata-harness/src/hooks.rs`): the Claude-shaped config types (`HooksSection`,
  `Entry`, `CommandEntry` — unknown event names kept but never run, unknown entry fields ignored, only
  `type: "command"`), `Hooks::compile` (matcher regex and type checked at load, 60 s default timeout, the reason
  plain for `settings.rs` to wrap in `HarnessError::Settings`), `claude_alias`, `payload`/`tool_input`, and
  `run_event`: configured order, the first block short-circuits the event, every hook reported as `Event::Hook`.
  `answer_of` reads Claude's channels — `hookSpecificOutput.permissionDecision` (deny blocks, allow/ask are
  no-ops), the legacy top-level `decision`, exit 2's stderr as block reason (context for `SessionStart` instead),
  plain stdout as `SessionStart` context; anything else a note (B3).
- `tools/shell.rs`: `Session`/`Session::new` and a new `run_hook` are `pub(crate)` — the payload written to a
  piped stdin from its own task, stdout and stderr kept apart (they mean different things to a hook), no folder
  carry-over, `GroupGuard` + timeout + stop flag like any command, the run's proxy through
  `Sandboxing::On(gate)`.
- `permissions/settings.rs`: `File.hooks` (so the key parses under `deny_unknown_fields`), `Settings.hooks`
  compiled at read, `Loaded.hooks` = the owner's only and `Loaded.project_hooks_skipped` (B1);
  `permissions::build_gate` returns `(gate, untrusted_project, sandbox, hooks, project_hooks_skipped)`.
- `event.rs`: `Event::Hook { turn: Option<u32>, hook_event, command, blocked, context_chars, note }` — `turn` is
  `null` for `SessionStart`, `note` carries a fail-open failure *or* the block reason (D6, so a record shows why
  a call never ran). `config.rs`: `RunConfig.hooks` + `RunConfig.sandbox`.
- `run.rs`: the four integration points around the loop's own steps (D3) — `SessionStart` after `Started`,
  `PreToolUse` before the gate (a blocked call gets `error: <reason>` and no gate judgement), `PostToolUse` after
  the tool, `Stop` on an empty reply — plus the lazy `HookSession` (D8, one session per run, build failure =
  `Event::Error` + run on) and `hooks_event` (the run's deadline and cancel honoured around every event).
- Callers: `bin/axh.rs` and `axiomata-cli/src/harness_cmd.rs` destructure the five-element tuple, print the
  *project hooks skipped* stderr note (B1) and put hooks + sandbox into `RunConfig` (D5/D7); the CLI's readable
  output prints `△ hook <event> …`. `agents/axh.rs` passes `loaded.hooks` (B4), pushes the skip note into its
  `Notes` collector and folds `Event::Hook` notes into the run's `stderr`.
- Tests: `hooks.rs` (a Claude-shaped section compiles / a broken one stops the run, the alias matcher,
  deny-vs-allow, exit 2, `SessionStart`'s three context channels, fail-open crash and timeout, the first block
  short-circuiting, `Stop` without a matcher, the payload's fields), `settings.rs` (the owner's hooks loaded, a
  project's parsed but left out — also in a trusted folder —, a broken project hooks file stops the load, the
  `hooks` key parsing beside unknown event names), `run.rs` (context behind the harness's own prompt, `PreToolUse`
  deny **and** B2's load-bearing *allow changes nothing against a deny rule*, `PostToolUse` replacing the model's
  result while the stream keeps the tool's own, the two-turn `Stop` through `stop_hook_active`, a session that
  cannot be built as an error note), and the core's end-to-end skill run whose `stderr` holds both the hook's
  block reason and the project-hooks note.

## HS6b — what exists

- `crates/axiomata-core/src/agents/axh.rs` — the backend: `run(request, config)` resolves the skill provider's
  endpoint (**B1**, errors are `InvalidAgentModel` naming the provider), takes an `agent_slots()` permit, builds
  the fresh gate (**B3**), reads `settings::load` for the sandbox only, loads the skill catalog from
  `~/.axh` + `~/.axiomata/skills` (notes → `tracing::warn`), and wires the toolbox in the CLI's order — files +
  `SkillRoots`, `skill` tool, gate, MCP (**B2**, `permitted(…, false)`), sandboxed shell — before
  `axiomata_harness::run` with the core-built prompt as the user message (**D1**). The outcome maps: `Answered`
  → exit 0 with the answer as `stdout`; `MaxTurns`/`Failed`/`Cancelled` → exit 1 with a `Notes` collector's lines
  (event errors, failed MCP servers, refused calls) as `stderr`; `Timeout` → `AgentTimeout { backend: "axh" }`.
  Tokens `0 → None`, turns → `num_turns`, `cost_usd: None` — the runner meters it (**D2/D8**).
- `agents/mod.rs`: `BACKEND_AXH`, `AgentBackend::Axh`, `run(&self, request, config)` (config reaches only the
  Axh arm) and `AgentRequest::name` (the run's name, → `restrict_name`). `skills/runner.rs`: `resolve_run_model`
  returns the **bare** model for Axh, `provider_label` the skill provider; `error.rs` gains
  `Harness(#[from] axiomata_harness::HarnessError)` shown as `harness: {0}`.
- Two small public additions in the harness itself: `skills::rules_of` (one rules string → `Vec<Rule>`, the same
  splitter the frontmatter parser uses) and `permissions::settings::home_dir()` — `$AXH_HOME` when set and
  absolute, else `$HOME`, empty counts as unset — so a test or embedder runs against its own `~/.axh`.
- Tests (`agents/axh.rs`, `runner.rs`): the end-to-end proof — a scratch `AXIOMATA_HOME` **and** `AXH_HOME`, a
  `backend: axh` skill and a fake `POST /v1/chat/completions` on the loopback → a `Success` row with
  `backend = "axh"`, `provider = "open_router"`, the bare `model`, 10/2 tokens, the metered cost and one single
  model call (no repair turn); B1's failed record ("… has no base URL …") without any model call; the gate's
  declare-is-the-whole-set semantics; the collector's keep/leave-alone; `resolve_run_model`/`provider_label`.

## HS6 — what exists

- `skills` (`Skill`, `Catalog`, `SkillTool`): the frontmatter parsed by `gray_matter` (a `RuleList` deserializer
  accepts Axiomata's space-separated `allowed_tools` string and Claude Code's list, the YAML checked for blow-up
  before the parser sees it); `Catalog::load(home, project, trusted, extra_dirs)` merges the folders of K2 with
  project > caller > owner, and every unusable skill is a note on stderr, never an error. `Skill::start_run` applies
  K5; `Skill::instructions` inlines the prepended files; `Skill::files` lists what sits beside the `SKILL.md`.
- The `skill` tool (K1.2), offered only while the catalog is not empty: its description lists every skill
  (`name: description`, 300 characters each, the whole list 8 000, then "…and N more, call skill with list=true"),
  `list=true` answers all names, a load emits `Event::Skill { name, how: loaded }` and returns the body plus the
  `skill:<name>/<rel>` file list (K4).
- `tools/files.rs`'s `SkillRoots`: `read_file`, `list_dir`, `grep` and `glob` take `skill:<name>/<rel>` paths as
  read-only roots beside the working folder, the `Read` rules judging exactly those paths; `write_file`/`edit_file`
  refuse them whatever the rules allow.
- `axh run --skill <name> [--skill-dir <path> …] [-- "extra prompt"]` and `axiomata-cli harness run` the same, the
  CLI adding `~/.axiomata/skills`; both emit `Event::Skill { name, how: run }` before the first model call and
  default the prompt to "Run the skill." (K1.1). An unknown `--skill` errors listing the known names.
- Live on this Mac (2026-10-13), against a base URL that refuses connections (no model call): `--skill mail-digest`
  through `--skill-dir ~/.axiomata/skills` — the skill event, the `skill` tool among the run's tools, its
  `prepend_files: ["Mail/.topics.md"]` skipped where there is no `Mail/` folder; the CLI lists all nine app skills
  for an unknown name.
- Tests (inline): the catalog (merge order, an untrusted project left out, a missing description, a `name` ≠ folder,
  an oversized file), the frontmatter (Axiomata's `mail-digest` style, Claude Code's list with a quoted
  `Bash(git status:*)`, an unknown key ignored), a skill run (body in the prompt, prepends in order and missing ones
  skipped, `..` refused, `allowed_tools` narrowing a `bash` call, `model` overriding), the `skill` tool (lists,
  loads, unknown name, the cut list, `list=true`, its event), the file roots (read, list, grep, glob, a write
  refused, an escape and a link out refused, a denied `Read` rule left out of a skill search) and the event's JSON.

## HS5 — what exists

- `mcp` (`config`, `approval`, `client`, `tools`): the two files merged with the owner winning (M1), approvals
  keyed by project + name + SHA-256 of the config in `~/.axh/mcp-approved.json` (M2), a `Client` whose transport is
  stdio (JSON-RPC lines, the removed client of `54173bf` as template) or Streamable HTTP (JSON or SSE answers,
  `Mcp-Session-Id` kept), the handshake `initialize` → `notifications/initialized` → `tools/list` (at most 10 pages,
  200 tools per server), and one `Tool` per MCP tool named `mcp__<server>__<tool>` (other characters → `_`, a name
  past 64 characters skipped with a note on stderr).
- `Access::Mcp`, the `mcp__server` / `mcp__server__tool` rule kind and the mode fallback (M5); the `bash`-style
  `Event::McpServer { name, status, tools, reason }`. Wired into `axh` and `axiomata-cli harness run` after the gate
  and before the shell; a server that fails to start is named in its event and left out, the run goes on. Servers
  are not sandboxed (M4); their tools are judged like any other call.
- Live (2026-10-13): against the real `@modelcontextprotocol/server-everything` over stdio — handshake, 13 tools
  listed, a call answered. The HTTP transport is tested against a fake server on the loopback (plain JSON, SSE with
  a notification before the response, the session id coming back, a 500 as an error).
- Tests (inline): config (both files merged, an unknown field, a bad name, a non-loopback `http://` URL, `${VAR}`
  in a header), approval (unapproved left out, approved runs, changed `args` ask again, mode 0600), the stdio
  client against a fake server script (handshake, pagination, result text, `isError`, a timeout, the process gone
  after the drop), the HTTP client, the two name cases, the MCP rules and the mode fallback.

## HS4 — what exists

- `commands` (`Registry`, `Command`, `Source`, `Expanded`, `PermissionsView`, `commands::builtin`): the files read
  through `axiomata_files::Root` (`LinkPolicy::Contained`; limits as named consts), the frontmatter parsed by hand
  (no `gray_matter`, no `serde_yaml`), the expansion with `$ARGUMENTS` / `$1`, and the five built-ins.
- `Gate::restrict` / `Gate::restrict_name` (C3) beside `Gate::rules` and `Gate::mode_name`, which the
  `/permissions` view is built from; `Event::Command`.
- `axh run … -- "/review src/"` and `axiomata-cli harness run "/review src/"`: the registry is built after the gate,
  a built-in is printed without a model (events in `axh`, text in the CLI), a file command runs as the prompt with
  its model and its narrowed gate; the readable output prints `/name (source)`, `--json` the `command` event.
- Two points the build plan left open were settled by the owner while building (2026-10-13): the `command` event
  carries the tag `event` like every other event (the `{"type": …}` in the plan was a slip of the pen, the same one
  HS6 and HS7's plans repeat), and "depth 3" counts the folders below `commands/` — `a/b/c/x.md` counts,
  `a/b/c/d/x.md` does not.
- Tests (inline): frontmatter (none, all four keys, quotes, an unknown key, a rule that does not parse, a block that
  never closes), names (folders as prefixes, a name that is no name, depth 4), precedence (built-in over project
  over user, an untrusted project not loaded at all), expansion (`$ARGUMENTS`, `$1`/`$2`, a missing `$3`, `$HOME`
  untouched, an unknown name, a `!` line, plain text, `@file` left in the text), limits (201 files, 65 KiB, a link
  out of the folder), the two restrict cases of C3, the three built-ins, and the `command` event's JSON.

## HS3b — what exists

- `sandbox` (`available`, `Policy::new(root, home, extra_writable, extra_secrets)`, `profile`) and `sandbox::proxy`
  (`Proxy::start(gate)`, `environment`); `Access::Network { host }` and the `Network(host)` rule; the `sandbox`
  section of the settings (`SandboxSettings`, returned by `build_gate`) and `ShellOptions::sandbox`.
- Tried on this Mac and kept honest: `(deny signal)` with `(allow signal (target same-sandbox))` holds, `(target
  others)` alone does not; `process-info` rules do not hide processes from `ps` (and the setuid `ps` does not run in a
  sandbox at all) — so other processes stay visible, but their environment is not readable by a non-root user
  anyway. Apple Events: Seatbelt's `appleevent-send`, `lsopen` and even a blanket `mach-lookup` deny did not stop
  `osascript … tell application "Finder"` on this Mac; refusing to start the programs does (also through a link; a
  copy of a system program is killed by macOS). Live (2026-10-13): `auto` let a Python write to `~` through and
  Seatbelt stopped it; with `--allow 'Network(example.com)'` curl reached example.com and got 403 for example.org.
- Reviews (code + security, 2026-10-13) found localhost open on every port, Apple Events, git folders beyond the
  top-level one, the `Host` header and pipelined requests, SSRF through a resolved private address, the proxy without
  limits, the sandbox silently off without `HOME`, and gaps in the secret list — all closed as above, with tests. A
  second security pass found the git denies passable by moving a prepared folder into place and by a new `.git` file,
  plus two `Content-Length`s and NAT64/6to4 addresses — closed too. Left by choice: a client that half-closes before
  its ask is answered counts as gone; any local process may use the proxy (judged by the same rules).
- Limits: a program that ignores the proxy variables reaches no network at all (not around the list); a host
  allowed is allowed with every path and port on it; the sandbox reads the rest of the disk (B3 is a deny list, not
  an allow list); a program that sends Apple Events through the system's interfaces itself (Python's `ctypes`, a
  compiled helper) is held only by macOS's own Automation consent; the package caches (B2).

## HS3a — what exists

- `permissions` (`Rule`, `RuleSet`, `Mode`, `Gate`, `Asker` with `Terminal` and `Nobody`, `build_gate`, `printable`),
  `permissions::command` (the lexer: `analyze`, opaque reasons, deny candidates, writes, protected names) and
  `permissions::settings` (the `.axh` files, `Trust`). Every tool says what a call touches (`Tool::access`); the
  toolbox's gate judges it before the call, a search leaves out what the read rules hold back, and the loop reports
  each judgement as a `permission` event.
- `axh run` and `axiomata-cli harness run`: `--permission-mode`, `--allow`, `--ask`, `--deny`; the CLI also `--role`
  (a role's `permissions` as allow rules), asks in the terminal and asks to trust a project with `.axh/` settings.
  `--allow-bash` is gone. The app's asker (`permission_asks`) follows with the Studio integration.
- Reviews (code and two security passes, 2026-10-13) found the first, naive splitter could be talked past (a `'` in a comment, a
  here-document, `$'…'`, `FOO=1 rm`, `/bin/rm`, `echo x > .axh/settings.json`), `grep`/`glob` read past `Read` denies,
  and protected names failed on case or through a link; the second pass found `eval`/`source`/`alias`, `X+=1 rm`,
  zsh precommands (`noglob`, `-`), non-literal command words (`=rm`, `$X`, `/bin/r?`), protected names behind globs,
  `RM` on macOS, `git -C . push`, writes after an earlier `cd`, links and hard links for path rules — all rebuilt as
  above, each with tests. Still open by the owner's choice: trust is per folder, not per content (P5).

## HS2 — what exists

- `tools::shell` (`shell_tools(cwd, ShellOptions)`): `bash`, `bash_output`, `kill_shell` over one `Session` per run
  (snapshot, folder, environment, jobs); the jobs are killed when the run's toolbox is dropped.
- Offered on every run since HS3a; each call is judged by the run's permissions (S1 superseded).
- Reviews (2026-10-13), built in: a process group is forgotten once its leader is reaped (no stale id ever hits
  another group), and a dropped call (timeout or stop of the run) kills its group; the snapshot runs as a login shell
  that reads `~/.zshrc` itself (an interactive `-i` shell in its own group would fight the terminal and stop); a
  `$SHELL` other than zsh or bash runs as `/bin/sh`; `builtin cd/eval/pwd` so a profile function cannot take over the
  wrapper; a deleted folder sends the next command to the run's folder; job output never breaks a UTF-8 character;
  at most 32 jobs are kept; Ctrl-C, SIGTERM and SIGHUP all end the run cleanly, its jobs included.
- **Known limits, until HS3's sandbox:** a child that leaves its process group on purpose (`setsid`, a daemon,
  `launchctl`) survives the kill; a command can read what the owner's user can read, including the harness's own
  start environment through `ps` (so the key) and the credential files — the clean environment (S5) keeps stray
  variables out of commands, it does not hide them.
- Tests: output and exit code, the folder carried over and set back, the cleaned environment (no key), a timeout
  killing a backgrounded grandchild, jobs read, stopped and ended with the run, and the zsh profile snapshot
  (alias, function, `PATH`) against a scratch `HOME`.

## HS1 — what exists

- `crates/axiomata-harness`: `config` (`ModelEndpoint`, `RunConfig`), `model` (`ChatModel`, `OpenAiCompat`),
  `tools` (`Tool`, `Toolbox`, the results cut to 30 000 characters) and `tools::files` (`read_file`, `write_file`,
  `edit_file`, `list_dir`, `grep`, `glob`, all through `axiomata_files::Root` with `LinkPolicy::Contained`), `prompt`,
  `event` (`Event`, `EventSink`), `run` (the loop, `Cancel`).
- `axh run --base-url … --model … [--cwd …] [--instructions-file …] [--max-turns N] [--timeout-secs S] "prompt"`.
- `axiomata-cli harness run [--provider open-router|ollama] [--model …] [--cwd …] [--json] "prompt"` — owner only.
- Limits and guards (security audit 2026-10-13): no write into `.git`, `.hg`, `.jj`, `.axiomata`, `.claude`,
  `.opencode`, `.vscode`, `.mcp.json`, `opencode.json` (a line in `.git/config` runs at the owner's next `git status`;
  the others are configuration a later session obeys) and no read or listing of the version-control stores; at most
  16 tool calls per turn; answers read up to 16 MiB; no redirects; a key only over https or to this machine, and
  redacted from echoed errors; a stopped run stops a running search and starts no tool (a write already on a
  blocking thread still finishes). Build files (`Makefile`, `package.json`, `build.rs`) stay writable — the owner
  reviews the diff, as with any coding agent.
- Tests: the client against a fake API on the loopback, the loop with a scripted model, the file tools in a temp
  folder (escapes refused), and a live test (`tests/live.rs`, ignored) against a real API.
