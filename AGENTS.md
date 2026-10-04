# AGENTS.md

Guidance for coding agents working in this repository — Opencode and Claude Code alike
(Claude Code reads it through `CLAUDE.md`, which imports this file and adds what is specific
to it). It is intentionally short and pointer-heavy — it is loaded in full on **every** turn,
so it only carries build/run commands, a compact current-state summary, and the handful of
non-obvious traps that would otherwise cost a whole debug session to rediscover. Everything
else (design rationale, full module-by-module walkthrough, milestone history) lives in
**[`docs/architecture.md`](docs/architecture.md)**, which is *not* auto-loaded — read it
before starting substantial new work, and **update it** (not just the paragraph below) when a
milestone or major feature lands. It went stale between M3 and M6 for exactly the opposite
reason once already; see its own maintenance note.

## Project status

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command centre / second brain,
built around the **ARMS framework** (Applications, Routines, Memory, Skills — see `ARMS-Agentic-OS-Guide.pdf`; the
design has since diverged, `docs/architecture.md` §1 explains how).

- **Done:** M0–M3, M5, M6, post-M6 modules (ToDo, Calendar/Reminders/Mail, themes, HTML viewer, Terminal), M7.0–M7.3
  (Kanban, IDE shell, agent panes, git layer), the editor ED0–ED6, Opencode 2, the **Studio** workbench
  (`docs/plans/workbench.md`), Run/Tasks (#50) and Debug (#51). **M4 (always-on scheduling) was dropped** (owner,
  2026-09-20) — do not plan around it. Node/TypeScript debugging is parked (owner, 2026-10-02).
- **Next / open:** the owner's Mac test of the Studio, Run/Tasks and Debug (the Tauri glue in `src-tauri` is not
  compilable on the Linux dev box); #50 follow-ups; **agent-to-agent communication (M7.5)** — approved 2026-10-03,
  CP-A1 (engines + roles), CP-A2 (the board's agent flow) and CP-A3 (the mailbox core) and CP-A4 (the MCP server) built 2026-10-04, next CP-A5 (per-harness entry, `docs/plans/a2a.md`); ED7; a Mac-only-code split. Deferred by owner decision:
  ⌘K spotlight search and further model-provider work.
- **Where the detail lives:** `docs/architecture.md` §5 (what exists), §7 (milestones) and its "Status log"
  (the full status paragraph, 2026-10-04 snapshot at the end); plans in `docs/plans/`. Read them before substantial
  new work and **update them** (not just this file) when a milestone or major feature lands.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (must be warning-free)
cargo fmt                                  # format
cargo fmt --check                          # verify formatting in CI-style checks
cargo test --workspace                     # run all tests
cargo test -p axiomata-core                # run just the core engine's tests

cargo run -p axiomata-cli                  # headless: init the core, print status, exit
cargo run -p axiomata-cli -- --help        # every subcommand; each has its own --help. The groups:
#   list-skills | run-skill <name> | list-runs | get-run <id> | skills reseed [--force]
#   memory sync|status                       (the workspace router blocks)
#   routines list|add|edit|delete|tick       (cron is 6–7 fields, seconds first)
#   board list|new|rename|delete|add|edit|move|claim|done|verify|archive   (Kanban; claim is a CAS)
#   board plan|dep|report|verdict|events|note|input|fail|cancel|reopen|taken-over|approve   (agent flow, a2a.md CP-A2)
#   mcp-serve                                (MCP server of one agent session on stdio; needs AXIOMATA_AGENT_ID; logs go to stderr)
#   ide projects list|new|rename|set-root|delete
#   ide agents list|new|edit|delete|prepare|new-session|discard-worktree|status|diff|base|commit|discard|discard-hunk|take-over
#                                            (take-over: squash by default, never pushes)
#   ide engines list|add|edit|delete         (the owner's engine catalog; delete refused while a session/role uses it)
#   ide roles list|show|save|delete|project|confirm   (AGENT.md roles; `project`/`confirm`: a project's own roles, by hash)
#   files roots|read|write|grants|search     (through the editor's guard; roots: workspace, project:<id>, worktree:<agent>, grant:<id>)
#   assistant "hi" [--resume <id>] [--instruct] | modules | module-action <instance> <action> --json '{}' | graph | import obsidian <folder>

cd apps/axiomata && cargo tauri dev       # run the desktop app (hot-reloading dev mode)
cd apps/axiomata && npm run check         # svelte-check + tsc (must be clean)
cd apps/axiomata && npx vite --port 1420  # frontend alone in a browser: Tauri commands are
                                           # served by src/core/devmock.ts fixtures (DEV only)
cd apps/axiomata && npx vitest run        # frontend unit tests (pure TS logic, e.g. core/*.ts)
```

For browser-level checks (`agent-browser` against `vite --port 1420`) the mock backend
returns fixture data; anything that needs the real Rust side (persistence, file commands,
the agent) is verified by launching `cargo tauri dev` under a scratch `AXIOMATA_HOME`.

One-time setup for the Tauri app: `cargo install tauri-cli --version "^2" --locked`, and
`cd apps/axiomata && npm install`.

Dev-only: a local `apps/axiomata/.env.local` with `VITE_AXIOMATA_DISABLE_AUTO_REFRESH=true`
skips the calendar/mail/reminders modules' mount-time auto-refresh (each is a billed
`run_skill` agent turn) on repeated `cargo tauri dev` restarts — cached digests still load,
and each module's manual ↻ still runs regardless (`core/devFlags.ts`).

## Architecture — traps worth knowing before you touch things

Full walkthrough: `docs/architecture.md` §3–§5. The load-bearing facts that aren't obvious
from the code itself:

- **Runtime data lives outside the repo**, at `~/.axiomata/` (config, DB, logs, skills,
  dashboard layout — all app-owned), separate from the user's freely-chosen Second-Brain
  workspace folder (`config.workspace_root`, memory-router content only). Override with
  `AXIOMATA_HOME` in tests. New shared Cargo deps go in the root `Cargo.toml` under
  `[workspace.dependencies]`, referenced per-crate as `some_crate.workspace = true`.
- **Skills live in one place only**, `~/.axiomata/skills/<name>/SKILL.md` — there is no
  workspace-local skill location (dropped deliberately; `docs/architecture.md` §4 explains
  why).
- **Opencode 2 is a service, and Axiomata is its API client** (`docs/plans/opencode2.md`,
  crate `axiomata-opencode`; docs: https://opencode.ai/v2/docs, *not* `/docs`, that is v1).
  Skills and chat are sessions on the one shared background service (`POST /api/session`,
  prompt, wait on `/api/event`, read the messages back) — never parse a CLI's output again,
  never `--standalone` (it deadlocks on the shared database and starts its own MCP servers,
  apple-mail opening Mail.app each time). Unit tests cannot reach the real service
  (`cfg(test)` guard in `agents/opencode.rs`); live checks are `#[ignore]`d in
  `crates/axiomata-opencode/tests/live.rs`. After every Opencode update, check one real skill run.
- **Connector skills reach MCP through opencode's own config, not Axiomata's** — skill
  sessions run on the service (`AgentBackend::Opencode`), which resolves the
  MCP servers from `~/.config/opencode/opencode.json` (the same `apple-mail` /
  `apple-reminders` servers); tool use is pre-approved by the session's permission rules
  (`auto_approve_tools`, the former `--auto`). There is no
  `allowed_tools` allow-listing at runtime anymore (that was a Claude Code `--allowedTools`
  trap, §5 of `docs/architecture.md`); the frontmatter field is kept as documentation only.
- **Routines**: cron is the `cron` crate's **6–7 field, seconds-first** format
  (`0 */2 * * * *`), not 5-field crontab. `add`/`update`/`delete` all exist (`update` is a
  full replace, not a partial patch); the dashboard's Routines module UI uses a friendly
  interval picker (`core/routineInterval.ts`) over that cron, not a bare text field.
- **A connector module (Calendar/Reminders/Mail-shaped) is "provider = skill, not code"**:
  an MCP-backed `*-digest` skill, no live poll (every refresh is a real agent turn), writes
  go through a silent one-shot instruct turn, not the skill. Follow this pattern for the next
  integration rather than hand-rolling Tauri commands for it (`docs/architecture.md` §5).
  The digests run on the single agent harness — an Opencode session on whatever
  `skill_provider` routes to (`openrouter/deepseek/...`, `anthropic/claude-haiku-4-5`, or a
  local `ollama/<model>`), the same way opencode itself would run them; that harness is what
  replaced both the Claude Code CLI and the Stufe 2 `ollama-agent` tool loop.
  `mail-digest` also uses `prepend_files: ["Mail/.topics.md"]` to inline its workspace topics
  (opencode has no file tool for `mail-digest`'s workspace file).
- **The vault router is `AGENTS.md`** (was `CLAUDE.md`, 2026-09-28): `memory sync` writes the router block into
  `<workspace>/AGENTS.md` and `<area>/AGENTS.md`, renames an old `CLAUDE.md` router on the way (the owner's own text
  goes along) and leaves a `CLAUDE.md` that only imports `@AGENTS.md`, so Claude Code reads the same map. The chat's
  appended `module-context.md` now opens with a fixed guide to the app (`bridge.rs` `APP_GUIDE`: workspace, skills and
  their frontmatter, routines, the board, "ask when unclear, confirm before moving or deleting").
- **Bundled skills are seed-if-absent**: a `resources/<name>/SKILL.md` edit does **not**
  reach an install whose `~/.axiomata/skills/<name>/SKILL.md` already exists (the seed never
  overwrites). Bring it up to date with `cargo run -p axiomata-cli -- skills reseed --force`
  (re-copies only the bundled ones — the four digests/cleanup plus `inbox-sort` and `todo-to-kanban`; user skills untouched).
- **HTML/course pages render via `<iframe sandbox srcdoc=…>`, not `asset://`** — an
  `asset://` + `<iframe src=…>` design was tried first and never actually worked (silent
  WebKit sandboxing wall); don't re-attempt it without reading the postmortem in
  `docs/architecture.md` §5 first.
- **The Kanban board mirrors itself into the vault, one way only**: every board
  change rewrites `<workspace>/Kanban/<id>-<name>.md` (`core/board_mirror.rs`), so
  Obsidian, the Second-Brain search and the memory router can see a board without
  the app. **Nothing reads that file back** — the database is the board. A rename
  produces a new file name and the old one is swept up by id prefix; deleting a
  board removes its mirror. Writing it is best-effort and never fails the edit
  that triggered it.
- **File access for the editor goes through `axiomata-files`, never a raw path**: the webview
  names a file as `{ root, rel }` (`workspace` strict; `project:`/`worktree:`/`grant:` contained),
  every action walks from the root fd with `openat(O_NOFOLLOW)` (`pinned.rs`), and a new place
  on disk is reachable only through `file_pick` — the native dialog driven from Rust. Never grant
  `dialog:*` (or `fs:*`) in `capabilities/default.json`; that would let the webview forge picks.
- **Nothing a stored file says may start a process**: a pane's command is a *prop* of the component that mounts the
  terminal, never a field of its stored config (`modules/terminal.svelte`'s `initialCommand`). A task pane's tab names
  only the task id; its command line is resolved in Rust (`task_command_line`) and kept in memory (`ide/taskRuns.ts`),
  and a project's own `.axiomata/tasks.json` runs only after the owner confirmed its exact bytes (SHA-256, `axiomata-tasks`).
  Same for roles (`AGENT.md`, `axiomata-roster`): a role names engines by id and never carries a command line, and a project's own
  roles apply only after the same kind of confirmation. Engines are saved into the *file* config only (`roster::persist_engines`),
  never by saving the live config — that would undo a workspace change queued for the next start.
- **In an agent session the CLI acts as that agent** (`axiomata_core::session`, from `AXIOMATA_AGENT_ID`/`_NAME`): `--actor`
  cannot say otherwise, the owner's commands (`board plan approve`, `board approve`, `board taken-over`, deleting/archiving …)
  are closed there, and an agent adds cards only to the board's proposal column. The board store enforces the same gates for
  any caller (`human:` actor for the owner's steps; an `agent:` actor never moves a card freely). Not a sandbox — see a2a.md A39.
- **Kanban has no tile** — it is the ring entry `view:kanban` that opens a large panel. The agent bridge's manifest lists only
  *mounted instances*, so anything an agent must reach without an open window (the Kanban actions) is a **shell action**
  (`registerShellAction`, prefixed `kanban_`), never a module action.
- **Every board has a Vorschlag and a Review column** (roles `proposal`/`review`, not deletable): "the open column" for a
  new card is the first *plain* open one — `board_move`/`move_to_status` and the assistant's `add_card` skip the roles. A new
  card put in Vorschlag by mistake waits for an approval nobody expects.
- **The editor engine (`src/editor/`) imports nothing from the app** — no DOM, no Svelte, no
  `core/`; `src/fileapp/` depends on it, never the reverse (D1, extractable for ED7). Its
  `EditorDocument` is a mutable class: a Svelte component redraws via its own counter after
  every `doc.*` call, not via Svelte reactivity (`EditorSurface.svelte`'s header).
- **tree-sitter grammars are built, not downloaded at runtime**: `apps/axiomata/scripts/build-grammars.sh
  [name…]` (pinned tags, pinned `tree-sitter-cli`) writes `public/grammars/`, which is checked in. Never read
  a tree-sitter `node.text` — `web-tree-sitter` re-calls the parse callback with a stale position; slice the
  `TextStore` instead (`syntax/highlighter.ts`). The CSP's `'wasm-unsafe-eval'` exists for tree-sitter.
- **Themes**: every colour/size in a Svelte component goes through a `--ax-*` token
  (`themes/tokens.css`) — no literals. A user's `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection.
- **UI scale** (`docs/plans/editor-look.md`, K9–K11): every UI size is `N px × var(--ax-ui-scale)`
  (tokens already are; a size that is no token is written `calc(Npx * var(--ax-ui-scale))`), set by
  `core/uiScale.ts` from the display's real density (`axiomata-macos::display`) or the "UI size"
  setting. The canvas, the floating panels and the file tree store **unscaled** sizes and draw them
  times `$uiScale` — divide pointer deltas by it. Not scaled: the editor's and terminal's fonts, and
  anything measured in editor character cells. Script that reads a size token gets the unresolved
  `calc(…)`, not a number. UI icons: `ui/Icon.svelte`/`ui/IconButton.svelte` over the Lucide subset
  vendored by `scripts/vendor-icons.sh`.
- **Backend ids + the one frontmatter bridge**: `backend:` is `opencode` (default) |
  `ollama`; the only other relevant field is `prepend_files: ["rel/path.md"]` (workspace files
  inlined into the prompt as `## Context file:` blocks; missing = skipped, `..`/absolute =
  run fails as a recorded `Failed`, per §D of the Stufe 2 plan). The `local_backend` /
  `effective_backend` mechanism and the `ollama-agent` backend are gone.
- **Model / provider**: the provider is chosen **per role** — `agents.chat_provider` for
  interactive chat, `agents.skill_provider` for skill/routine runs — resolved via
  `AgentDefaults::provider_for(ProviderRole::{Chat,Skill})`. Every Opencode session gets
  the model `provider/<model>` from *its role's* provider (`providers[chat_provider].chat_model`
  / `providers[skill_provider].skill_model`; a skill's own `model:` frontmatter still wins),
  and opencode resolves the provider's auth/keys itself from its own credential store — there
  is no `ANTHROPIC_*` env plumbing anymore. `guard_redirected_turn(db, config, role)` still
  gates the paid roles, metering them from token counts × an owner-set per-model price table
  (`config.agents.costs` / `spend::metered_cost_usd`) rather than the CLI's own estimate —
  `runs.model` (migration 0007) records the model so `spend::reconcile_recorded_costs` can
  re-meter already-recorded runs at startup. v1 providers: `Anthropic`
  (default; no base URL/key, subscription-billed via the CLI login) | `OpenRouter` | `Ollama`
  — all `ProviderSettings` fields kept per provider even while unused. Anthropic's defaults
  are `claude-sonnet-5` (chat) / `claude-haiku-4-5` (skills). The old flat `agents.claude_model`
  and single `agents.active_provider` are both migration-only now (`active_provider` folds
  into both role fields on load, then `save()` drops it). Details: `docs/architecture.md` §5
  "Model providers"; `docs/plans/per-role-provider.md`;
  `docs/plans/settings-provider-overhaul.md`.
