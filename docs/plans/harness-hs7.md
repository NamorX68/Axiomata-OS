# HS7 — hooks: build instructions

Self-contained instructions for building step HS7 of the harness (`docs/plans/harness.md`, card #37). Written
2026-10-13 for another model to build without the conversation. Build HS4–HS6 first (`harness-hs4.md` …
`harness-hs6.md`); HS7 starts from their commits. Owner decisions (2026-10-13) are marked **[owner]**, points the
planner fixed are **[default]** — build them as written, do not redesign.

## Where and how

As in HS4 ("Where and how"): worktree `.claude/worktrees/harness`, branch `worktree-harness`, no `axiomata-core`
dependency, gate `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`,
one commit `feat(harness): hooks, step HS7 (#37)`, no push. Format reference: Claude Code's hooks (the owner's own
`~/.claude/settings.json` uses them — read it for real examples, never change it). Process handling: copy the
patterns of `src/tools/shell.rs` (process group, `kill_on_drop`, cleaned environment, output cap).

## Decisions

- **E1 [owner] — the core events of Claude Code:** `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
  `Stop`. No others in HS7.
- **E2 [owner] — where hooks are written.** A `hooks` section in `~/.axh/settings.json` (the owner's) and in the
  project's `.axh/settings.json` / `.axh/settings.local.json`. Claude Code's format, exactly:
  ```json
  {"hooks": {"PreToolUse": [{"matcher": "Bash|Edit", "hooks": [{"type": "command", "command": "…", "timeout": 60}]}]}}
  ```
  `matcher` is a regex over the tool name (only for the two tool events; empty or `*` = every tool); `type` is only
  `command` in HS7; `timeout` in seconds (default 60, at most 600 — named consts). Extend `settings.rs`'s file struct
  (it has `deny_unknown_fields`, so the `hooks` key must be added there or every file with hooks fails to load).
- **E3 [default] — a project's hooks run only after the owner approved them.** They execute commands outside the
  sandbox (E4), so trusting the folder is not enough: the project's whole `hooks` sections (both files together) are
  approved at once, keyed by project path + SHA-256 of their canonical JSON; any change asks again. Reuse the approval
  code of HS5 (`src/mcp/approval.rs`): move it to `src/approval.rs` with a `kind` (`"mcp"` / `"hooks"`) in the key,
  keep HS5's file and behaviour working (its tests must stay green), store hook approvals in
  `~/.axh/hooks-approved.json`. The CLI shows every hook command (through `permissions::printable`) and asks y/n;
  headless leaves unapproved project hooks out and says so on stderr. The owner's hooks need no approval.
- **E4 [owner] — hooks run outside the sandbox,** like Claude Code: `$SHELL -c <command>` (`/bin/sh` if unset), in
  the working folder, stdin = the event JSON, environment = the cleaned tool environment plus `AXH_PROJECT_DIR` and,
  for scripts written for Claude Code, `CLAUDE_PROJECT_DIR` (both the working folder). Own process group, killed on
  timeout and with the run; stdout and stderr each capped at 64 KiB.
- **E5 [default] — input, Claude-compatible.** JSON on stdin with `session_id` (the run's id), `cwd`,
  `hook_event_name`, `transcript_path` (empty string — the harness writes no transcript yet), and per event:
  `SessionStart`: `source: "startup"`; `UserPromptSubmit`: `prompt`; `PreToolUse`: `tool_name`, `tool_input`;
  `PostToolUse`: `tool_name`, `tool_input`, `tool_response` (`{"output": …, "ok": bool}`); `Stop`: `stop_hook_active`.
  `tool_name` uses Claude Code's name where one exists, so existing scripts work: `bash`→`Bash`, `read_file`→`Read`,
  `write_file`→`Write`, `edit_file`→`Edit`, `glob`→`Glob`, `grep`→`Grep`, `list_dir`→`LS`; others (`bash_output`,
  `kill_shell`, `skill`, `mcp__…`) as they are. A matcher is held against both names (`Bash` and `bash` match).
- **E6 [default] — output, Claude-compatible.** Exit 0: success; for `SessionStart` and `UserPromptSubmit` stdout is
  added to the model's context (a user message `Hook context:` + text before the prompt). Exit 2: blocking — stderr
  is the reason: `PreToolUse` refuses the call (the model gets the reason as the tool result), `PostToolUse` gives the
  reason to the model after the result, `UserPromptSubmit` stops the run before any model call, `Stop` makes the run
  go on with the reason as a new user message. Any other exit: a non-blocking error (event + stderr line), the run
  goes on. Instead of exit codes a hook may print JSON on stdout with exit 0: `{"continue": false, "stopReason": …}`
  ends the run; `{"decision": "block", "reason": …}` blocks like exit 2; for `PreToolUse`
  `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow" | "deny" | "ask",
  "permissionDecisionReason": …}}`; `additionalContext` in `hookSpecificOutput` adds context like stdout above.
  Unparseable JSON on exit 0 → treated as plain stdout.
- **E7 [default] — a hook never beats a deny rule.** `PreToolUse` runs **before** the gate. Its `deny` refuses
  outright; its `ask` makes the gate ask even where a rule allows; its `allow` counts like an allow rule for this one
  call — it cannot beat a deny rule, a command's or skill's `restrict`, `plan` mode, the protected names or an opaque
  command (those are still refused or asked). This deliberately differs from Claude Code, where `allow` bypasses the
  permission system.
- **E8 [default] — order and loops.** Several matching hooks of one event run one after the other in file order
  (owner's first, then the project's); the first block wins and the rest are skipped. A `Stop` hook may make the run
  go on at most 3 times (named const) — after that `stop_hook_active` is true and a block is ignored with a note.
  Every hook run is an event `Event::Hook { event, command (printable, cut to 200 chars), outcome: "ok" | "blocked"
  | "error" | "timeout", ms }`.

## What to build

1. **`src/hooks/config.rs`** — the types of E2 (`HookEvent` enum, `Matcher` with a compiled regex, `HookCommand`),
   parsing from the settings files, merged owner-then-project, project part only if approved (E3).
2. **`src/hooks/run.rs`** — `Hooks::fire(event, input: Value) -> Outcome` running the matching commands (E4–E6,
   E8); `Outcome { blocked: Option<String>, context: Vec<String>, stop_run: Option<String>,
   permission: Option<(Decision, String)> }`.
3. **Wiring in `src/run.rs`** — `SessionStart` once before the first model call; `UserPromptSubmit` with the prompt
   (after HS4's command expansion); `PreToolUse` / `PostToolUse` around each tool call in `Toolbox::call` (the gate
   learns E7's hook decision through a new optional argument or a field on `CallContext` — keep the gate's existing
   API working for callers without hooks); `Stop` when the model answers without a tool call.
4. **`bin/axh.rs` / `harness_cmd.rs`** — load hooks with the settings, the approval prompt (E3) in the CLI.
5. **`Event::Hook`** (E8) in `event.rs`; the CLI's readable output prints `⚙ hook <event>: <outcome>`.
6. **Docs** in the same commit — `docs/plans/harness.md`: row HS7 ✅, section "HS7 — hooks (decided 2026-10-13)"
   with E1–E8 and "HS7 — what exists"; status line in `docs/architecture.md`; `AGENTS.md` `HS1–HS7 built`.
   Note in the section that the Studio's live status (a later step) will listen to these events.

## Tests (inline, all must pass)

Write the hook commands as small shell scripts in the test's temp folder.

- config: Claude's format parses; unknown `type` → error; bad regex → error; `timeout` above 600 → capped.
- approval: unapproved project hooks skipped; approved run; a changed command → asked again; HS5's MCP approvals
  still work after the move.
- input: each event's JSON has the fields of E5; `tool_name` is `Bash` for `bash`; matcher `Bash` and `bash` both
  match.
- exit codes: 0 with stdout on `UserPromptSubmit` → context added; 2 on `PreToolUse` → call refused with stderr as
  reason, tool never ran; 2 on `PostToolUse` → reason reaches the model; 2 on `Stop` → run goes on, and stops after
  3 times; exit 1 → error event, run goes on; timeout → process group killed, `timeout` outcome.
- JSON: `continue: false` ends the run; `permissionDecision: deny` refuses; `ask` makes a rule-allowed call asked
  (headless: refused); `allow` lets an otherwise-asked call through **but not** one a deny rule covers, nor in
  `plan` mode.
- environment: `AXH_PROJECT_DIR` and `CLAUDE_PROJECT_DIR` set; the harness's API key variable is absent.

## Out of scope (do not build)

Other events (Notification, PermissionRequest, SubagentStop, PreCompact, SessionEnd, …), `prompt`-type or HTTP
hooks, hooks in skill frontmatter, running hooks in the sandbox, a transcript file, the Studio's status view. Each is
a later step.
