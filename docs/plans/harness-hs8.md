# HS8 — Subagents: a `task` tool starting a nested run

Self-contained instructions for step HS8 of the harness (`docs/plans/harness.md`, card #37). Written 2026-10-14
for the owner. Owner decisions are marked **[owner]**, points the planner fixed are **[default]** — build them as
written, do not redesign.

## Where and how

As in HS6/HS6b/HS7: worktree `.claude/worktrees/harness`, branch `worktree-harness`, gate
`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, one commit
`feat(harness): subagents, step HS8 (#37)`, no push. H2 unchanged (no harness → core dependency); the core wiring
below is core → harness, as in HS6b/HS7.

A run gains one tool, `task`. Calling it starts a **nested run**: its own system prompt, its own conversation, its
own limits — on the parent's model, with the parent's toolbox, gate, hooks and `Cancel`. The tool's result is the
subagent's final answer, so the parent's context stays clean while the work happens.

## Decisions (owner, 2026-10-14)

- **B1 [owner] — `task` is judged like a change.** The call is `Access::Other` under the tool name `task`: rules
  (`allow task` / `ask task` / `deny task`) decide first, then the mode — `default`/`accept_edits` **ask**
  (an interactive answer; headless with nobody there: `no one to ask`, refused), `auto` allows, `plan`/`dont_ask`
  refuse. Spawning a subagent is therefore *opt-in*: a run switches it on through `allow task` (owner settings,
  `--allow task`, a role's permissions), through `--permission-mode auto`, or through an interactive answer;
  `deny task` blocks it everywhere. A narrowed run (a command's or skill's `restrict`) needs `task` declared *and*
  still passes the normal judgement — a declaration alone never allows (HS6b-B3, C3). No new `Access` variant and
  no new mode behaviour: every existing statement about rules and modes holds unchanged.
- **B2 [owner] — the subagent gets exactly the parent's toolbox, minus `task`.** Same `Toolbox` (same gate
  instance, same shell/MCP/skill sessions, same sandbox), same model, same working folder. No per-call tool
  selection and no agent types in HS8 — `subagent_type`, roles and tool lists come with M7.5. One level only: a
  subagent is never offered `task`, so nesting deeper than one cannot start; a hallucinated `task` call falls into
  the ordinary `call_until` and gets `error: there is no tool named "task"`.
- **B3 [owner] — the core offers `task` only to skills that declare it.** In an unattended run (`backend: axh`)
  the frontmatter decides: `RunConfig.subagents` is `true` only when `task` appears in the skill's
  `allowed_tools` — a skill that declares nothing (most of them) never even sees the tool. Because the core's gate
  takes `allowed_tools` as its *allow rules* **and** its narrowing (HS6b-B3), the same declaration is at the same
  time the permission: declared = offered and allowed, undeclared = neither. The `~/.axh` rules and mode never feed
  the core gate, so they add nothing here. In the CLI/`axh` the layers stay independent: `subagents` defaults to
  `true` (offering) and the gate alone (B1) decides whether a call passes — a human started that run.

## Defaults (planner)

- **D1 — the tool.** Name `task`, in a new **private** module `crates/axiomata-harness/src/task.rs` (the loop's
  own tool, not a caller-facing API): the `ToolSpec` — description "Start a subagent for a self-contained piece of
  work. It runs in its own context — yours stays clean — with the same tools, permissions and folder, and returns
  only its final answer; give it everything it needs in `prompt`, it sees nothing else." — and two required
  string arguments: `description` (a short label for the event stream) and `prompt` (the subagent's whole user
  message). A missing or empty one answers
  `error: the task needs a non-empty description and prompt`; the run goes on. The name `task` is reserved at
  depth 0 — a caller's own tool of that name would be shadowed (documented here, not guarded).
- **D2 — where it runs: in the loop, replacing only the `call_until` step.** The rest of a call's path stays
  exactly as it is (`ToolCall` event, `MAX_CALLS_PER_TURN`, `PreToolUse`, gate-log drain, `ToolResult` event,
  `PostToolUse`, the tool message). The arm itself:
  1. parse the arguments (D1);
  2. if the toolbox has a gate: `gate.judge("task", &Access::Other)` — a refusal ends the arm with
     `error: <reason>` exactly as `call_until` would; **the parent drains the gate's judgement log right after the
     judgement and before the nested run starts** (the nested run drains the same log in its own loop — left
     waiting, the parent's `task` judgement would surface inside the subagent's events and vanish from the
     parent's);
  3. run the nested run (D3) through the wrapper of D8 with a copy of the config: `instructions: None`,
     `max_turns`/`timeout` from D6, the effective timeout additionally capped by the parent's remaining time
     (`deadline − now`), everything else — cwd, hooks, sandbox, skill dirs, `max_output_tokens` — identical, and
     the same `Cancel`;
  4. after it returns: `usage += nested.usage`, so the parent's `Finished` carries the subagent's tokens too (the
     spend metering stays honest); gate judgements the nested run left behind (it cancelled between judging and
     draining) are drained **wrapped as `Sub` events**, so nothing surfaces on the wrong level;
  5. map the outcome to the tool result: `Answered` → `true` with the answer, cut to
     `MAX_TOOL_OUTPUT_CHARS` (an empty answer → `true` with `the subagent finished without text`);
     `MaxTurns`/`Timeout`/`Cancelled`/`Failed` → `false` with
     `error: the subagent stopped without answering: <reason>` — the parent's own checks end it on the next turn
     (they share the cancel token and the deadline). The nested call goes through `Box::pin` (async recursion).
- **D3 — `run()` keeps its signature; the nesting lives in `run_inner(..., depth)`.** Depth 0 = today's run plus
  the `task` spec appended to `specs` (and into `Event::Started.tools`) and D2's interception; `SessionStart`
  hooks fire at depth 0 only. Depth ≥ 1 = no `task` spec, no interception, everything else identical. Public
  `run()` calls `run_inner(..., depth: 0)`. `RunConfig.subagents == false` behaves like a run built without the
  spec and without the interception (D7).
- **D4 — the subagent runs the owner's hooks.** `PreToolUse`/`PostToolUse`/`Stop` fire inside the nested run as
  in any run — HS7-B2's veto must hold for a subagent too, it works with the run's tools; `SessionStart` does
  **not** repeat (it belongs to the run, not to a subagent). The nested run's hook payloads carry its own
  `session_id`, generated exactly as today, so a hook sees a distinct session per subagent.
- **D5 — the subagent's own prompt** (the roadmap's "its own prompt"): `prompt::subagent_prompt(cwd, date)` — a
  fixed short system prompt: you are a subagent inside a larger run on the user's machine; do only the task in the
  user message; use the tools and read a file before you change it; when the task is done answer with the result
  alone, no preamble — the agent that sent you continues from your answer; then `Working folder:` and `Today:`.
  The nested messages are `[system, user(prompt)]`: the parent's `RunConfig.instructions` are **dropped** for the
  subagent (they are the caller's contract with the parent; isolation is the point — the parent's model passes
  along whatever context the subagent needs).
- **D6 — limits**, as named consts in `config.rs` and `RunConfig` fields with those defaults:
  `DEFAULT_SUBAGENT_MAX_TURNS = 15` and `DEFAULT_SUBAGENT_TIMEOUT = 5 min`. The subagent's turns count only
  against its own limit (D2.3 caps its time by the parent's remaining time); the parent's `Finished.turns` stays
  its own model calls while its `Finished.usage` includes the subagent's tokens (D2.4).
- **D7 — availability is `RunConfig.subagents: bool`, default `true`.** `false` = no spec, no interception. The
  CLI/`axh` keep the default; the core sets it per B3.
- **D8 — events: `Event::Sub { depth: u32, event: Box<Event> }`.** Everything the nested run emits arrives
  wrapped — `Started`, `Usage`, `AssistantText`, `ToolCall`, `ToolResult`, `Permission`, `Hook`, `Skill`, `Error`
  and its own `Finished`, nothing dropped; the inner field serialises as **`inner`** (the outer tag already owns
  `event`): `{"event":"sub","depth":1,"inner":{…}}`. The wrapper is a small `SubSink` in `run.rs` borrowing the
  parent's sink; accounting never counts events — D2.4 sums the `RunOutcome`. The parent's `ToolCall(task)` …
  `ToolResult(task)` bracket the whole nested stream. Consumers:
  - **CLI `Readable`**: the match moves into `fn render(&Event) -> Option<String>` (`Usage` → `None`, as today);
    `Event::Error` stays a stderr line, also inside a `Sub` (with its indent); the `Sub` arm renders the inner
    event indented by `"  " × depth`, nested wrappers cumulatively, except a sub's `Finished`, which reads
    `↳ subagent · {turns} turns · {in}/{out} tokens` so a second finished line can never be mistaken for the
    run's own.
  - **`axh`'s JSON lines**: unchanged — serde handles the nesting.
  - **core `Notes`**: the collector's match becomes a function it calls recursively for a `Sub`; a line from
    inside a subagent is prefixed `subagent: `.
- **D9 — hook alias**: `claude_alias("task") = Some("Task")`, so a `PreToolUse`/`PostToolUse` matcher written for
  Claude's `Task` tool fires on ours (HS7-D2 extended).
- **D10 — core wiring (B3)**: a small `pub` method `Rule::names_tool(&self, name: &str) -> bool` in
  `permissions` (kind `Tool`, that tool name, no pattern — exactly what `restrict_allows` decides for such a call);
  `core/agents/axh.rs` sets `RunConfig.subagents = request.allowed_tools`' parsed rules containing
  `names_tool("task")` — the same `rules_of` list it already feeds the gate with — and leaves the gate
  construction itself untouched. The `Notes` recursion of D8 is the only other core edit.

## What to build

1. **`event.rs`** — `Event::Sub` (D8) + its JSON test; **`config.rs`** — `subagents`, `subagent_max_turns`,
   `subagent_timeout` with the two consts (D6/D7).
2. **`task.rs` (new, private)** — the spec and the argument check (D1).
3. **`prompt.rs`** — `subagent_prompt` (D5); **`permissions/mod.rs`** — `Rule::names_tool` (D10);
   **`hooks.rs`** — the `task → Task` alias (D9).
4. **`run.rs`** — `run_inner(depth)` behind the unchanged `run()`, the `task` arm with its judgement ordering
   (D2), the nested `RunConfig` (D2.3), usage aggregation, `SubSink` (D8), the depth rules (D3).
5. **`crates/axiomata-cli/src/harness_cmd.rs`** — the `Readable` refactor and the `Sub` arm (D8).
6. **`crates/axiomata-core/src/agents/axh.rs`** — the `subagents` flag from the frontmatter (B3/D10) and the
   `Notes` recursion (D8).
7. **Docs in the same commit** — this file; `docs/plans/harness.md` (roadmap HS8 ✅, an `HS8 — subagents`
   decisions section, an `HS8 — what exists`); `docs/architecture.md` (§5 harness paragraph + status log);
   worktree `AGENTS.md` (`HS1–HS8 built` + one trap line about the two layers: offering vs judging).

## Tests (inline, all must pass)

- **arguments**: a `task` call without `description`/`prompt` (or an empty one) → the tool error, no nested model
  call (the scripted model's call count stays at one).
- **the happy path**: script `[task call → subagent's text answer → parent's final answer]`; the parent answers
  with the final text; the stream holds `Sub { depth: 1, … }` events (the subagent's `AssistantText` and its
  `Finished`) beside exactly **one unwrapped** `Finished`; the parent's `Finished.usage` is the exact sum of all
  three replies's tokens (7/1 + 10/2 + 10/2 = 27/5); `Event::Started.tools` contains `task` at depth 0 and the
  subagent's request's tool list does not (the scripted model records the specs it was offered).
- **B1**: `deny task` → refused, no nested call, a `Permission` event with `allowed: false` and `tool: "task"`;
  mode `default` + `Nobody` + no rule → refused (`no one to ask`); `allow task` → the subagent runs. The other
  modes' behaviour is `Access::Other`'s, already pinned by HS3a's tests.
- **restrict**: a narrowed run whose rules do not name `task` refuses it, naming the command (HS6b-B3); with
  `task` declared *and* an `allow task` rule it runs — the two layers of B1 in one test.
- **hooks inside** (D4): a `PreToolUse` hook blocking `echo` blocks the subagent's own call —
  `Sub { … Hook { blocked: true } }` in the stream and `error: …` in the subagent's next message; a
  `SessionStart` hook fires **once** in a run that used `task`.
- **limits** (D6): `subagent_max_turns = 1` → the subagent's single turn calls a tool, its second turn hits the
  limit → the parent gets `error: the subagent stopped without answering: MaxTurns` and finishes normally with its
  next reply; `subagent_timeout = 0` → the subagent never reaches the model (its replies stay queued), the
  parent continues.
- **cancel**: a test tool that fires the run's `Cancel` from inside the subagent → the subagent stops, and the
  parent run ends `Cancelled`.
- **D7**: `subagents: false` → no `task` in the offered specs; a `task` call gets
  `error: there is no tool named "task"`.
- **core (B3)**: the end-to-end skill run's request body has **no** `task` tool for a skill whose frontmatter
  declares no `task`; with `allowed_tools: Read task` the body offers it (the gate's allow side of the same
  declaration is pinned by HS6b's own test).
- **event/CLI/permissions**: `Event::Sub` serialises as `{"event":"sub","depth":1,"inner":{…}}`; `render` indents
  a nested event, writes a sub's finished line as `↳ …`, and returns `None` for `Usage`; `Rule::names_tool` on a
  plain rule, a patterned rule and a non-tool rule; `claude_alias("task") == Some("Task")`; `subagent_prompt`
  carries the working folder and the date.

## Out of scope (do not build)

Parallel `task` calls in one turn (the loop runs a reply's calls one after the other — by design), subagent
types/roles (`subagent_type`), a per-call tool list (B2 declined), per-subagent models, nesting deeper than one
level (B2), `SubagentStop`-style hooks, presenting `Event::Sub` in the Studio/app, subagents on the
opencode/ollama backends — and the roadmap's next backend items (sessions over several turns, streaming, the
Anthropic client). Known limit, documented not fixed: events a tool sends through its own sink handle (the
`skill` tool's load event) arrive at the top level rather than wrapped — they are neither turned off nor
miscounted, only not indented.
