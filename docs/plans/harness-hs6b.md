# HS6b — `backend: axh` in the core: skills through the own harness

Self-contained instructions for step HS6b of the harness (`docs/plans/harness.md`, card #37). Written 2026-10-14
for the owner's approval; builds on HS6 (`harness-hs6.md`, commit `7136e24`). Owner decisions are marked
**[owner]**, points the planner fixed are **[default]** — build them as written, do not redesign.

## Where and how

As in HS6 ("Where and how"): worktree `.claude/worktrees/harness`, branch `worktree-harness`, gate
`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, one commit
`feat(core): run skills through the own harness, step HS6b (#37)`, no push. HS6b is the **allowed** dependency
direction (core → harness; the harness must never depend on core, H2). No schema migration: the `runs` table
already has `backend`, `model`, token and cost columns.

## Decisions

- **B1 [owner] — no silent fallback.** `backend: axh` resolves the endpoint from
  `config.agents.providers[agents.skill_provider]` as it is. A provider without a `base_url` (Anthropic, the
  current skill default, has neither base URL nor key and the harness has no Anthropic client yet — H4) yields a
  recorded `Failed` run with a clear message ("backend axh needs a provider with a base URL (open_router or
  ollama); the skill provider anthropic has none — set it in Settings → Providers"). The run never falls back to
  opencode: a run that says `axh` in `runs.backend` ran on the own harness or not at all. OpenRouter without an
  API key is refused the same way (as the CLI's `harness run` does).
- **B2 [owner] — MCP only from the harness's own config.** An axh run reads `~/.axh/mcp.json` and the project's
  `.mcp.json` through `mcp::config::load` — never `~/.config/opencode/opencode.json`. Owner-level servers run
  without approval; a project server is never asked about (headless: `mcp::approval::permitted(…, can_ask=false)`
  leaves it out with the M2 note). Connector skills (`mail-digest` & Co.) therefore need their `apple-*` servers
  copied into `~/.axh/mcp.json` once, by the owner.
- **B3 [owner] — unattended, deterministic permissions.** The run's gate is built fresh: `Mode::Default`, asker
  `Nobody`, `RuleSet.allow` = the skill's declared `allowed_tools` (parsed with the harness's own `rules_of`), and
  the same list as the gate's narrowing (`Gate::narrow("the skill <name>", …)`). So *what the skill declares is
  allowed, and what it does not name is refused before anything is asked* — including file reads, once the skill
  declares anything at all. A skill that declares nothing runs with the mode default: reads free, everything that
  changes something asks and is refused by `Nobody`. The settings (`settings::load`, owner + project) only ever
  narrow: their sandbox `denyRead` applies to the shell, and — owner, 2026-10-18, after the security review — their
  `deny` and `ask` rules join the gate (an `ask` is a refusal, nobody answers), so a skill cannot declare its way
  past the owner's `deny: Bash(rm:*)`. Their `allow` rules and mode do **not** feed the gate: they would widen a
  skill beyond its declaration. The sandbox (Seatbelt) is on wherever the system has it.
- **B4 [owner] — no bundled skill switches.** Nothing changes for existing skills; `backend: axh` is opt-in per
  `SKILL.md` frontmatter, written by the owner for the skills they want on the own harness.

## Defaults (planner)

- **D1 — the core keeps building the prompt.** `skills::runner::build_prompt_at` (date line, `prepend_files`
  inline as untrusted data, `SKILL.md` body) produces the run's **user message**, exactly as opencode receives it
  today ("The skill's own instruction body is the prompt on every backend"). The harness's own `Skill::start_run`
  (K1.1) is *not* used — it would inline the body a second time. The harness contributes what it is built for:
  loop, tools, gate, sandbox, MCP. `RunConfig.instructions` stays `None`.
- **D2 — the model travels bare.** `resolve_run_model` for `Axh`: frontmatter `model:` else
  `agents.providers[skill_provider].skill_model`, with **no** `provider/` prefix (the harness posts the string as
  `model` to `{base_url}/v1/chat/completions`). `stored_model` is therefore the bare id already, so
  `metered_cost_usd` (keyed by bare model) and `runs.model` work unchanged. A skill on axh names its model as the
  API does (`deepseek/deepseek-chat`, not `openrouter/deepseek/...`).
- **D3 — no JSON repair turn on axh.** The harness has no sessions across runs yet (H3), so the opencode backend's
  second `ask_for_json` turn has no equivalent; `output: json` skills stay on opencode for now (their
  `reported_failure` check still applies — it runs on every backend). Limitation, not a bug.
- **D4 — `system_prompt_file` is ignored**, like the opencode *run* path ignores it (only chat prepends the module
  manifest; skill runs never set it).
- **D5 — the same 4-slot `agent_slots` semaphore** as every other backend, taken before the tools/MCP are built.
- **D6 — the timeout** is `AgentRequest::timeout` (`skill.timeout_secs` else `agents.skill_timeout_secs`, already
  resolved by the runner) into `RunConfig.timeout`; `FinishReason::Timeout` maps to `exit_code 1` with
  `AxiomataError::AgentTimeout { backend: "axh", … }`'s text in stderr → the usual `Failed` record. Changed in
  the review (2026-10-18): first built as an `Err`, which dropped the tokens spent before the deadline, so the
  daily cap never saw what a timed-out run cost.
- **D7 — one collector sink.** A small `EventSink` keeps what the record needs: `Event::Error` messages, failed
  MCP servers, refused calls (`Event::Permission { allowed: false }`) → joined into `AgentRunResult::stderr`.
  The reply (`RunOutcome.answer`) becomes `stdout`, usage becomes the token columns, `outcome.turns` the
  `num_turns`. No per-event DB plumbing — the `runs` JSONL line gets the whole `RunRecord` as always.
- **D8 — `provider_label` for `Axh` = the skill provider's label** (same as opencode: axh bills through that
  provider's key), so `runs.provider` is filled and `guard_redirected_turn` / the daily cap keep working.
- **D9 — `runs.backend` = `"axh"` verbatim**, no migration.

## What to build

1. **`axiomata-harness` (two small public additions, no behaviour change)**
   - `skills::rules_of(text) -> Result<Vec<Rule>, String>` made `pub` — one string of rules as the harness reads
     them (split outside parentheses, each part parsed); the core parses a skill's `allowed_tools` with it (B3).
   - `permissions::settings::home_dir() -> Result<PathBuf, HarnessError>` — the harness's home folder: `$AXH_HOME`
     when set (must be absolute), else `$HOME` (absolute), else a clear `Settings` error. Mirrors the repo's
     `AXIOMATA_HOME` test override so core tests never read the real `~/.axh` (and never start real MCP servers).
     Implemented as a pure `home_from_env(axh_home, home)` + a thin wrapper, so the resolution is unit-testable
     without touching the environment.
2. **`crates/axiomata-core/Cargo.toml`** — `axiomata-harness = { path = "../axiomata-harness" }` (path dep, like
   the other sibling crates); `[dev-dependencies] tokio = { workspace = true, features = ["net"] }` for the fake
   model server of the end-to-end test.
3. **`crates/axiomata-core/src/error.rs`** — new variant `Harness(#[from] axiomata_harness::HarnessError)`,
   display `"harness: {0}"` (the Board/Ide/Roster seam pattern: an external crate's error entering through its own
   variant). `UnknownAgentBackend`'s message becomes `expected "opencode", "ollama" or "axh"`.
4. **`crates/axiomata-core/src/agents/mod.rs`**
   - `pub const BACKEND_AXH: &str = "axh";`, `AgentBackend::Axh` variant (doc: the own harness,
     `axiomata-harness`, an OpenAI-compatible endpoint of the skill provider), arms in `resolve` / `id`.
   - `AgentBackend::run(&self, request, config)` — one new parameter: `config` supplies the skill provider's
     endpoint to the `Axh` arm; opencode and ollama ignore it. Sole caller: `skills/runner.rs::run_on_backend`.
   - `AgentRequest` gets `pub name: Option<String>` — the run's name (skill/routine), used as the gate's
     `restrict_name` so a refusal reads `the command /mail-digest allows only: …` (the harness's own wording for
     skills, K5). Threaded through `request_for` / `agent_request` from `run_on_backend`, which already has it.
5. **`crates/axiomata-core/src/agents/axh.rs` (new, the heart)** — `pub async fn run(request, config)` after
   `opencode::run`'s shape:
   1. `endpoint(config, request.model.clone())` per **B1** → `ModelEndpoint { base_url, api_key, model }`;
      errors are `AxiomataError::InvalidAgentModel` (they surface as the misconfigured-skill/failed-record paths).
   2. `home = settings::home_dir()?` (step 1), then take an `agent_slots()` permit (**D5**).
   3. `gate_for(request.name, request.allowed_tools)` per **B3** (fresh `Gate::new`, `Mode::Default`, `Nobody`,
      allow + restrict = the declared rules, none → restrict off).
   4. `sandbox = settings::load(&home, &cwd, false)?.sandbox` — settings read for the sandbox only (B3);
      project settings are never trusted in an unattended run.
   5. `Catalog::load(&home, &cwd, /* trusted */ false, &[paths::global_skills_dir()])` — notes go to
      `tracing::warn!`, never fail the run (K3). The catalog feeds the `skill:<name>/…` read roots (K4) and the
      `skill` tool (offered while the catalog is non-empty, K1.2).
   6. `OpenAiCompat::new(endpoint)`; `Toolbox` = file tools + `SkillTool` + `tools.set_gate(...)` + MCP (B2,
      `permitted(…, false)`) + shell with the sandbox — the same order and building blocks as the CLI's
      `harness_cmd::run`, minus slash commands, trust prompts and every interactive ask.
   7. `axiomata_harness::run(&run_config, &model, &tools, &prompt, &mut sink, &cancel)` with
      `RunConfig { cwd, timeout: request.timeout, skill_dirs: [global_skills_dir], ..new }`, `instructions: None`,
      no signal handlers (`Cancel::new()`, never signalled — this process is the app, not `axh`).
   8. Map the outcome per **D6/D7**: `Answered` → `exit_code 0`, stdout = answer; `MaxTurns`/`Failed`/
      `Cancelled`/`Timeout` → `exit_code 1` with the reason in stderr (D6); tokens
      (`0 → None`), `num_turns`, `duration_ms`; `cost_usd: None` — `run_on_backend` meters it (D2).
6. **`crates/axiomata-core/src/skills/runner.rs`** — the four seams: `backend.run(request, config)`;
   `resolve_run_model` gains the `Axh` arm (D2); `provider_label` gains `Axh` (D8); `request_for`/`agent_request`
   pass the run's `name` onto the request. Doc comments that list the backend ids get `axh` added
   (`execute_prompt`, `registry::Skill::backend`, `skills/model.rs` `RunRecord::backend`).
7. **Docs in the same commit** — this file; `docs/plans/harness.md`: roadmap row HS6b ✅, a section
   "HS6b — `backend: axh` in the core (decided 2026-10-14)" with B1–B4 + defaults, "HS6b — what exists";
   `docs/architecture.md` status line + §5 backend ids; `AGENTS.md` `HS1–HS6b built, HS7–HS8 open` and the
   "Backend ids" bullet `opencode (default) | ollama | axh`.

## Tests (inline, all must pass)

- harness: `rules_of("Bash(git status:*) Read, Edit")` → 3 rules; `home_from_env` — `AXH_HOME` wins and must be
  absolute, `$HOME` fallback, neither → error.
- resolve: `"axh"` → `Axh`, `id()` = `"axh"`; retired ids still map to opencode; unknown id still errors.
- `resolve_run_model` (Axh): frontmatter `model:` wins; else the skill provider's `skill_model`; neither →
  `InvalidAgentModel`. `provider_label` (Axh) → the skill provider.
- `endpoint` (B1): openrouter with URL+key → endpoint; anthropic-style (no base URL) → error naming the skill
  provider; openrouter without key → error; ollama without key → endpoint; no model → error.
- `gate_for` (B3): declared `Read` → `judge` allows `read_file`, refuses an undeclared `bash` with the
  `allows only: …` reason; no declaration → reads allowed (mode default), a bash command refused (`Nobody`);
  unparsable `allowed_tools` → `InvalidSkill` carrying the run's name.
- collector (D7): `Error`, failed `McpServer` and denied `Permission` events land in stderr text; `Started`/
  `Usage`/allowed permissions do not.
- misconfigured (B1): `backend: axh` on a default config (Anthropic, no URL) records a `Failed` run whose error
  names the missing base URL — no model call, no `Err`.
- **end to end (the proof)**: a scratch `AXIOMATA_HOME` **and `AXH_HOME`** under the `ENV_MUTEX` fixture, a
  `SKILL.md` with `backend: axh`, `config.agents.skill_provider = open_router` pointed at a **fake
  `POST /v1/chat/completions` server on the loopback** (raw tokio listener, like the harness's `FakeApi`) with
  `api_key`, `skill_model = "test-model"` and a `costs["test-model"]` price. `execute_and_record_skill` must
  return a `Success` record and persist a `runs` row with `backend = "axh"`, `provider = "open_router"`,
  `model = "test-model"`, 10 in / 2 out tokens, the metered cost and `stdout = "ok"`; the fake server must have
  seen the bearer key, `model: "test-model"` and the skill's body in the messages. No model cost, no real
  network, no real `~/.axh`.

## Out of scope (do not build)

An Anthropic client for the harness (H4 — B1 exists precisely because of that gap), sessions/streaming/events in
the DB, `execute_prompt`/routines gaining `axh` (the routine store keeps validating `opencode`/`ollama` only),
chat going through the harness, the JSON repair turn (D3), switching any bundled skill (B4), `~/.config/opencode/
opencode.json` being read anywhere (B2), a Studio/TUI view of the harness events. Each is a later step.
