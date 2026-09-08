# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this
repository. It is intentionally short and pointer-heavy — it is resent in full on **every**
turn in this repo, so it only carries build/run commands, a compact current-state summary,
and the handful of non-obvious traps that would otherwise cost a whole debug session to
rediscover. Everything else (design rationale, full module-by-module walkthrough, milestone
history) lives in **[`docs/architecture.md`](docs/architecture.md)**, which is *not*
auto-loaded — read it before starting substantial new work, and **update it** (not just the
paragraph below) when a milestone or major feature lands. It went stale between M3 and M6
for exactly the opposite reason once already; see its own maintenance note.

## Project status

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines, Memory,
Skills — see `ARMS-Agentic-OS-Guide.pdf`, though the actual design has since diverged from it
in several places; `docs/architecture.md` §1 explains how).

Milestones **M0–M3, M5, M6** are complete (scaffold, skills runner, memory router, routines
scheduler w/ full CRUD, the Svelte module-canvas dashboard, the particle-graph Second Brain),
plus ongoing post-M6 feature work (ToDo, Calendar/Reminders/Mail connector modules, themes,
the `srcdoc` HTML viewer). Only **M4 (always-on/background scheduling)** is still
unimplemented. Full detail: `docs/architecture.md` §5 (what exists) and §7 (milestone
history). Detailed step-by-step milestone plans live outside this repo, in the owner's local
Claude Code planning notes — read them before starting new M0–M6-scale work if available.

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (must be warning-free)
cargo fmt                                  # format
cargo fmt --check                          # verify formatting in CI-style checks
cargo test --workspace                     # run all tests
cargo test -p axiomata-core                # run just the core engine's tests

cargo run -p axiomata-cli                  # headless: init the core, print status, exit
cargo run -p axiomata-cli -- list-skills   # discovered skills (~/.axiomata/skills/)
cargo run -p axiomata-cli -- run-skill <name>   # run a skill, print outcome, exit 1 if it failed
cargo run -p axiomata-cli -- list-runs --limit 20   # recent run history from the DB
cargo run -p axiomata-cli -- memory sync    # regenerate the workspace CLAUDE.md router blocks
cargo run -p axiomata-cli -- memory status  # is the router stale?
cargo run -p axiomata-cli -- routines list  # scheduled routines, soonest next-fire first
cargo run -p axiomata-cli -- routines add --name daily --cron '0 0 9 * * *' --skill <name>
cargo run -p axiomata-cli -- routines edit <id> --name … --cron … --skill|--prompt … [--backend …] [--disabled]
cargo run -p axiomata-cli -- routines delete <id>
cargo run -p axiomata-cli -- routines tick  # run one scheduler poll pass now (no 30s wait)
cargo run -p axiomata-cli -- assistant "hi" [--resume <session_id>] [--instruct] [--allowed-tools <tools>]
cargo run -p axiomata-cli -- modules        # print the module manifest the dashboard wrote
cargo run -p axiomata-cli -- module-action <instance> <action> --json '{}'  # needs a running dashboard
cargo run -p axiomata-cli -- graph          # workspace graph summary (areas, links, skills, routines)
cargo run -p axiomata-cli -- import obsidian <folder> [--dry-run] [--skip-secrets]  # agent-sorted import

cd apps/dashboard && cargo tauri dev       # run the desktop app (hot-reloading dev mode)
cd apps/dashboard && npm run check         # svelte-check + tsc (must be clean)
cd apps/dashboard && npx vite --port 1420  # frontend alone in a browser: Tauri commands are
                                           # served by src/core/devmock.ts fixtures (DEV only)
cd apps/dashboard && npx vitest run        # frontend unit tests (pure TS logic, e.g. core/*.ts)
```

For browser-level checks (`agent-browser` against `vite --port 1420`) the mock backend
returns fixture data; anything that needs the real Rust side (persistence, file commands,
the agent) is verified by launching `cargo tauri dev` under a scratch `AXIOMATA_HOME`.

One-time setup for the Tauri app: `cargo install tauri-cli --version "^2" --locked`, and
`cd apps/dashboard && npm install`.

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
- **An MCP tool call is silently denied in a headless run unless `allowed_tools` is set** —
  it is *not* covered by `--permission-mode` at all. The run still exits 0 and "succeeds";
  the tool call is just refused, with no error anywhere. This bit the Calendar module once
  already. Set `SKILL.md` frontmatter `allowed_tools:`, or the caller's own
  `--allowed-tools` / `ChatRequest.allowed_tools`, for any skill or chat/instruct turn that
  reaches an MCP tool (`docs/architecture.md` §5 "Agent backends").
- **Routines**: cron is the `cron` crate's **6–7 field, seconds-first** format
  (`0 */2 * * * *`), not 5-field crontab. `add`/`update`/`delete` all exist (`update` is a
  full replace, not a partial patch); the dashboard's Routines module UI uses a friendly
  interval picker (`core/routineInterval.ts`) over that cron, not a bare text field.
- **A connector module (Calendar/Reminders/Mail-shaped) is "provider = skill, not code"**:
  an MCP-backed `*-digest` skill, no live poll (every refresh is a real agent turn), writes
  go through a silent one-shot instruct turn, not the skill. Follow this pattern for the next
  integration rather than hand-rolling Tauri commands for it (`docs/architecture.md` §5).
- **HTML/course pages render via `<iframe sandbox srcdoc=…>`, not `asset://`** — an
  `asset://` + `<iframe src=…>` design was tried first and never actually worked (silent
  WebKit sandboxing wall); don't re-attempt it without reading the postmortem in
  `docs/architecture.md` §5 first.
- **Themes**: every colour/size in a Svelte component goes through a `--ax-*` token
  (`themes/tokens.css`) — no literals. A user's `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection.
- **Model**: every `claude -p` run passes `--model` from the **active provider**'s settings —
  `config.agents.providers[active_provider].chat_model` for chat turns,
  `.skill_model` for skill/routine runs (a skill's own `model:` frontmatter still wins). v1
  providers: `Anthropic` (default; no base URL/key, subscription-billed via the CLI login) |
  `OpenRouter` | `Ollama` — all four `ProviderSettings` fields kept per provider even while
  inactive. Anthropic's defaults are `claude-sonnet-5` (chat) / `claude-haiku-4-5` (skills) —
  skills stay on Haiku because the app kept hitting its session usage limit within a day or
  two of small feature work; bump `skill_model` once that's not a concern. The old flat
  `agents.claude_model` is migration-only now. Provider env plumbing and the
  `RwLock<Config>` / `get_config`/`save_config` runtime-mutation path:
  `docs/architecture.md` §5 "Model providers"; full rationale:
  `docs/plans/settings-provider-overhaul.md`.

## Sub-agents (use the Rust variants, not the Python-oriented defaults)

The owner's global `~/.claude/CLAUDE.md` defines mandatory automatic sub-agent triggers.
Three of the named agents there (`test-engineer`, `dependency-auditor`,
`performance-analyzer`) are worded for a Python/`uv` stack and **do not apply to this repo**.
Global, Rust-flavored replacements exist at `~/.claude/agents/{rust-test-engineer,
rust-dependency-auditor,rust-performance-analyzer}.md` (usable in any Rust project) — use
those instead, same trigger conditions translated to Rust terms (`cargo test`, `cargo audit`,
`rusqlite`/`tokio`), with a **project-local cadence override** (owner, 2026-09-08; rationale
in `docs/architecture.md` §7): fire them **once per plan checkpoint, and always before a
commit**, not after every single changed `fn`/`struct`/`Cargo.toml` line mid-task. Keep
writing/updating tests inline as code lands regardless — the test-engineer's run is a bundled
second-pass gap check over the accumulated diff, not the first pass. The same override applies
to Claude's own verification loop: batch `cargo build`/`clippy`/`fmt`/`test` per unit of work,
and run the full set only when the work is done, before handing off to a sub-agent, and before
a commit.

`architecture-reviewer`, `security-auditor`, `docs-writer`, and `refactoring-specialist` are
already language-agnostic as globally defined and apply here unchanged. When
`architecture-reviewer` or another background sub-agent run is not available (e.g. a session
rate limit), do a manual review pass yourself rather than skipping the check — the trigger
is mandatory, not the specific tool.
