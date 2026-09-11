//! The `ollama-agent` backend: a bounded tool-call loop over a local Ollama
//! model and the MCP servers a skill's `allowed_tools` names.
//!
//! This is the "lean local agent" of Stufe 2
//! (`docs/plans/stufe2-lean-ollama-agent.md`): the model receives *only* the
//! skill's SOP as the user turn plus the tool schemas it needs — no Claude
//! Code system prompt, no dev `CLAUDE.md`, no module manifest — and drives the
//! MCP server(s) itself via native Ollama tool calling (`POST /api/chat` with
//! a `tools` array, non-streaming). The loop is bounded by [`MAX_ITERS`] turns
//! and by `request.timeout`, whichever comes first.
//!
//! ```text
//! system = PREAMBLE
//! user   = request.prompt                     (the SKILL.md body)
//! loop (max MAX_ITERS turns):
//!       POST /api/chat { messages, tools }        (ollama-rs native tool calling)
//!   -> tool_calls? dispatch each via the MCP client, append `tool` messages
//!   -> plain content? that is the final output (carries `num_turns`)
//! ```
//!
//! Every early return shuts the spawned MCP servers down (see [`shutdown`]),
//! so nothing outlives the run. Implemented in Stufe 2 CP2.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

use ollama_rs::Ollama;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::tools::{ToolCall, ToolInfo};
use serde_json::json;
use tokio::time::timeout;

use super::{
    AgentRequest, AgentRunResult, BACKEND_OLLAMA_AGENT, MAX_RESPONSE_BYTES, truncate_utf8,
};
use crate::error::AxiomataError;
use crate::mcp::{self, McpClient, McpTool};

/// Error attribution backend name — matches the `SKILL.md` frontmatter
/// identifier and [`AgentRunResult`]'s `backend` column.
const BACKEND: &str = BACKEND_OLLAMA_AGENT;

/// Upper bound on assistant turns per run. A summarizing model that keeps
/// calling tools is either working (a long SOP) or thrashing; either way the
/// run must end, and a failed run beats an infinite one.
const MAX_ITERS: usize = 12;

/// The whole `system` message. `user` carries the `SKILL.md` body as the task
/// turn — the same shape the Claude Code backend's `-p` prompt has.
const PREAMBLE: &str = "You are a local automation agent running unattended. \
Carry out the procedure in the next message exactly, using the tools provided. \
Never ask questions and never wait for confirmation. The only tools available \
are the ones provided — do not mention or attempt any others. When the \
procedure is finished, output only the final result it specifies: no preamble, \
no explanation, no markdown fences.";

/// Runs the `ollama-agent` loop for `request` against local Ollama with `model`.
///
/// `request.allowed_tools` names the MCP servers to spawn (via their
/// `mcp__<server>__<tool>` prefixes); `request.mcp_servers` supplies their
/// spawn config and `request.ollama_base_url` the daemon URL (`None` → the
/// library default).
///
/// A non-zero `exit_code` is never set here: every failure surfaces as `Err`
/// (the runner records it as a `Failed` run).
///
/// Errors:
///     [`AxiomataError::AgentApi`] for a malformed `allowed_tools`,
///     a missing `[mcp_servers]` entry, an MCP spawn/handshake/tool-list or
///     call failure, a bad Ollama `base_url`, or an Ollama API error;
///     [`AxiomataError::AgentTimeout`] when `request.timeout` elapses before
///     the model produces a non-tool answer.
pub async fn run(request: AgentRequest, model: &str) -> Result<AgentRunResult, AxiomataError> {
    let started = Instant::now();
    let wanted = wanted_servers(request.allowed_tools.as_deref())?;

    // Drop kills each server on any early return; `shutdown` is the clean
    // close-stdin-then-wait path.
    let mut clients: Vec<(String, McpClient)> = Vec::new();
    for name in wanted.keys() {
        let cfg = request
            .mcp_servers
            .get(name)
            .ok_or_else(|| AxiomataError::AgentApi {
                backend: BACKEND,
                message: format!("no [mcp_servers] entry for {name:?}"),
            })?;
        let client = McpClient::connect(name, cfg, mcp::DEFAULT_CALL_TIMEOUT)
            .await
            .map_err(|e| AxiomataError::AgentApi {
                backend: BACKEND,
                message: e.to_string(),
            })?;
        clients.push((name.clone(), client));
    }

    // Advertise only the tools the skill actually allowed, dropping anything
    // the server itself marks destructive (belt & braces on a read-only digest
    // run; each drop is traced so a mislabelled read tool is diagnosable).
    let mut advertised: Vec<(String, McpTool)> = Vec::new();
    for (name, client) in &mut clients {
        let tools = client
            .list_tools()
            .await
            .map_err(|e| AxiomataError::AgentApi {
                backend: BACKEND,
                message: e.to_string(),
            })?;
        for tool in tools {
            if !wanted[name].contains(&tool.name) {
                continue;
            }
            if tool.annotations.get("destructiveHint") == Some(&serde_json::Value::Bool(true)) {
                tracing::warn!(
                    server = %name,
                    tool = %tool.name,
                    "ollama-agent: dropping a tool the MCP server marks destructive"
                );
                continue;
            }
            advertised.push((name.clone(), tool));
        }
    }

    let infos = tool_infos(&advertised)?;
    let index = dispatch_index(&advertised);

    let ollama = match request
        .ollama_base_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        Some(url) => Ollama::try_new(url).map_err(|e| AxiomataError::AgentApi {
            backend: BACKEND,
            message: format!("bad ollama base_url {url:?}: {e}"),
        })?,
        None => Ollama::default(),
    };

    let mut messages = vec![
        ChatMessage::system(PREAMBLE.to_owned()),
        // The SKILL.md body is the task turn — parity with the Claude Code
        // backend's `-p` prompt, no "Begin." filler.
        ChatMessage::user(request.prompt.clone()),
    ];

    for turn in 0..MAX_ITERS {
        let left =
            request
                .timeout
                .checked_sub(started.elapsed())
                .ok_or(AxiomataError::AgentTimeout {
                    backend: BACKEND,
                    timeout: request.timeout,
                })?;
        tracing::info!(turn, tool_count = infos.len(), "ollama-agent: chat turn");
        let req = ChatMessageRequest::new(model.to_owned(), messages.clone()).tools(infos.clone());
        let resp = match timeout(left, ollama.send_chat_messages(req)).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(source)) => {
                shutdown(&mut clients).await;
                return Err(AxiomataError::AgentApi {
                    backend: BACKEND,
                    message: source.to_string(),
                });
            }
            Err(_elapsed) => {
                shutdown(&mut clients).await;
                return Err(AxiomataError::AgentTimeout {
                    backend: BACKEND,
                    timeout: request.timeout,
                });
            }
        };

        match interpret(&resp.message) {
            Step::Final(text) => {
                tracing::info!(turn, "ollama-agent: final answer");
                shutdown(&mut clients).await;
                // `num_turns` carries the loop counter (bare() leaves it None)
                // so the step count surfaces in `get-run` / the DB without
                // needing `RUST_LOG`.
                return Ok(AgentRunResult {
                    stdout: truncate_utf8(text, MAX_RESPONSE_BYTES),
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: started.elapsed().as_millis() as u64,
                    cost_usd: None,
                    input_tokens: None,
                    output_tokens: None,
                    num_turns: Some((turn + 1) as u32),
                });
            }
            Step::Calls(calls) => {
                tracing::info!(
                    turn,
                    tools = ?calls.iter().map(|c| &c.function.name).collect::<Vec<_>>(),
                    "ollama-agent: dispatching tool calls"
                );
                // Keep the assistant turn (role + tool_calls) in context, then
                // append one `tool` message per call result.
                messages.push(resp.message.clone());
                for call in calls {
                    let out = match index.get(&call.function.name) {
                        None => format!("error: no such tool {:?}", call.function.name),
                        Some(server) => {
                            let client = clients
                                .iter_mut()
                                .find(|(s, _)| s == server)
                                .map(|(_, c)| c)
                                .expect("dispatch_index only names spawned servers");
                            match client
                                .call_tool(&call.function.name, call.function.arguments.clone())
                                .await
                            {
                                Ok(result) if result.is_error => {
                                    format!("[tool reported an error] {}", result.text)
                                }
                                Ok(result) => result.text,
                                Err(AxiomataError::McpTimeout { .. }) => {
                                    shutdown(&mut clients).await;
                                    return Err(AxiomataError::AgentApi {
                                        backend: BACKEND,
                                        message: format!(
                                            "MCP tool {} timed out",
                                            call.function.name
                                        ),
                                    });
                                }
                                Err(e) => format!("[tool call failed] {e}"),
                            }
                        }
                    };
                    messages.push(ChatMessage::tool(out));
                }
            }
        }
    }

    shutdown(&mut clients).await;
    Err(AxiomataError::AgentApi {
        backend: BACKEND,
        message: format!("no final answer within {MAX_ITERS} steps"),
    })
}

/// Closes every live MCP session cleanly (stdin-EOF, short grace, then kill).
/// Consumes the clients, so it can only run once — the loop's shutdown-on-
/// every-path guarantee.
async fn shutdown(clients: &mut Vec<(String, McpClient)>) {
    for (_, client) in clients.drain(..) {
        client.shutdown().await;
    }
}

/// Derives, from a skill's `allowed_tools`, which `[mcp_servers]` to spawn
/// and which of their tools to advertise to the model.
///
/// Whitespace-splits the value; each `mcp__<server>__<tool>` token contributes
/// `server → tool`. Non-`mcp__` tokens (`Bash(...)`, `Edit`, `Write`) are
/// silently ignored — this backend has no shell or file tools. A token that
/// starts `mcp__` but has no `__<tool>` after the server is malformed.
///
/// Errors:
///     [`AxiomataError::AgentApi`] for a malformed `mcp__…` token.
fn wanted_servers(
    allowed_tools: Option<&str>,
) -> Result<BTreeMap<String, BTreeSet<String>>, AxiomataError> {
    let mut wanted: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for token in allowed_tools.unwrap_or("").split_whitespace() {
        let Some(rest) = token.strip_prefix("mcp__") else {
            continue;
        };
        let Some((server, tool)) = rest.split_once("__") else {
            return Err(AxiomataError::AgentApi {
                backend: BACKEND,
                message: format!("malformed tool spec {token:?}"),
            });
        };
        wanted
            .entry(server.to_owned())
            .or_default()
            .insert(tool.to_owned());
    }
    Ok(wanted)
}

/// Builds the `ToolInfo` list handed to Ollama from the advertised MCP tools.
///
/// `ollama-rs` 0.3.6's ergonomic constructors are generic over a compile-time
/// `Tool` trait, so each one is deserialised from JSON instead — the only path
/// that works for schemas discovered at run time. A missing description
/// becomes `""`.
///
/// Errors:
///     [`AxiomataError::AgentApi`] if two tools share a bare name across
///     different servers (the model only ever sees the bare name, so an
///     ambiguous name would dispatch calls to the wrong server).
fn tool_infos(advertised: &[(String, McpTool)]) -> Result<Vec<ToolInfo>, AxiomataError> {
    let mut seen = BTreeSet::new();
    let mut infos = Vec::with_capacity(advertised.len());
    for (_, tool) in advertised {
        if !seen.insert(&tool.name) {
            return Err(AxiomataError::AgentApi {
                backend: BACKEND,
                message: format!("tool name '{}' ambiguous across servers", tool.name),
            });
        }
        let info: ToolInfo = serde_json::from_value(json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description.clone().unwrap_or_default(),
                "parameters": tool.input_schema,
            },
        }))
        .map_err(|e| AxiomataError::AgentApi {
            backend: BACKEND,
            message: e.to_string(),
        })?;
        infos.push(info);
    }
    Ok(infos)
}

/// One assistant-message interpretation.
#[derive(Debug)]
enum Step {
    /// No `tool_calls`: the model is done for good.
    Final(String),
    /// One or more `tool_calls` to dispatch; content alongside calls still
    /// counts as `Calls` (the model may narrate while calling).
    Calls(Vec<ToolCall>),
}

/// Splits an assistant message into a final answer or tool calls to dispatch.
fn interpret(message: &ChatMessage) -> Step {
    if message.tool_calls.is_empty() {
        Step::Final(message.content.clone())
    } else {
        Step::Calls(message.tool_calls.clone())
    }
}

/// Maps each advertised tool's bare name to the server that owns it, so a
/// `tools/call` lands on the right client.
fn dispatch_index(advertised: &[(String, McpTool)]) -> HashMap<String, String> {
    advertised
        .iter()
        .map(|(server, tool)| (tool.name.clone(), server.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::Value;

    fn advertised(pairs: &[(&str, &str, &Value)]) -> Vec<(String, McpTool)> {
        pairs
            .iter()
            .map(|(server, name, schema)| {
                (
                    (*server).to_owned(),
                    McpTool {
                        name: (*name).to_owned(),
                        description: None,
                        input_schema: (*schema).clone(),
                        annotations: Value::Null,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn wanted_servers_groups_tools_by_server_and_ignores_foreign_tokens() {
        let wanted = wanted_servers(Some(
            "mcp__apple-mail__list_inbox_emails mcp__apple-reminders__calendar_events \
             mcp__apple-mail__search_emails Bash(git *) Edit",
        ))
        .unwrap();
        assert_eq!(wanted.len(), 2);
        assert_eq!(
            wanted["apple-mail"],
            BTreeSet::from(["list_inbox_emails".to_owned(), "search_emails".to_owned(),])
        );
        assert_eq!(
            wanted["apple-reminders"],
            BTreeSet::from(["calendar_events".to_owned()])
        );
        // A tool name containing `__` after the server keeps the rest intact.
        let nested = wanted_servers(Some("mcp__srv__a__b")).unwrap();
        assert_eq!(nested["srv"], BTreeSet::from(["a__b".to_owned()]));
    }

    #[test]
    fn wanted_servers_is_empty_for_none_blank_or_no_mcp_tools() {
        assert!(wanted_servers(None).unwrap().is_empty());
        assert!(wanted_servers(Some("")).unwrap().is_empty());
        assert!(wanted_servers(Some("Edit Write")).unwrap().is_empty());
    }

    #[test]
    fn wanted_servers_rejects_a_malformed_mcp_token() {
        let err = wanted_servers(Some("mcp__broken")).unwrap_err();
        assert!(
            matches!(err, AxiomataError::AgentApi { .. })
                && err.to_string().contains("malformed tool spec"),
            "{err}"
        );
    }

    #[test]
    fn tool_infos_round_trips_a_real_object_schema() {
        let schema = json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
        });
        let infos = tool_infos(&advertised(&[("mail", "echo", &schema)])).unwrap();
        assert_eq!(infos.len(), 1);
        let serial = serde_json::to_value(&infos[0]).unwrap();
        assert_eq!(serial["type"], json!("function"));
        assert_eq!(serial["function"]["name"], json!("echo"));
        assert_eq!(serial["function"]["description"], json!(""));
        assert_eq!(serial["function"]["parameters"], schema);
    }

    #[test]
    fn tool_infos_rejects_a_duplicate_bare_name() {
        let schema = json!({ "type": "object", "properties": {} });
        let err = tool_infos(&advertised(&[
            ("mail", "echo", &schema),
            ("reminders", "echo", &schema),
        ]))
        .unwrap_err();
        assert!(
            matches!(err, AxiomataError::AgentApi { .. })
                && err.to_string().contains("ambiguous across servers"),
            "{err}"
        );
    }

    #[test]
    fn interpret_distinguishes_final_answers_from_tool_calls() {
        let plain = ChatMessage::assistant("done".to_owned());
        assert!(matches!(interpret(&plain), Step::Final(t) if t == "done"));

        let mut calling = ChatMessage::assistant(String::new());
        calling.tool_calls = vec![ToolCall {
            function: serde_json::from_value(json!({
                "name": "echo", "arguments": {}
            }))
            .unwrap(),
        }];
        assert!(matches!(interpret(&calling), Step::Calls(_)));

        // Content *and* tool_calls together count as Calls — the model is not
        // done, it narrated while dispatching.
        let mut narrated = ChatMessage::assistant("working…".to_owned());
        narrated.tool_calls = calling.tool_calls.clone();
        assert!(matches!(interpret(&narrated), Step::Calls(_)));
    }

    #[test]
    fn dispatch_index_maps_each_bare_name_to_its_server() {
        let schema = json!({ "type": "object", "properties": {} });
        let index = dispatch_index(&advertised(&[
            ("mail", "list_inbox_emails", &schema),
            ("reminders", "calendar_events", &schema),
        ]));
        assert_eq!(index["list_inbox_emails"], "mail");
        assert_eq!(index["calendar_events"], "reminders");
    }

    /// A missing `[mcp_servers]` entry for a server `allowed_tools` names is
    /// an `AgentApi` error raised *before* anything is spawned — no Ollama
    /// daemon, no MCP process.
    #[tokio::test]
    async fn run_fails_when_a_wanted_server_is_not_in_mcp_servers() {
        let request = AgentRequest {
            prompt: "hi".to_owned(),
            cwd: PathBuf::from("/tmp"),
            timeout: Duration::from_secs(10),
            env: Vec::new(),
            system_prompt_file: None,
            model: None,
            allowed_tools: Some("mcp__mock__echo".to_owned()),
            mcp_servers: BTreeMap::new(),
            ollama_base_url: Some("http://127.0.0.1:1".to_owned()),
        };
        let err = run(request, "m").await.unwrap_err();
        assert!(
            matches!(err, AxiomataError::AgentApi { .. })
                && err.to_string().contains("no [mcp_servers] entry"),
            "{err}"
        );
    }

    /// A stand-in for a local Ollama daemon: serves a fixed list of canned
    /// `/api/chat` responses, one per connection, then the listener closes (a
    /// further call gets a connection error → surfaces as `AgentApi`). Same
    /// "in-repo fake, no dev-dep" style as the mock MCP server. `delay` is
    /// applied before every response — set it past the run timeout to exercise
    /// the timeout branch. `seen` captures each request's raw bytes for
    /// asserting what the loop sent.
    struct FakeOllama {
        port: u16,
        seen: Arc<tokio::sync::Mutex<Vec<Vec<u8>>>>,
        _task: tokio::task::JoinHandle<()>,
    }

    impl FakeOllama {
        async fn spawn(turns: Vec<Value>, delay: Duration) -> Self {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let seen = Arc::new(tokio::sync::Mutex::new(Vec::new()));
            let seen_task = seen.clone();
            let task = tokio::spawn(async move {
                for body in turns {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    // Best-effort drain of the request; over loopback one read
                    // is enough for a small JSON body and we don't parse it.
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
            Self {
                port,
                seen,
                _task: task,
            }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }
    }

    fn mock_request(prompt: &str, fake: &FakeOllama, timeout: Duration) -> AgentRequest {
        let mut mcp_servers = BTreeMap::new();
        mcp_servers.insert(
            "mock".to_owned(),
            crate::mcp::mock_server::mock_server_config(),
        );
        AgentRequest {
            prompt: prompt.to_owned(),
            cwd: PathBuf::from("/tmp"),
            timeout,
            env: Vec::new(),
            system_prompt_file: None,
            model: None,
            allowed_tools: Some("mcp__mock__echo".to_owned()),
            mcp_servers,
            ollama_base_url: Some(fake.url()),
        }
    }

    fn canned_turn(content: &str, tool_calls: bool) -> Value {
        json!({
            "model": "m",
            "created_at": "2026-01-01T00:00:00Z",
            "done": true,
            "message": {
                "role": "assistant",
                "content": content,
                "tool_calls": if tool_calls {
                    vec![json!({ "function": { "name": "echo", "arguments": { "text": "ping" } } })]
                } else {
                    vec![]
                }
            }
        })
    }

    /// Happy path: the model calls `echo` on turn 1, the tool result re-enters
    /// context, and turn 2 is the final answer.
    #[tokio::test]
    async fn loop_runs_a_tool_then_returns_the_final_answer() {
        let fake = FakeOllama::spawn(
            vec![canned_turn("", true), canned_turn("final answer", false)],
            Duration::ZERO,
        )
        .await;
        let result = run(
            mock_request("do the thing", &fake, Duration::from_secs(10)),
            "m",
        )
        .await
        .expect("a two-turn run should succeed");
        assert_eq!(result.stdout, "final answer");
        assert_eq!(result.exit_code, 0);
        assert_eq!(
            result.num_turns,
            Some(2),
            "the loop counter records how many turns the run took"
        );

        let seen = fake.seen.lock().await;
        assert_eq!(seen.len(), 2, "one http request per chat turn");
        // Turn 2's request body must carry the tool result from turn 1, i.e.
        // the loop actually fed the `echo` output back to the model.
        let second = String::from_utf8_lossy(&seen[1]);
        assert!(second.contains("\"role\":\"tool\""), "{second}");
        assert!(second.contains("ping"), "{second}");
    }

    /// The run timeout fires while the fake is still sleeping; the loop must
    /// surface `AgentTimeout` and shut the spawned MCP server down without
    /// hanging on the 3 s grace period (the mock exits on stdin-EOF, so a
    /// reaped server returns almost at once).
    #[tokio::test]
    async fn loop_aborts_on_the_run_timeout() {
        let fake = FakeOllama::spawn(
            vec![canned_turn("late reply", false)],
            Duration::from_millis(200),
        )
        .await;
        let started = Instant::now();
        let err = run(mock_request("hi", &fake, Duration::from_millis(100)), "m")
            .await
            .unwrap_err();
        assert!(matches!(err, AxiomataError::AgentTimeout { .. }), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "shutdown must not wait out the grace period on a server that \
             exits when its stdin closes"
        );

        let seen = fake.seen.lock().await;
        assert_eq!(seen.len(), 1, "the loop stops after the first timeout turn");
    }
}
