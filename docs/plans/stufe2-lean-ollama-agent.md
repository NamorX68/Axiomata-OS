# Plan: Stufe 2 — a lean local agent for connector digests

Status: **COMPLETE — CP1 + CP2 + CP3 + CP4 done** (CP4 2026-09-11). CP1
(`54173bf`): the `[mcp_servers]` config schema, a hand-rolled stdio MCP client
(`crates/axiomata-core/src/mcp/mod.rs`), the `axiomata-cli mcp import` helper —
verified against the real `apple-mail` (27 tools) and `apple-reminders`
(5 tools) servers. CP2 (`e2a2b28`): `AgentBackend::OllamaAgent` +
`crates/axiomata-core/src/agents/ollama_agent.rs` (bounded tool-call loop), the
two `AgentRequest` fields, the `runner` arms, 14 tests incl. two `FakeOllama`
loop tests. **CP3 mechanisms** (`3a63f58`, 2026-09-11):
`local_backend` / `Skill::effective_backend` (provider-driven backend
selection), `prepend_files` + `build_prompt` (feeds `Mail/.topics.md` into the
prompt), per-turn loop `tracing::info!`, `axiomata-cli get-run <id>`, the three
digest `SKILL.md` edits, 6 unit tests. **CP4** (docs catch-up in
`docs/architecture.md` / `CLAUDE.md`, the Settings hint, `num_turns` + a
"final answer" trace line, the `prepend_files` debug trace, the
`build_prompt`-error→`Failed`-record decision, the spend-guard note, and
`axiomata-cli skills reseed [--force]` for the bundled-skill re-seed gotcha).
**CP3 bake-off round 1:** see
`docs/plans/stufe2-cp3-bakeoff.md` — the switch works (all three digests
resolve to `ollama-agent` under `skill_provider = ollama`); `gemma4:e4b-mlx`
clears reminders 2/2, calendar/mail emit broken JSON; `Spark-X2.5-4B` can't
load on Ollama 0.33.3; `granite4.2:8b` / `lfm2.5:8b` pulled 2026-09-11, round 2
pending. What's left is operational, not a checkpoint — see "After CP4".

## Context

The connector digests (`mail-digest`, `calendar-digest`, `reminders-digest`) run as full
`claude -p` agent turns. That is fatal for any local Ollama model:

| model | speed | executes the SOP? |
|---|---|---|
| qwen3:30b, granite4.2:8b, gemma4:12b-mlx | timeout >600 s (~8 steps × ~35–44K-token ctx) | yes |
| lfm2.5:8b | fast | no — greets 2/2 |
| gemma4:e4b-mlx | ~30 s | no — greets ~3/4, treats the whole scaffold as "my briefing" |

Stufe 1 (drop `module-context.md`, ~1.6K tokens) did **not** move the needle locally — the
Claude Code system prompt + the user's `~/.claude/CLAUDE.md` dev-standards still dominate and
the small model never recognises the `SKILL.md` body as its task. Those two we cannot strip
from `claude -p`.

**Outcome wanted:** a code path that sends a local model *only* the skill's SOP + the tool
schemas it needs — no Claude Code system prompt, no dev CLAUDE.md, no module manifest — and
runs a bounded tool-call loop directly. Then a small fast model has a real shot, and the
`mail`-summarization quality question becomes testable on its own.

This is the deliberate exception to CLAUDE.md's "connector = provider = skill, not code" —
but only for the *local* case, and with a clear reason.

## Shape

New `AgentBackend::OllamaAgent` variant. A skill opts in with `SKILL.md`
frontmatter `local_backend: ollama-agent`; the runner picks it over the skill's
declared `backend` **when `config.agents.skill_provider == ollama`** (CP3
mechanism 1) — so one Settings switch moves every connector digest, and cloud
providers revert them to `claude-code`. The backend bypasses the per-role
provider *env* system entirely — it always talks to local Ollama at
`config.agents.providers.ollama.base_url` and takes its model from
`providers.ollama.skill_model`. (`backend: ollama-agent` directly also works,
for an always-local skill.)

```
system  = <preamble>
user    = <prepend_files contents, if any> + SKILL.md body
tools   = MCP tool schemas for the servers referenced by the skill's `allowed_tools`
loop (max ~12 iterations, within timeout_secs):
  POST /api/chat {messages, tools}      (ollama-rs, native tool calling — no Anthropic shim)
  -> model returns tool_calls?  dispatch each via the MCP client, append `tool` messages, repeat
  -> model returns content, no tool_calls?  that's the final output -> RunRecord.stdout
```

Model resolution: reuse the `resolve_skill_model` convention — `SKILL.md` `model:` wins,
else `providers.ollama.skill_model`, else `ollama_model`.

## The real work: an MCP client in Rust

Axiomata speaks MCP **nowhere** today; `claude -p` does it. Stufe 2 needs Axiomata to spawn
the `apple-mail` / `apple-reminders` MCP servers itself and speak the protocol.

- **CP1 decision (made):** hand-rolled stdio JSON-RPC client, **not** `rmcp`. `rmcp` 0.8
  pulls ~75 crates (`darling`, `schemars`, `futures`, `tokio-util`) for exactly three
  methods over newline-delimited JSON-RPC, and it is two majors behind the current 3.x
  (API-churn risk). The shipped client is ~260 lines with zero new dependencies, matches the
  workspace's explicit dependency discipline, and owns the lifecycle guarantees the runner
  needs (kill on completion, per-call timeouts, skip unsolicited notification lines).
  Transport: one JSON-RPC message per line; the reader skips non-JSON / notification lines
  until a matching `id` responds, so a chatty server can't wedge a request.
- **Server definitions:** a new Axiomata-owned `[mcp_servers]` table in `config.toml`
  (`McpServerConfig { command, args, env }`), seeded from the current Claude config once —
  `axiomata-cli mcp import` reads `~/.claude.json`'s well-known `mcpServers` block (only
  `type: "stdio"` entries with a `command`), merging without overwriting. Servers are
  **copied**, never parsed from Claude's file at run time.
- **Per run:** derive the needed servers from the skill's `allowed_tools` prefixes
  (`mcp__apple-mail__…` -> `apple-mail`), spawn only those (stdio subprocess), `initialize`
  handshake, `tools/list`, then `tools/call` in the loop, kill on completion. No pooling in v1.

## Checkpoints

- **CP1 — MCP client.** ✅ Dependency decision (hand-rolled over `rmcp`, see above) + a thin
  wrapper in `crates/axiomata-core/src/mcp/`: spawn a stdio server, initialize, list tools,
  call a tool, shut down. `[mcp_servers]` config schema + `axiomata-cli mcp import` helper.
  Unit tests against an in-binary mock MCP server; verified live against `apple-mail` and
  `apple-reminders`.
- **CP2 — the backend + loop.** `AgentBackend::OllamaAgent { model }`; `AgentBackend::resolve`
  maps `"ollama-agent"`; `provider_label` / `claude_env` -> local-unmetered arms in
  `skills/runner.rs`; a bounded tool-call loop over `ollama-rs` chat + tools + the CP1 MCP
  client. **Full spec: the "CP2 — implementation plan (detail)" section below** (files,
  signatures, the `ollama-rs` 0.3.6 API notes, the loop, the failure→status table, the
  test surface, the settled decisions). CP2 tests: the pure helpers **and** two loop
  tests against a tiny in-repo `FakeOllama`; the failure-matrix branches get live
  coverage at CP3.
- **CP3 — provider switch + wire all three digests + bake-off.** Two runner
  mechanisms so that **`skill_provider = ollama` makes every connector digest
  run on `ollama-agent`, one switch**: (1) `local_backend:` SKILL.md frontmatter
  + `Skill::effective_backend(config)`; (2) `prepend_files:` frontmatter (feeds
  `Mail/.topics.md` into the prompt — unblocks `mail-digest`, which has no file
  tool locally). All three digests get `local_backend: ollama-agent`; `cleanup`
  does not. Plus one `tracing::info!` per loop turn. Then a bake-off under a
  scratch `AXIOMATA_HOME` against real Ollama + MCP over a 4-model roster
  (`gemma4:e4b-mlx` / `gemma4:12b-mlx` / `granite4.2:8b` / `lfm2.5:8b`); pass =
  `Success` + shape-valid stdout + under `timeout_secs`, 2/2 runs (mail summary
  *quality* judged separately). **Full spec: "CP3 — implementation plan
  (detail)" below.**
- **CP4 — docs + polish (last checkpoint). ✅ COMPLETE (2026-09-11).** See "CP4 —
  implementation plan (detail)" below for what shipped: §A docs in
  `docs/architecture.md` + `CLAUDE.md` + the `per-role-provider.md` pointer; §B the
  Settings hint under `skill_provider` when it's `ollama`; §C observability polish
  (`num_turns`, the "final answer" trace line, the `prepend_files` `debug!`); §D the
  `build_prompt` error → recorded `Failed` run (recommended option); §E documented only;
  §F the `axiomata-cli skills reseed [--force]` command.

## CP2 — implementation plan (detail)

**Status: shipped in `e2a2b28`** — `agents/ollama_agent.rs`, the enum variant +
`resolve`/`id`/`run` arms, the two `AgentRequest` fields, the `runner` arms, the
shared `truncate_utf8`, the `mcp::mock_server` promotion, 14 new tests (pure
helpers + the two `FakeOllama` loop tests). Build / `clippy --all-targets -D
warnings` / `fmt` / 259 tests all clean. The plan below is kept as the record of
what was built.

Everything here was checked against the tree at `54173bf` and `ollama-rs` **0.3.6**
(the version in `Cargo.lock`; workspace dep is `ollama-rs = "0.3"`,
`default-features = false`).

### Files touched

| File | Change |
|---|---|
| `Cargo.toml` (root) | **none.** `ollama-rs` is already a dep; `schemars` rides along as its transitive dep (used unconditionally by `ollama_rs::generation::tools`). No new crate. |
| `crates/axiomata-core/src/agents/mod.rs` | `BACKEND_OLLAMA_AGENT` const; `AgentBackend::OllamaAgent { model }` variant; arms in `resolve` / `id` / `run`; **two new `AgentRequest` fields** (below). |
| `crates/axiomata-core/src/agents/ollama_agent.rs` | **new** — the bounded tool-call loop + its pure helpers. |
| `crates/axiomata-core/src/agents/ollama.rs` | **unchanged** — that is the *old* raw-completion `AgentBackend::Ollama`; leave it alone. |
| `crates/axiomata-core/src/skills/runner.rs` | `execute_skill`'s model match arm; `agent_request()` fills the 2 new fields; `provider_label` + `claude_env` get an `OllamaAgent` arm (both → "local, no provider / no env", same as `Ollama`). |
| `crates/axiomata-core/src/error.rs` | extend `UnknownAgentBackend`'s `#[error("… expected \"claude-code\" or \"ollama\"")]` string to also list `"ollama-agent"`. No new variant. |
| `docs/architecture.md` §6, project `CLAUDE.md` backend list | **deferred to CP4.** |

### `AgentBackend` wiring (`agents/mod.rs`)

```rust
pub const BACKEND_OLLAMA_AGENT: &str = "ollama-agent";

pub enum AgentBackend {
    ClaudeCode,
    Ollama { model: String },        // unchanged: single completion, no tools
    OllamaAgent { model: String },   // new: local tool-call loop over MCP
}
```

`resolve` arm — model precedence is **SKILL.md `model:` → `providers[Ollama].skill_model`
→ `agents.ollama_model`** (one tier more than the `Ollama` arm, which skips the middle):

```rust
BACKEND_OLLAMA_AGENT => {
    let provider_default = config.agents.providers
        .get(&ProviderId::Ollama)
        .map(|s| s.skill_model.trim())
        .filter(|m| !m.is_empty())
        .map(str::to_owned);
    let model = model_override.map(str::to_owned)
        .or(provider_default)
        .unwrap_or_else(|| config.agents.ollama_model.clone());
    Ok(Self::OllamaAgent { model })
}
```

`id` → `BACKEND_OLLAMA_AGENT`; `run` → `Self::OllamaAgent { model } => ollama_agent::run(request, model).await`.
`execute_skill`'s `let model = match backend { … }` gets `AgentBackend::OllamaAgent { .. } => None`
(the model lives in the enum, exactly like the `Ollama` arm).

### Getting `[mcp_servers]` + the Ollama URL into the backend

`AgentBackend::run(&self, request: AgentRequest)` is handed no `&Config`. Add two
**backend-agnostic** fields to `AgentRequest`, populated for every backend in
`runner::agent_request`, read only by `ollama_agent`:

```rust
pub struct AgentRequest {
    // …existing…
    /// MCP stdio servers this run may spawn (`ollama-agent` only; a copy of
    /// `config.mcp_servers`). Other backends ignore it.
    pub mcp_servers: std::collections::BTreeMap<String, crate::config::McpServerConfig>,
    /// Local Ollama daemon URL for `ollama-agent`
    /// (`config.agents.providers[Ollama].base_url`); `None` → library default
    /// (`http://127.0.0.1:11434`). Ignored by other backends.
    pub ollama_base_url: Option<String>,
}
```

`runner::agent_request` adds:

```rust
mcp_servers: config.mcp_servers.clone(),
ollama_base_url: config.agents.providers
    .get(&crate::config::ProviderId::Ollama)
    .and_then(|s| s.base_url.clone()),
```

Rationale: this mirrors how `env` and `system_prompt_file` are already
backend-specific `AgentRequest` fields the other backends ignore; it keeps
`AgentBackend::run`'s signature and the enum's contract stable. *Alternative
considered and rejected:* add `config: &Config` to `AgentBackend::run` — touches
every backend arm, every call site, and the public shape of the enum, for one
consumer.

### `ollama-rs` 0.3.6 API — the non-obvious bits

- **Client:** `ollama_rs::Ollama`. `Ollama::default()` = `127.0.0.1:11434`.
  For a configured URL use `Ollama::try_new(url: impl IntoUrl) -> Result<Self, url::ParseError>`.
- **Non-streaming call:** `ollama.send_chat_messages(req: ChatMessageRequest) -> ollama_rs::error::Result<ChatMessageResponse>`.
  `default-features = false` drops the `stream` feature, so `send_chat_messages_stream`
  isn't even compiled — good, no accidental streaming path.
- **Request:** `ChatMessageRequest::new(model_name: String, messages: Vec<ChatMessage>).tools(Vec<ToolInfo>)`.
- **Messages:** `ChatMessage::system(String)` / `::user(String)` / `::tool(String)` /
  `::assistant(String)`. Struct fields are `pub`: `role: MessageRole`,
  `content: String`, `tool_calls: Vec<ToolCall>`, `thinking: Option<String>`, `images`.
  There is **no `tool_name` field** on a tool message in 0.3.6 — tool results are
  positional; fine for our short loops, note it if a model complains.
- **Response:** `ChatMessageResponse { message: ChatMessage, final_data: Option<ChatMessageFinalResponseData>, .. }`.
  `message.tool_calls: Vec<ToolCall>`; `ToolCall { function: ToolCallFunction { name: String, arguments: serde_json::Value } }`.
  `final_data` (present when `done`) carries `prompt_eval_count` / `eval_count` (`u64`).
- **`ToolInfo` from a runtime schema — the trap.** `ToolInfo::new` is generic over a
  *compile-time* `Tool` trait (and `#[ollama_rs::function]` / `Coordinator` likewise),
  so none of the ergonomic paths work for schemas discovered at run time. But
  `ToolInfo` / `ToolFunctionInfo` are `#[derive(Deserialize)]` with `pub` fields and
  `parameters: schemars::Schema` (which deserialises from any JSON object). So build
  each one by deserialising the whole struct:

  ```rust
  let info: ToolInfo = serde_json::from_value(json!({
      "type": "function",
      "function": {
          "name": mcp_tool.name,
          "description": mcp_tool.description.clone().unwrap_or_default(),
          "parameters": mcp_tool.input_schema,   // the raw JSON Schema from tools/list
      },
  })).map_err(|e| AxiomataError::AgentApi { backend: BACKEND, message: e.to_string() })?;
  ```

### `ollama_agent.rs` shape

Entry point mirrors `ollama::run`:

```rust
pub async fn run(request: AgentRequest, model: &str) -> Result<AgentRunResult, AxiomataError>
```

Constants: `const MAX_ITERS: usize = 12;`, `const BACKEND: &str = BACKEND_OLLAMA_AGENT;`.
Reuse `ollama.rs`'s `truncate_utf8` + `MAX_RESPONSE_BYTES` (lift both to
`pub(crate)` in `agents/mod.rs` and share, or copy). `AgentRunResult::bare` is
`pub(crate)` and reachable from this submodule (same as `ollama.rs`).

**Message layout** — mirrors what the Claude Code backend already does (it sends
the `SKILL.md` body as the `-p` prompt, i.e. a user turn, *not* appended to a
system prompt): `system` = the preamble alone, `user` = `request.prompt` (the
`SKILL.md` body). No content-free `"Begin."` filler. If CP3 finds a model still
just greets, the fallback to try is "everything in one `user` turn, no system
message".

Preamble (the whole `system` message):

```
You are a local automation agent running unattended. Carry out the procedure in
the next message exactly, using the tools provided. Never ask questions and never
wait for confirmation. The only tools available are the ones provided — do not
mention or attempt any others. When the procedure is finished, output only the
final result it specifies: no preamble, no explanation, no markdown fences.
```

**Pure helpers — this is the CP2 unit-test surface:**

1. `fn wanted_servers(allowed_tools: Option<&str>) -> Result<BTreeMap<String, BTreeSet<String>>, AxiomataError>`
   — whitespace-split; for each `mcp__`-prefixed token, strip the prefix and
   `split_once("__")` → `(server, bare_tool)`; non-`mcp__` tokens (`Bash(...)`,
   `Edit`, `Write`) are silently ignored (this backend has no shell/file tools);
   a malformed `mcp__…` token → `AgentApi { message: "malformed tool spec '<tok>'" }`.
2. `fn tool_infos(advertised: &[(String /*server*/, McpTool)]) -> Result<Vec<ToolInfo>, AxiomataError>`
   — the `serde_json::from_value` construction above; missing description → `""`;
   **guard:** two entries sharing `McpTool.name` → `AgentApi { message: "tool name 'X' ambiguous across servers" }`
   (bare names are what the model sees; the digests' two servers have disjoint
   names, but fail loud rather than dispatch to the wrong one).
3. `enum Step { Final(String), Calls(Vec<ToolCall>) }` +
   `fn interpret(message: &ChatMessage) -> Step` — `tool_calls` empty → `Final(content.clone())`,
   else `Calls(tool_calls.clone())` (content-alongside-calls counts as `Calls`).
4. `fn dispatch_index(advertised: &[(String, McpTool)]) -> HashMap<String /*bare tool*/, String /*server*/>`.

**The loop** (not pure; the two `FakeOllama` tests in CP2 cover the happy path
and the run-timeout, the failure branches at CP3):

```
run(request, model):
  started = Instant::now()
  wanted  = wanted_servers(request.allowed_tools.as_deref())?

  clients: Vec<(String, McpClient)> = []          // Drop kills each server on any early return
  for name in wanted.keys():
      cfg = request.mcp_servers.get(name)
          .ok_or(AgentApi { backend: BACKEND, message: "no [mcp_servers] entry for {name:?}" })?
      clients.push((name, McpClient::connect(name, cfg, mcp::DEFAULT_CALL_TIMEOUT).await
          .map_err(|e| AgentApi { backend: BACKEND, message: e.to_string() })?))

  advertised: Vec<(String, McpTool)> = []
  for (name, client) in &mut clients:
      for t in client.list_tools().await.map_err(|e| AgentApi { .. })?:
          if !wanted[name].contains(&t.name): continue
          if t.annotations.get("destructiveHint") == Some(&Value::Bool(true)):
              tracing::warn!(server = %name, tool = %t.name,
                  "ollama-agent: dropping a tool the MCP server marks destructive");
              continue                                    // read-only digests; belt & braces
          advertised.push((name.clone(), t))

  infos = tool_infos(&advertised)?
  index = dispatch_index(&advertised)

  ollama = match request.ollama_base_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
      Some(u) => Ollama::try_new(u).map_err(|e| AgentApi { backend: BACKEND,
                     message: "bad ollama base_url {u:?}: {e}" })?,
      None    => Ollama::default(),
  }

  messages = vec![ ChatMessage::system(PREAMBLE.to_owned()),
                   ChatMessage::user(request.prompt.clone()) ]   // SKILL.md body as the task turn

  for _ in 0..MAX_ITERS:
      left = request.timeout.checked_sub(started.elapsed())
          .ok_or(AgentTimeout { backend: BACKEND, timeout: request.timeout })?
      req  = ChatMessageRequest::new(model.to_owned(), messages.clone()).tools(infos.clone())
      resp = match timeout(left, ollama.send_chat_messages(req)).await {
          Ok(Ok(r))  => r,
          Ok(Err(e)) => { shutdown(clients).await; return Err(AgentApi { backend: BACKEND, message: e.to_string() }) }
          Err(_)     => { shutdown(clients).await; return Err(AgentTimeout { backend: BACKEND, timeout: request.timeout }) }
      }
      match interpret(&resp.message):
          Final(text) =>
              { shutdown(clients).await
                return Ok(AgentRunResult::bare(truncate_utf8(text, MAX_RESPONSE_BYTES), String::new(), 0, ms(started))) }
          Calls(calls) => {
              messages.push(resp.message.clone())         // role=assistant, keeps tool_calls
              for call in calls:
                  out = match index.get(&call.function.name) {
                      None          => "error: no such tool {name:?}",
                      Some(server)  => {
                          client = clients.iter_mut().find(|(s,_)| s == server).unwrap().1
                          match client.call_tool(&call.function.name, call.function.arguments.clone()).await {
                              Ok(r) if r.is_error                     => "[tool reported an error] " + r.text,
                              Ok(r)                                   => r.text,
                              Err(AxiomataError::McpTimeout { .. })   => { shutdown(clients).await;
                                  return Err(AgentApi { backend: BACKEND, message: "MCP tool {name} timed out" }) }
                              Err(e)                                  => "[tool call failed] {e}",
                          }
                      }
                  }
                  messages.push(ChatMessage::tool(out))
          }
  shutdown(clients).await
  Err(AgentApi { backend: BACKEND, message: "no final answer within {MAX_ITERS} steps" })
```

`ms(started) = started.elapsed().as_millis() as u64`; `shutdown(cs) = for (_, c) in cs { c.shutdown().await }`.
Optional: instead of `bare(..)`, fill `input_tokens` / `output_tokens` from the last
turn's `resp.final_data.{prompt_eval_count, eval_count}`; `cost_usd` stays `None`
(local, unmetered). Not required for CP2.

### Failure → recorded status

`record_from_result` maps `exit_code == 0` → `Success`, else `Failed`;
`failure_record` (the runner's `Err` arm) stringifies the error → `Failed`.

| Situation | `ollama_agent::run` returns | Recorded |
|---|---|---|
| model replies, no `tool_calls` | `Ok(AgentRunResult::bare(text, "", 0, ms))` | **Success** |
| `allowed_tools` names a server not in `config.mcp_servers` | `Err(AgentApi)` — before any spawn | Failed |
| MCP spawn / handshake / `tools/list` fails | `Err(AgentApi)` (wraps `AxiomataError::Mcp`) | Failed |
| Ollama daemon down / HTTP error | `Err(AgentApi)` | Failed |
| whole-run `timeout` exceeded | `Err(AgentTimeout)` | Failed |
| model calls a tool name that doesn't exist | tool message `"error: no such tool …"`, loop continues | (self-corrects; may still end Success) |
| `tools/call` → `isError: true` | tool message `"[tool reported an error] …"`, continue | — |
| `tools/call` → `Err(Mcp)` (bad JSON-RPC, malformed) | tool message `"[tool call failed] …"`, continue | — |
| `tools/call` → `Err(McpTimeout)` (server hung) | `Err(AgentApi)` — bail, don't thrash | Failed |
| `MAX_ITERS` reached, no final text | `Err(AgentApi)` | Failed |

No new `AxiomataError` variant: `AgentApi { backend: &'static str, message }` and
`AgentTimeout { backend, timeout }` already exist and already carry a `backend`.

### Tests

**Pure-helper unit tests** (`ollama_agent.rs`, no daemon, no MCP server):
- `wanted_servers`: two servers with several tools; non-`mcp__` tokens ignored;
  `None` / `""` → empty map; `"mcp__broken"` → `AgentApi`.
- `tool_infos`: an `McpTool` with a real object `input_schema` deserialises into a
  `ToolInfo` whose re-serialised `function.name` / `function.parameters` match the
  input; `description: None` → `""`; duplicate `name` → `AgentApi`.
- `interpret`: message with `tool_calls` → `Calls`; without → `Final(content)`;
  content + `tool_calls` together → `Calls`.
- `dispatch_index`: each bare tool name maps to its server.
- server missing from `request.mcp_servers` → `run` returns `AgentApi` before it
  spawns anything (near-pure: assert on the error, no daemon needed).

**Loop tests** (decision 4 — do these in CP2, not CP3). Two, against a
hand-rolled fake Ollama; `send_chat_messages` is non-stream so each turn is a
single JSON body, which keeps the fake tiny. Reuse CP1's mock MCP server for the
tool side — **CP2 promotes `mock_server_config` / `mock_server_self_entry` from
`mcp::tests` to a `#[cfg(test)] pub(crate)` helper in `mcp`** so this module can
spawn the same `echo`/`boom` mock.

1. `loop_runs_a_tool_then_returns_the_final_answer` — fake serves turn 1 =
   a `ChatMessageResponse` with one `tool_calls` entry for `echo`
   (`{"text":"ping"}`), turn 2 = content, no `tool_calls`. `AgentRequest` with
   `allowed_tools = Some("mcp__mock__echo")`, `mcp_servers = {"mock": mock_server_config()}`,
   a generous `timeout`. Assert: `Ok`, `stdout` == turn-2 content, `exit_code 0`;
   and (proves the tool result re-entered the context) turn 2's request body,
   captured by the fake, contains a `role:"tool"` message carrying `"ping"`.
2. `loop_aborts_on_the_run_timeout` — fake sleeps 200 ms before replying,
   `AgentRequest.timeout = 100 ms`, one turn. Assert `Err(AgentTimeout)` and that
   the mock MCP server process was reaped (no leak).

```rust
/// A stand-in for a local Ollama daemon: serves a fixed list of canned
/// `/api/chat` responses, one per connection, then the listener closes (a
/// further call gets a connection error → surfaces as `AgentApi`). Same
/// "in-repo fake, no dev-dep" style as CP1's mock MCP server. `delay` is
/// applied before every response — set it past the run timeout to exercise
/// the timeout branch. `seen` captures each request's raw bytes for asserting
/// what the loop sent.
struct FakeOllama {
    port: u16,
    seen: std::sync::Arc<tokio::sync::Mutex<Vec<Vec<u8>>>>,
    _task: tokio::task::JoinHandle<()>,
}

impl FakeOllama {
    async fn spawn(turns: Vec<serde_json::Value>, delay: std::time::Duration) -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let seen_task = seen.clone();
        let task = tokio::spawn(async move {
            for body in turns {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                // Best-effort drain of the request; over loopback one read is
                // enough for a small JSON body and we don't parse it anyway.
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                seen_task.lock().await.push(buf[..n].to_vec());
                tokio::time::sleep(delay).await;
                let body = serde_json::to_vec(&body).unwrap();
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
                let _ = sock.flush().await;
            }
        });
        Self { port, seen, _task: task }
    }
    fn url(&self) -> String { format!("http://127.0.0.1:{}", self.port) }
}
```

A canned turn body is a `ChatMessageResponse` shape — minimally
`{"model":"m","created_at":"t","done":true,"message":{"role":"assistant",
"content":"…","tool_calls":[{"function":{"name":"echo","arguments":{"text":"ping"}}}]}}`
(drop `tool_calls` or pass `[]` for the final turn).

Everything else — the failure-matrix branches (unknown tool name, `isError`,
`Err(Mcp)` vs `Err(McpTimeout)`, `MAX_ITERS` exhaustion) — stays eyeballed +
gets real exercise in **CP3** against live Ollama + MCP under a scratch
`AXIOMATA_HOME`.

### Decisions (settled 2026-09-10, owner)

1. **`AgentRequest` gains the 2 fields** (`mcp_servers`, `ollama_base_url`), *not*
   `&Config` into `AgentBackend::run` — it stays the codebase's "backend-specific
   field other backends ignore" pattern (`env`, `system_prompt_file`, `model`).
2. **`destructiveHint` filter: yes**, and each drop is a `tracing::warn!` (server
   + tool name) so a server that mislabels a read tool is diagnosable.
3. **No `"Begin."` turn.** `system` = preamble only, `user` = the `SKILL.md`
   body — parity with the Claude Code backend's `-p`-prompt shape. CP3 fallback if
   a model still greets: fold everything into one `user` turn.
4. **Loop tests land in CP2**, not CP3 — two, via the tiny hand-rolled
   `FakeOllama` above (happy path + run-timeout). The failure-matrix branches stay
   eyeballed and get live coverage at CP3.

## CP3 — implementation plan (detail)

Checked against the tree at `e2a2b28` (CP2 shipped). CP3 delivers **two small
runner mechanisms + wires all three connector digests to run locally when the
skill provider is Ollama** + a viability/quality bake-off. The risk is not
Rust, it's "does a 4–8B local model actually drive these SOPs".

### Goal (owner, 2026-09-10)

> Selecting **Ollama as the skill provider** must make *all* connector digests
> (`calendar` / `reminders` / `mail`) run on `ollama-agent` — one switch, no
> per-skill fiddling. Switching back to a cloud provider reverts them to
> `claude-code`.

So the digests are **not** hard-flipped to `backend: ollama-agent`. Instead a
skill declares a *local variant* and the runner picks it when
`config.agents.skill_provider == ProviderId::Ollama`.

### Mechanism 1 — provider-driven backend selection

New optional `SKILL.md` frontmatter field **`local_backend`** (a backend id,
in practice `ollama-agent`): "use this instead of `backend` when the configured
skill provider is a local one".

- `skills/registry.rs` — `SkillFrontmatter { local_backend: Option<String>, … }`;
  `skills/model.rs` — `Skill { local_backend: Option<String>, … }`, plumbed
  through `registry`'s builder like `backend` already is.
- `skills/model.rs` — a helper on `Skill`:
  ```rust
  /// The backend id to actually resolve: `local_backend` when the configured
  /// skill provider is local (Ollama today) and the skill declares one, else
  /// the plain `backend`. Keyed on the provider, so one Settings switch moves
  /// every skill that opts in.
  pub fn effective_backend<'a>(&'a self, config: &Config) -> &'a str {
      match self.local_backend.as_deref() {
          Some(local) if config.agents.provider_for(ProviderRole::Skill) == ProviderId::Ollama => local,
          _ => &self.backend,
      }
  }
  ```
- `skills/runner.rs` `execute_skill` — resolve `skill.effective_backend(config)`
  instead of `&skill.backend` (one line). Nothing else in the resolve path
  changes; `AgentBackend::resolve("ollama-agent", skill.model.as_deref(), config)`
  already does the right model precedence (→ `providers[Ollama].skill_model`).
- `execute_prompt` (raw-prompt routines) is untouched — it has no `Skill`, so no
  `local_backend`; a routine that names a *skill* target goes through
  `execute_skill` and gets the switch for free.
- **Reverts cleanly:** provider back to `anthropic`/`open_router` → next digest
  refresh resolves `backend: claude-code` again (config is read per run). No
  restart.
- **Out of scope, unchanged:** a non-digest skill with no `local_backend` (e.g.
  `cleanup`) still routes through `claude -p` when `skill_provider = ollama` —
  same as today, and `cleanup` needs file-editing tools the loop hasn't got
  anyway.
- **Settings:** no new control — the `skill_provider` `<select>` from the
  per-role-provider work is the switch. A one-line hint under it ("Ollama →
  connector digests run locally via the tool-call agent, no cloud cost") is
  nice-to-have; fold into CP4.

### Mechanism 2 — `prepend_files` (unblocks `mail-digest`)

`mail-digest` step 1 tells the agent to *read `Mail/.topics.md` from the
workspace*. `ollama-agent` has **only** the 3 MCP mail tools — no file tool —
so a local run can't see configured topics, and a small model told to "read a
file" with no file tool thrashes. Fix it generically (needed now, since "all
digests" includes mail):

New optional frontmatter **`prepend_files: ["Mail/.topics.md"]`**. In
`execute_skill`, before the prompt is built:

```rust
let mut prompt = String::new();
for rel in &skill.prepend_files {
    // Path-safety: workspace-relative only, no escape.
    if rel.contains("..") || std::path::Path::new(rel).is_absolute() {
        return Err(AxiomataError::InvalidSkill { path: skill_path, reason:
            format!("prepend_files entry {rel:?} must be a workspace-relative path") });
    }
    match std::fs::read_to_string(config.workspace_root.join(rel)) {
        Ok(content) if !content.trim().is_empty() =>
            prompt.push_str(&format!("## Context file: {rel}\n\n{}\n\n---\n\n", content.trim())),
        _ => {}   // missing / empty file → contribute nothing
    }
}
prompt.push_str(&skill.body);
```

Applies to **every backend** — for `claude-code` the digest gets topics inline
instead of reading them, which is strictly fine. Then `mail-digest` step 1 is
reworded once (works for both backends):

> 1. Configured topics, if any, appear at the very top of this message under
>    "Context file: Mail/.topics.md" — one topic per line. If that block is
>    absent, there are no configured topics: skip the per-topic `search_emails`
>    calls and the topic classification in step 4. Never try to open the file
>    yourself.

`calendar-digest` / `reminders-digest` have no file dependency — no
`prepend_files`, no body change (beyond the tool-wording tuning below).

### The SKILL.md edits (`~/.axiomata/skills/`)

All three connector digests get one added frontmatter line:

```
 backend: claude-code
+local_backend: ollama-agent
```

`mail-digest` additionally gets `prepend_files: ["Mail/.topics.md"]` and the
step-1 reword above. Leave `allowed_tools` (now also the `ollama-agent`
server/tool *derivation source*), `timeout_secs: 600`, and — for calendar /
reminders — the SOP body as-is for the first run. Do **not** add a `model:`
line: the model comes from `providers[Ollama].skill_model`, which is what the
owner sets when picking Ollama as the skill provider; a per-skill pin would
fight the bake-off.

SOP tool-wording is a **tuning knob, not a pre-edit**: the SOPs say things like
"the `reminders_lists` tool, action `read`". A native-tool-calling model gets
each tool's real JSON schema (via `tool_infos`), so "action `read`" may not
match a real parameter. First run unchanged; only if a model *systematically*
malforms a call, reword that step to the schema
`cargo run -p axiomata-cli -- mcp tools apple-reminders` prints. Log every body
edit in the bake-off notes.

### Mechanism 3 — one loop-tracing line

A `tracing::info!` at the top of each loop iteration in `ollama_agent::run` —
turn index, and after the response, the tool names called (or "final") — so a
bake-off run under `RUST_LOG=axiomata_core::agents::ollama_agent=info` shows
step count and tool-call sequence per model. ~~`AgentRunResult` from this backend
sets `num_turns: None` (via `bare()`), so the log is the only place step count
surfaces.~~ **Superseded by CP4:** `ollama-agent` now sets `num_turns: Some(turn + 1)`
on its result, so the step count also shows in `get-run`. ~2 lines; no behaviour
change.

### Files touched (CP3)

| File | Change |
|---|---|
| `crates/axiomata-core/src/skills/registry.rs` | `SkillFrontmatter`: `local_backend: Option<String>`, `prepend_files: Option<Vec<String>>` (`#[serde(default)]`). |
| `crates/axiomata-core/src/skills/model.rs` | `Skill`: `local_backend: Option<String>`, `prepend_files: Vec<String>`; `fn effective_backend(&self, &Config) -> &str`. |
| `crates/axiomata-core/src/skills/runner.rs` | `execute_skill`: resolve `skill.effective_backend(config)`; build the prompt with the `prepend_files` prefix loop (path-safety guard). |
| `crates/axiomata-core/src/agents/ollama_agent.rs` | the per-iteration `tracing::info!`. |
| `~/.axiomata/skills/{calendar,reminders,mail}-digest/SKILL.md` | `local_backend: ollama-agent`; mail also `prepend_files:` + the step-1 reword. |
| `crates/axiomata-cli/src/main.rs` | add `get-run <id>` if the bake-off validator wants it (thin wrapper over `runlog::get_run`). Optional. |

No `config.rs` change — the switch is `agents.skill_provider`, which already
exists. Unit tests: `effective_backend` (provider Ollama + `local_backend` set →
local; provider Ollama + no `local_backend` → `backend`; provider Anthropic +
`local_backend` set → `backend`); `prepend_files` (present file prepended,
missing file skipped, `..`/absolute rejected).

### Bake-off procedure

Candidate roster (4, owner-set 2026-09-11 — confirm exact tags with
`ollama list`):

| tag | size | note |
|---|---|---|
| `gemma4:e4b-mlx` | ~4B | round 1: reminders 2/2, calendar/mail broke JSON — the fast baseline |
| `gemma4:12b-mlx` | 12B | the Context table's ">600 s timeout" verdict was under the *old* `claude -p` framing — worth a fresh run now that it's gone; expect it to be the slow one, the 600 s ceiling is the real risk here |
| `granite4.2:8b` | 8B | pulled 2026-09-11 |
| `lfm2.5:8b` | 8B | pulled 2026-09-11 |

All four are local, load on Ollama 0.33.3, and `ollama show` lists `tools`
(native `/api/chat` tool calling — what the loop needs) **and** `thinking`.
Spread is 4B / 8B / 8B / 12B — if none of the 8B/12B clear a digest that
`gemma4:e4b-mlx` also fails, that digest is cloud-only for now.

**Tuning knob — `think: false`.** All four are reasoning models and think by
default; the loop doesn't set `ChatMessageRequest::think(...)`, so every turn
pays reasoning latency + tokens, ×up to 12 turns, against the 600 s ceiling.
The digests are mechanical extract-and-shape tasks, not reasoning tasks. If a
model times out or thrashes, first retry with `.think(false)` on the request
(one line in `ollama_agent::run`); note per-model whether it helped or hurt
JSON validity.

- **`SparkLLM/Spark-X2.5-4B:latest`** (owner suggestion, 2026-09-10) — 4.11B,
  ~1M ctx, markets "strong agent/coding" + tool use. **Two gates before it can
  even enter the bake-off:** (1) stock Ollama does **not** yet support the
  `spark2_5` architecture — `ollama pull`/`run` fails on a mainline install
  until that lands (or the owner runs a patched build); (2) native
  `/api/chat` tool-call emission is *not* confirmed on its model card — the
  `ollama-agent` loop needs it. Put it **last** in the bake-off order and try
  it only once gate 1 clears; if the loop shows zero `tool_calls` in the trace,
  it's out regardless of gate 1.

Setup once per bake-off session (the `local_backend` edits are already in the
repo; the scratch config just needs the provider switch + a topics file):

```sh
export AXIOMATA_HOME=$(mktemp -d)                       # fresh DB
cargo run -p axiomata-cli -- mcp import                 # seeds [mcp_servers] from ~/.claude.json
mkdir -p "$AXIOMATA_HOME/workspace/Mail"                # or point workspace_root at the real vault
printf 'Fotografie\nDevelopment\nKI/AI/LLM\n' > "$AXIOMATA_HOME/workspace/Mail/.topics.md"
# edit $AXIOMATA_HOME/config.toml:
#   workspace_root = "…/workspace"           (or the real ~/Documents/vault)
#   [agents]
#   skill_provider = "ollama"                # <- the switch: digests now resolve to ollama-agent
#   [agents.providers.ollama]
#   base_url = "http://localhost:11434"
#   skill_model = "M"                        # <- the candidate, per run
```

Per candidate model M, for **all three** digests (`calendar-digest`,
`reminders-digest`, `mail-digest`), run **2×** each:

```sh
RUST_LOG=axiomata_core::agents::ollama_agent=info \
  cargo run -p axiomata-cli -- run-skill mail-digest
cargo run -p axiomata-cli -- list-runs --limit 5        # status, duration_ms, provider (empty = local)
```

Sanity-check the switch itself once: with `skill_provider = "ollama"` a run's
`backend` column is `ollama-agent`; flip to `skill_provider = "anthropic"` and
the same skill's next run is `claude-code`.

Record per run: **status** (Success/Failed + error), **wall-clock** vs 600 s,
**step count / tool sequence** (from the log), **stdout shape-valid?**:

- `calendar-digest`: object with `calendars: string[]`, `events[]` of
  `{id,title,start,end,calendar,location,allDay}` — or the documented
  `{"calendars":[],"events":[],"error":"…"}`. The dashboard parser strips one
  ` ```json ` fence, so a fenced reply still passes; anything else non-parseable
  fails.
- `reminders-digest`: `{lists: string[], tasks:[{id,title,list,notes,dueDate,priority}]}`,
  `priority ∈ none|low|medium|high`.
- `mail-digest`: `{emails:[{id,sender,subject,date,reason,topic,summary}]}`,
  `reason ∈ important|topic`, `topic` a string from `.topics.md` or `null`. Plus
  the **quality** read: are `summary` fields real 1–2-sentence gists (not
  subject restatements)? Compare against an OpenRouter baseline run of the same
  inbox.

Validator: `list-runs --limit 1` → id → `get-run <id>` → `jq -e`.

### Success criteria (CP3 done)

- The **switch works**: `skill_provider = ollama` ⇒ all three digests resolve to
  `ollama-agent`; back to a cloud provider ⇒ `claude-code`. `effective_backend`
  + `prepend_files` unit tests green.
- `calendar-digest` + `reminders-digest`: **`Success`**, **shape-valid** stdout,
  **under `timeout_secs`**, on **≥1** candidate model, 2/2 runs.
- `mail-digest`: same bar for *shape* (valid JSON, right keys) on ≥1 model, 2/2.
  Summary **quality** is judged separately by the owner against the OpenRouter
  baseline — a shape-pass with weak summaries still counts as "wired", with a
  note that quality needs a bigger model or stays on cloud.
- Winning model(s) + every SOP-body edit recorded in
  `docs/plans/stufe2-cp3-bakeoff.md` and `docs/plans/per-role-provider.md`'s
  Stufe 2 pointer.
- If a digest clears shape on **no** candidate: it keeps `local_backend:
  ollama-agent` in frontmatter (so the switch still targets it once a model
  works) but the bake-off notes flag it as "cloud-only for now", and the owner
  keeps `skill_provider` on cloud. The mechanism shipped regardless.

### Repo artefacts vs report

- **In the repo:** the three `SKILL.md` `local_backend:` additions (+ mail's
  `prepend_files:` and step-1 reword), the two runner mechanisms + their unit
  tests, the `tracing::info!` line, any SOP tuning edits, and
  `docs/plans/stufe2-cp3-bakeoff.md` with the results table.
- **No e2e test file.** The repo has no e2e harness; real-backend checks are run
  by hand under a scratch `AXIOMATA_HOME` (CLAUDE.md convention). CP2's
  `FakeOllama` tests cover the loop glue; CP3's residual branches (multi-call
  turns, `MAX_ITERS`, `is_error`) get eyeballed during the bake-off.

## CP4 — implementation plan (detail)

**Status: shipped (2026-09-11).** §B–§F decision summaries: the `build_prompt`
path error is mapped to a recorded `Failed` run (recommended option, §D); the
spend-guard corner is documented only (architected away, §E); the bundled-skill
re-seed gotcha ships as `axiomata-cli skills reseed [--force]` over the new
`skills::reseed_default_skills(force)` (§F). The plan below describes the work
as specified.

Checked against `3a63f58` (CP3 mechanisms shipped). CP4 is **the last
checkpoint of Stufe 2** — docs to catch up with reality, a one-line Settings
hint, small observability polish, and one deploy gotcha. No behaviour change of
substance. After CP4, Stufe 2 is a complete feature; what's left (below,
"After CP4") is operational, not a checkpoint.

### A. Docs — the bulk of CP4

`docs/architecture.md`:
- §"Agent backends (`agents/`)" — the `enum` is now
  `AgentBackend { ClaudeCode, Ollama { model }, OllamaAgent { model } }`. Add a
  bullet for `ollama_agent.rs`: the bounded `POST /api/chat` tool-call loop
  (`ollama-rs` native tools, non-stream, `MAX_ITERS` + `timeout`), the CP1 MCP
  client it drives, `shutdown` on every exit path. Note the two new
  `AgentRequest` fields (`mcp_servers`, `ollama_base_url`) other backends ignore.
- §"Model providers" — add: `skill_provider = ollama` no longer just redirects
  `claude -p`; a skill with `local_backend:` frontmatter *resolves to a
  different backend* (`ollama-agent`) under that provider, via
  `Skill::effective_backend`. The three connector digests opt in; `cleanup`
  does not.
- New short §: **`local_backend` + `prepend_files`** (or fold into "Model
  providers") — the CP3 mechanisms, one paragraph each, pointing here.
- line ~91 "agent backend dispatch (Claude Code / Ollama)" → "… / Ollama /
  ollama-agent".
- §5 "what exists" + §7 milestone history — add the Stufe 2 landing
  (CP1 MCP client + `[mcp_servers]`; CP2 `OllamaAgent`; CP3 the switch +
  `prepend_files`; bake-off ongoing). This is exactly the "update it when a
  milestone lands" the file's own maintenance note asks for.

Project `CLAUDE.md` ("traps worth knowing"):
- backend list / commands: add `mcp import|list|tools`, `get-run`, the
  `ollama-agent` backend, `local_backend:` / `prepend_files:` frontmatter.
- the "connector = provider = skill, not code" trap gets its Stufe 2 exception
  spelled out: for the *local* case a digest runs a real Rust loop
  (`ollama-agent`), selected by `skill_provider = ollama` + `local_backend`.

`docs/plans/per-role-provider.md` — its "Stufe 2" pointer: mark CP1–CP3 done,
point at `stufe2-cp3-bakeoff.md` for model status.

### B. Settings hint (small UI)

`apps/dashboard/src/shell/Settings.svelte` (~line 423, under the
`skill_provider` `<select>`): a hint line shown only when
`config.agents.skill_provider === "ollama"`, e.g. *"Connector digests run
locally via the tool-call agent — no cloud cost."* All colours/sizes via
`--ax-*` tokens, no literals (theme rule). Static text — `devmock.ts` needs
nothing. `npm run check` must stay clean.

### C. Observability polish (from the CP3 review)

In `agents/ollama_agent.rs`:
- set `num_turns: Some((turn + 1) as u32)` on the returned `AgentRunResult`
  instead of `bare()`'s `None`, so step count shows in `list-runs` / `get-run`
  without `RUST_LOG` (the loop counter is right there).
- an explicit `tracing::info!(turn, "ollama-agent: final answer")` on the
  `Step::Final` branch (right now "final" is inferred from the *absence* of a
  "dispatching" line).

In `skills/runner.rs` `build_prompt`:
- `tracing::debug!(rel, "prepend_files: skipping a missing/unreadable context file")`
  in the `_ => {}` arm, so a typo'd path is diagnosable instead of silently
  yielding no context.

### D. `build_prompt` error path — decide

A bad `prepend_files` entry (`..` / absolute) makes `execute_skill` return
`Err(InvalidSkill)`, so `execute_and_record_skill` persists **no run** —
unlike the unknown-backend case, which records a `Failed` run the dashboard
can show. Moot for the three digests (paths are correct + tested); matters
only for a future misconfigured skill.
- **Minimum:** a line in `execute_skill`'s doc comment — `prepend_files` path
  errors join the "resolution failure → `Err`, nothing recorded" bucket.
- **Or:** map it to `failure_record(&skill.name, backend_id, …)` like the
  unknown-backend arm, for dashboard visibility. Cheap; slightly more
  consistent. Recommended if touching the file anyway.

### E. Spend guard — document, don't fix (unless trivial)

`guard_redirected_turn` runs in `execute_and_record_skill` *before* the
backend is resolved. With the `local_backend` design this is a non-issue on
the intended path: `skill_provider = ollama` ⇒ the guard checks the Ollama
provider, whose recorded spend is ~0 (`ollama-agent` runs record
`provider: None`) ⇒ guard passes. It only bites a `SKILL.md` that hard-codes
`backend: ollama-agent` *while* `skill_provider` is a paid provider over its
cap — a corner the mechanism is designed to avoid.
- **CP4:** a one-paragraph note in `docs/architecture.md` §"Model providers"
  (spend bullet) is enough.
- **If fixing:** move the `guard_redirected_turn` call out of
  `execute_and_record_skill` into `run_on_backend` (right after
  `provider_label`), and skip it for `AgentBackend::Ollama | OllamaAgent`.
  Check the routine scheduler's own guard call (`routines::scheduler`) too —
  it has the same shape. Not required for CP4.

### F. Deploy gotcha — bundled-skill re-seed

`skills::seed_skill` is **seed-if-absent** (`create_new`; an existing
`SKILL.md` is left untouched). So the `resources/` edits (`local_backend:` on
all three digests, `prepend_files:` + the step-1 reword on `mail-digest`)
**do not reach an install whose `~/.axiomata/skills/<name>/SKILL.md` already
exists** — only a fresh `AXIOMATA_HOME` picks them up. The owner's machine is
already hand-synced (verified in the CP3 session); the bake-off's scratch
`AXIOMATA_HOME` gets them via the seed. But this needs one of:
- **Minimum:** a line in `CLAUDE.md` / the architecture doc — "changing a
  bundled skill's `resources/SKILL.md` requires re-copying it into
  `~/.axiomata/skills/` on any existing install; the seed won't."
- **Or (recommended if cheap):** `axiomata-cli skills reseed [--force]` — a
  thin CLI over a new `skills::reseed_default_skills(force: bool)` that, with
  `--force`, overwrites the bundled four from `resources/` (leaving
  non-bundled skills alone). Generally useful beyond Stufe 2.

### CP4 done when

Docs (§A) updated; the Settings hint (§B) ships and `npm run check` is clean;
§C polish applied; §D and §E decisions recorded (a note is a valid outcome);
§F handled (note or `reseed` command). `cargo fmt` / `clippy --all-targets -D
warnings` / `cargo test --workspace` clean; `cd apps/dashboard && npm run
check` clean. Update this plan's status line and check off CP4.

### After CP4 — operational, not a checkpoint

Stufe 2 is a complete feature at that point. What remains is *running the
procedure*, not building:

1. **Bake-off round 2** — `granite4.2:8b` / `lfm2.5:8b` (pulled 2026-09-11) +
   a `gemma4:12b-mlx` retry, per `stufe2-cp3-bakeoff.md`'s "next round". Try
   the `think: false` knob if a model is slow. Judgement call, not code.
2. **Per-digest go/no-go** — for each of the three digests, does *any* local
   model clear shape + timeout 2/2? If yes, that digest can go local; if no,
   it stays cloud-only (its `local_backend` line is harmless — the switch just
   won't have a working target). Then the owner decides whether to flip their
   live `skill_provider` to `ollama`.
3. **`mail-digest` summary quality** — only meaningful once a model produces
   non-empty `emails[]`; compare against the OpenRouter baseline.

Separately: **M4 (always-on / background scheduling)** from the original M0–M6
milestone plan is still unimplemented and is unrelated to Stufe 2.

## Non-goals (v1)

Streaming; parallel tool calls; MCP server pooling; a lean loop against *cloud* providers
(possible later — cheaper minimal-prompt calls — but out of scope); `cleanup` on the new
backend.

## Risks

- ~~MCP server config discovery~~ — **resolved in CP1**: Axiomata owns `[mcp_servers]`,
  seeded once from `~/.claude.json` via `axiomata-cli mcp import`, never parsed at run time.
- Small-model tool-arg formatting: the one gemma4:e4b-mlx run that engaged (session run #230)
  reported "tool calls failed". Native `ollama-rs` tool calling removes the Anthropic-shim
  variable; whether the model itself formats args cleanly is a CP3 finding. CP2's loop
  tolerates it — a bad/unknown tool call comes back to the model as a `tool` message
  (`"error: no such tool …"` / `"[tool call failed] …"`) rather than aborting the run.
- `ToolInfo` runtime construction: `ollama-rs` 0.3.6 has no non-generic constructor, so CP2
  deserialises the whole struct from JSON (see the API notes above). If a future `ollama-rs`
  bump changes `ToolFunctionInfo` / `schemars::Schema`, that one call site breaks loudly at
  compile time — acceptable, and pinned by `Cargo.lock`.
- Summarization quality on a 4–8B model is still unproven — but for the first time testable
  without the instruction-following wall in the way. If no local model summarizes mail
  acceptably, the fallback stays `skill_provider = open_router` and Stufe 2 still pays off
  for calendar/reminders (pure tool-call + shape).
