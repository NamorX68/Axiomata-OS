# Plan: Stufe 2 — a lean local agent for connector digests

Status: **CP1 + CP2 done.** The `[mcp_servers]` config schema, a
hand-rolled stdio MCP client (`crates/axiomata-core/src/mcp/mod.rs`), and the
Claude-Code import helper (`axiomata-cli mcp import`) are shipped and verified
against the real `apple-mail` (27 tools) and `apple-reminders` (5 tools) servers
(commit `54173bf`). CP2 (the `OllamaAgent` backend + tool-call loop) is fully
spec'd below — see **"CP2 — implementation plan (detail)"** — and implemented
(`AgentBackend::OllamaAgent`, `crates/axiomata-core/src/agents/ollama_agent.rs`,
the two `AgentRequest` fields, the `runner` arms, the pure-helper + `FakeOllama`
loop tests). CP3 (wire the digests, live quality check) is next. Follows Stufe 1
(drop `module-context.md` from skill runs, shipped in commit `42fc45d`).

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

New `AgentBackend` variant, selected per skill by `SKILL.md` frontmatter
`backend: ollama-agent` (opt-in; `claude-code` stays the default and the fallback). It
bypasses the per-role provider system entirely — always talks to local Ollama at
`config.agents.providers.ollama.base_url`.

```
system  = <preamble> + SKILL.md body
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
- **CP3 — wire the digests.** `backend: ollama-agent` in the 3 read-only digest `SKILL.md`s
  (leave `cleanup` on `claude-code` — it edits files). End-to-end: run each under a scratch
  `AXIOMATA_HOME` against real Ollama + real MCP servers; assert valid JSON, under
  `timeout_secs`; this is also where the loop's glue (history accumulation, timeout
  budgeting, shutdown-on-every-path) first gets real coverage. Compare `mail-digest`
  summary quality against the OpenRouter baseline (run #235-style output) with 2–3
  candidate models (gemma4:e4b-mlx, and re-test granite4.2:8b / lfm2.5:8b now that the
  framing is gone — **both still need `ollama pull`**).
- **CP4 — docs + default.** `docs/architecture.md` §6 "Agent backends" (three variants now)
  and the connector-module note; CLAUDE.md's backend list. Decide: flip the digests' default
  to `ollama-agent` (with `claude-code` fallback if Ollama is down), or keep it opt-in via a
  config toggle. Update `docs/plans/per-role-provider.md`'s "Stufe 2" pointer.

## CP2 — implementation plan (detail)

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
