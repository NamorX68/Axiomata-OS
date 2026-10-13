# HS8 — subagents: build instructions

Self-contained instructions for building step HS8 of the harness (`docs/plans/harness.md`, card #37). Written
2026-10-13 for another model to build without the conversation. Build HS4–HS7 first (`harness-hs4.md` …
`harness-hs7.md`); HS8 starts from their commits. Owner decisions (2026-10-13) are marked **[owner]**, points the
planner fixed are **[default]** — build them as written, do not redesign.

## Where and how

As in HS4 ("Where and how"): worktree `.claude/worktrees/harness`, branch `worktree-harness`, no `axiomata-core`
dependency, gate `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`,
one commit `feat(harness): subagents, step HS8 (#37)`, no push. Frontmatter with the workspace's `gray_matter`, as in
HS6. Read `src/run.rs` (the loop), `src/tools/mod.rs` (`Toolbox`, `CallContext`), `src/event.rs` and HS6's
`src/skills/` (same file layout and trust rules) before starting.

## Decisions

- **A1 [owner] — definitions like Claude Code, plus one built in.** Markdown files `~/.axh/agents/<name>.md` (the
  owner's) and `<project>/.axh/agents/<name>.md` (only in a trusted folder; the project's beats the owner's on equal
  names). Frontmatter: `name` (required, equals the file stem, `[a-z0-9-]{1,64}`), `description` (required — the
  parent model reads it to choose), `tools` (optional: a list or comma-separated string of tool names; omitted = all
  tools the parent has, minus `task`), `model` (optional; `inherit` or omitted = the parent's model). Other keys are
  ignored. The body is the subagent's instructions. A broken file is skipped with a note, never fails the run;
  at most 100 definitions, 64 KiB each (named consts). Built in, no file: **`general`** — every tool of the parent
  except `task`, instructions: work on the task with the tools, then answer with a concise report of what was found
  or done (the report is all the parent sees). A file named `general.md` is skipped with a note.
- **A2 [owner] — a subagent never has more rights than its parent.** It shares the parent's `Arc<Gate>` (same rules,
  mode, `restrict` of a command or skill, same asker — the owner is asked the same way; HS3a's `asking` mutex already
  serialises asks from parallel subagents), the same sandbox settings and the same hooks (`PreToolUse` /
  `PostToolUse` fire for its tool calls; `SessionStart`, `UserPromptSubmit`, `Stop` do not — they belong to the
  parent's run). `tools` only narrows: a tool not listed is **not offered** to the subagent at all. Names in `tools`
  may be the harness's (`bash`, `read_file`) or Claude Code's (`Bash`, `Read`, … — the mapping of HS7's E5), plus
  `mcp__<server>` for all of a server's tools or `mcp__<server>__<tool>`; an unknown name is skipped with a note.
- **A3 [owner] — parallel, depth 1.** A subagent has no `task` tool. When one answer of the parent model contains
  several `task` calls, they run at the same time — at most 4 at once (named const), the rest wait; other tool calls
  of that answer keep running one after the other, in order, and every result goes back to the model in the original
  call order.
- **A4 [default] — a subagent's own state.** Its own message history (it never sees the parent's conversation, only
  its prompt), its own shell session (its own folder carry-over and background jobs, all ended when it finishes —
  parallel subagents would otherwise move each other's working folder), shared MCP clients (`Arc`, one process per
  server for the whole run), shared skill catalog. Limits: `max_turns` 30 (named const, never above the parent's), the
  parent's remaining overall time, the parent's stop/cancel ends it at once.
- **A5 [default] — the `task` tool.** Parameters `{"description": string (3–8 words, for display), "prompt":
  string, "subagent_type": string (optional, default "general")}`. Its description lists every available agent as
  `name: description` (each cut at 300 chars, the list at 8 000, like HS6's `skill` tool). `access()` is
  `Access::Own` — starting a subagent is not an action; everything it does is judged as usual (A2). Result: the
  subagent's final answer (cut like every tool result), then a line
  `[subagent <type>: <turns> turns, <in>/<out> tokens, <reason>]`. A subagent that ends on its turn limit, timeout or
  an error still returns what it has, with the reason; an unknown `subagent_type` is a tool error listing the names.
- **A6 [default] — events and usage.** `Event::Subagent { id, kind, description, status: "started" | "finished",
  turns, tokens_in, tokens_out, reason }` (`id` = `"sa-1"`, `"sa-2"`, … per run). Every event a subagent produces is
  sent as `Event::FromSubagent { id, event: Box<Event> }` (serialized `{"type": "from_subagent", "id": …, "event":
  {…}}`), so a front end can nest it. Its tokens are added to the parent's `usage` and to `finished`'s totals. The
  CLI's readable output indents a subagent's lines by two spaces under `▸ subagent <id> <kind>: <description>`.

## What to build

1. **`src/agents/mod.rs`** — `AgentDef { name, description, tools: Option<Vec<String>>, model: Option<String>,
   instructions: String, source }`; `AgentCatalog::load(home, project, trusted) -> (AgentCatalog, Vec<String>)`
   including `general` (A1).
2. **A tool factory.** The `task` tool must build a fresh toolbox for each subagent (A4). Add to `src/tools/mod.rs`:
   `pub trait ToolFactory: Send + Sync { fn build(&self) -> Result<Toolbox, HarnessError>; }` — the caller (`axh`,
   the CLI) implements it with what it already builds (file tools, `shell_tools` with the same options, the shared
   MCP tools, the skill tool), **without** `task`. Then filter by the definition's `tools` (A2) and set the parent's
   gate and hooks on it.
3. **`src/agents/task.rs`** — the `task` tool (A5): resolve the type, build the toolbox, run a nested `run::run`
   with a `RunConfig` derived from the parent's (A4 limits, model from A1), a sink that wraps events (A6), and the
   parent's stop flag; return the report.
4. **`src/run.rs`** — run consecutive `task` calls of one answer concurrently (A3): a `JoinSet` + `Semaphore(4)`;
   keep results in call order. Sum subagent usage into the run's totals (A6).
5. **`event.rs`** — `Event::Subagent`, `Event::FromSubagent` (A6).
6. **`bin/axh.rs` / `harness_cmd.rs`** — load the catalog with the other settings, implement `ToolFactory`, add the
   `task` tool to the parent's toolbox — always, whatever the mode: its subagent is held by the same gate (A2) —
   and the readable output (A6).
7. **Docs** in the same commit — `docs/plans/harness.md`: row HS8 ✅, section "HS8 — subagents (decided
   2026-10-13)" with A1–A6 and "HS8 — what exists"; status line in `docs/architecture.md`; `AGENTS.md` status
   `HS1–HS8 built` (HS6b open).

## Tests (inline, all must pass)

Use the fake model server the existing `run.rs`/`model.rs` tests use; script it so the parent calls `task` and the
subagent answers.

- catalog: owner + trusted project merged, project wins; untrusted project skipped; `general.md` skipped; missing
  `description` skipped; `tools` as list and as string; `model: inherit`.
- task: the subagent gets only the prompt (not the parent's messages); its final text and the summary line come back;
  unknown `subagent_type` → tool error with the names; turn limit → partial report with reason.
- rights: a subagent with `tools: Read` is not offered `bash` (check the tool list it was sent); in `plan` mode a
  subagent's `write_file` is refused; a deny rule of the parent holds for the subagent; the subagent has no `task`.
- parallel: three `task` calls in one answer run concurrently (prove it with a fake model that waits until all three
  requests arrived, with a timeout), at most 4 at once with six calls, results in call order.
- state: two parallel subagents each `cd` into a different subfolder and `pwd` — each sees its own; a subagent's
  background job is gone after it finishes.
- cancel: stopping the parent ends running subagents.
- events: `from_subagent` wraps a subagent's `tool_call`; `subagent` started/finished; tokens added to the parent's
  totals.

## Out of scope (do not build)

Subagents starting subagents, background subagents that outlive their `task` call, resuming a subagent, its own
permission rules, `SubagentStop` hooks, Axiomata's `AGENT.md` roles as subagents, a subagent on another provider or
key. Each is a later step.
