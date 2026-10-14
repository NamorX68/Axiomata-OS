# HS7 — Hooks: SessionStart / PreToolUse / PostToolUse / Stop

Self-contained instructions for step HS7 of the harness (`docs/plans/harness.md`, card #37). Written 2026-10-14
for the owner. Owner decisions are marked **[owner]**, points the planner fixed are **[default]** — build them as
written, do not redesign.

## Where and how

As in HS6/HS6b: worktree `.claude/worktrees/harness`, branch `worktree-harness`, gate
`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, one commit
`feat(harness): hooks, step HS7 (#37)`, no push. H2 unchanged (no harness → core dependency); the small core/CLI
wiring below is core → harness, as in HS6b.

Hooks are Claude Code compatible: the config sits in `settings.json` under `"hooks"` in Claude's shape, the payload
arrives as JSON on stdin, and the answers are Claude's JSON (or the exit codes). Claude's four events:
`SessionStart`, `PreToolUse`, `PostToolUse`, `Stop`.

## Decisions (owner, 2026-10-14)

- **B1 [owner, rev. 2026-10-14] — owner hooks only.** Hooks come from `~/.axh/settings.json` alone. A project's
  `.axh/settings.json` / `.axh/settings.local.json` *hooks section is parsed* (a settings file holding hooks must
  not fail the load) **but never executed**; the load reports that it was skipped and the caller puts a note in
  the event stream (CLI: stderr line, core: through the run's `Notes` into `stderr`) — the same shape as a
  project's MCP server being left out headless. The repo's rule *"nothing a stored file says may start a
  process"* therefore holds for hooks, too; the first answer (project hooks run, Claude parity) was withdrawn by
  the owner the same day, before any code existed.
- **B2 [owner, rev. 2026-10-14] — hooks veto, the gate decides.** A `PreToolUse` hook may **block** (`deny`, or
  exit 2) — then nothing runs and the reason goes back to the model. `allow` and `ask` are **no-ops**: what the
  rules, the mode and the skill's `allowed_tools` narrowing say stands (HS6b-B3's determinism is untouched — a
  headless run behaves the same with or without hooks). Hooks are a veto layer *before* the gate, not a second
  door beside it; Claude's JSON shape is parsed faithfully, its `allow` simply carries no weight here (the first
  answer, full Claude fidelity, was withdrawn by the owner the same day, before any code existed).
- **B3 [owner] — fail open.** A hook that crashes, exits with anything but 0/2, or runs into its timeout produces
  an `Event::Hook` note and the run continues. Only an explicit block blocks. (A broken hook must not stop
  `mail-digest` at 6 a.m.)
- **B4 [owner] — unattended core runs execute hooks** (`backend: axh`): same as the CLI, the owner's hooks from
  `~/.axh/settings.json` (B1 applies everywhere: no project hooks anywhere). HS6b-B3's "the core reads only the
  sandbox from the settings" is extended by exactly this — and with B2+B3 a hook may only block (never permit)
  and may not stop a run when it is itself broken, so a run can only come out stricter or unchanged, never
  looser.

## Defaults (planner)

- **D1 — config shape (Claude's).** `settings.json` gets a `hooks` key:
  `{"hooks": {"PreToolUse": [{"matcher": "Bash|Write", "hooks": [{"type": "command", "command": "…", "timeout": 30}]}]}}`.
  Only `type: "command"` exists; an unknown `type` or an unparsable matcher is a `HarnessError::Settings` at load
  (P4: a broken config stops the run, it is not skipped). Unknown *event names* are ignored (a Claude update must
  not brick a settings file); fields inside an entry that we do not know are ignored (forward compatible).
  No `timeout` given: 60 s (Claude's default).
- **D2 — matcher.** A regex, unanchored search like Claude's, matched against the tool's own name **and** its
  Claude alias (`bash → Bash`, `read_file → Read`, `write_file → Write`, `edit_file → Edit`, `list_dir → Ls`,
  `grep → Grep`, `glob → Glob`; `mcp__server__tool` is already Claude's spelling). Missing/empty matcher: every
  call. For `SessionStart` the matcher is matched against `source` (we always send `"startup"`); `Stop` has no
  matcher.
- **D3 — where in the loop.**
  - `SessionStart` once, after `Event::Started`, before the first model call: `additionalContext` (JSON) or plain
    stdout, exit 2's stderr (Claude's three context channels) becomes a `system` message right behind the harness's
    own — never a block.
  - `PreToolUse` before the gate, on every call the run actually attempts (not on calls cut by
    `MAX_CALLS_PER_TURN`): block → the model gets `error: <reason>` as the tool result, nothing runs, no gate log;
    `allow`/`ask` → nothing happens (B2) and the ordinary `call_until` with the gate judges as today. There is
    **no bypass path** — veto-only means the gate's own code needs no new door.
  - `PostToolUse` after the tool ran (any outcome), the `tool_response` = the (capped) tool output; a block
    replaces what the model gets with `error: <reason>` (Claude: the model sees the reason instead).
  - `Stop` when the model answered (no tool calls): block → the reason goes into the conversation as a user
    message and the loop continues with another model turn (`stop_hook_active: true` tells the hook it already
    did this); no block → the run ends as `Answered`. `max_turns` still bounds the whole thing.
- **D4 — stdin payload** (Claude's fields, ours in it): `session_id` (`axh-<pid>-<epoch-ms>`, there are no sessions
  yet), `transcript_path: ""`, `cwd`, `hook_event_name`, `source` (SessionStart), `tool_name`/`tool_input`
  (Pre/Post), `tool_response` (Post), `stop_hook_active` (Stop).
- **D5 — how a hook runs.** The shell module's `Session` (made `pub(crate)`), so a hook gets what a command gets:
  the owner's `$SHELL`, the profile snapshot (S2 — aliases and the homebrew PATH matter in hooks), the cleaned
  environment (S5), its own process group, killed on timeout or run end, and the sandbox from
  `RunConfig.sandbox` (a new field carrying the same `SandboxSettings` the shell tools get) — including the run's
  proxy for the network. Stdin = the payload, stdout/stderr captured. Hooks sequential in configured order; the
  first block short-circuits the rest of that event (its event is emitted, the skipped ones are not). Runs
  honour the run's `Cancel` and the run deadline (`min(hook timeout, time left)`).
- **D6 — events.** New `Event::Hook { turn: Option<u32>, hook_event, command (cut), blocked, context_chars,
  note }` — `note` carries a fail-open failure (B3) or the block reason, cut like every event text. The core's
  `Notes` collector folds a `note` into the run's `stderr`, so a dashboard run shows what a hook did or why it
  did not. The session's JSONL line stays the only sink plumbing — no new tables.
- **D7 — wiring.** `settings::read` parses and validates hooks (D1) from every file, but `settings::load` puts
  **only the owner's** into `Loaded.hooks` (B1) and reports a project's skipped hooks as
  `Loaded.project_hooks_skipped: bool` (the `untrusted_project` pattern); `permissions::build_gate`'s tuple grows
  `hooks` (and the skip flag) as further elements (axh binary and `axiomata-cli harness run` destructure them and
  print the skip note); `RunConfig` gains `hooks` and `sandbox`; `core/agents/axh.rs` passes `loaded.hooks` (B4)
  and puts the skip note through its `Notes` collector — its gate for B3 is built exactly as in HS6b, untouched.
  A caller that loads no settings runs no hooks (empty `Hooks` = zero cost, no Session built).
- **D8 — one Session per run with hooks**, built lazily in `run()` when `hooks` is non-empty; a Session that
  cannot be built is an `Event::Error` note and the run goes on without hooks (B3 applied to the runner itself).
  Known cost: one extra profile snapshot per run that has hooks.

## What to build

1. **`crates/axiomata-harness/src/hooks.rs` (new)** — the config types (Claude's shape, D1), the compiled
   `Hooks` (regexes validated at parse), `HookOutcome { context, blocked }`, and `run_event(event, target,
   payload, hooks, session, limit, stop, sink)` doing the spawning (D5) and the answer parsing: JSON
   `hookSpecificOutput.permissionDecision` / legacy `decision`, exit 2 = block with stderr, `allow`/`ask` parsed
   and dropped (B2), everything else fail-open (B3).
2. **`settings.rs`** — `File.hooks`, `Settings.hooks`, strict validation per D1; `Loaded.hooks` (owner only)
   plus `Loaded.project_hooks_skipped` (B1).
3. **`event.rs`** — `Event::Hook` (D6); **`config.rs`** — `RunConfig.hooks` + `RunConfig.sandbox`.
4. **`tools/shell.rs`** — `Session` and a stdin-taking run method `pub(crate)` (D5).
5. **`run.rs`** — the four integration points (D3), the SessionStart message insert, `stop_hook_active`,
   deadline/cancel handling (D5).
6. **Callers** — `bin/axh.rs`, `crates/axiomata-cli/src/harness_cmd.rs` (tuple + `RunConfig`, D7),
   `crates/axiomata-core/src/agents/axh.rs` (`Loaded.hooks` → `RunConfig`, `Notes` collects `Event::Hook`
   notes, B4).
7. **Docs in the same commit** — this file; `docs/plans/harness.md` (roadmap HS7 ✅, a decisions section, a
   "HS7 — what exists"); `docs/architecture.md` (§5 harness paragraph + status); worktree `AGENTS.md`
   (`HS1–HS7 built, HS8 open`, one line about hooks where the harness trap list needs it).

## Tests (inline, all must pass)

- settings: a Claude-shaped `hooks` block parses (all four events, matcher, timeout); an unknown event name is
  ignored; a broken matcher / unknown `type` is a settings error naming the file; **a project's hooks section
  parses but does not reach `Loaded.hooks`** — the skip flag is set (B1).
- matcher: `"Bash"` hits `bash` (alias), `"Read"` hits `read_file`, empty hits everything, `mcp__x__y` hits the MCP
  tool, a non-matching hook does not fire; `SessionStart`'s matcher sees `source`.
- SessionStart: plain stdout and JSON `additionalContext` both become context (asserted through the fake model's
  request body); exit 2's stderr becomes context; `exit 7` only notes (B3) and the run answers normally.
- PreToolUse: `deny` JSON blocks — the tool never runs, the model gets `error: …`, `Event::Hook.blocked`; **`allow`
  JSON changes nothing — a `deny` rule still refuses the call** (B2, the load-bearing test); a hook-free run keeps
  the gate as the sole decider (existing tests); exit 2 blocks with stderr.
- Fail-open (B3): a crashing hook and a hook running into its `timeout: 1` produce notes, the tool still runs.
- PostToolUse: a block replaces the model's tool result with the reason.
- Stop: a block feeds the reason back and a second model turn happens (two canned replies from the fake API);
  the second `Stop` payload carries `stop_hook_active: true`.
- core (B4): a scratch `AXIOMATA_HOME` + `AXH_HOME` with a settings.json whose `PreToolUse` hook exits 2 with
  stderr — an `backend: axh` skill run records `stderr` containing that text (the `Notes` collector path); a
  project settings file with hooks puts the skip note into the run's `stderr` too.

## Out of scope (do not build)

`UserPromptSubmit` / `PreCompact` / `Notification` / `SubagentStop` / `SessionEnd`, `type: "prompt"` (LLM) hooks,
async/parallel hook execution, hooks on the opencode/ollama backends and on chat, a hook trust/approval flow (B1
decided against it), hooks in the permission-ask UI (#88), a settings page editor for hooks. Each is a later step.
