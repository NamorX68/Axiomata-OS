//! A minimal MCP (Model Context Protocol) stdio client.
//!
//! Axiomata reaches connector tools (Apple Mail, Apple Reminders, …) through
//! MCP servers spawned as subprocesses. The skill runner (CP2 of
//! `docs/plans/stufe2-lean-ollama-agent.md`) starts the servers a run needs —
//! derived from the skill's `allowed_tools` — speaks an `initialize` /
//! `tools/list` / `tools/call` exchange over stdio, then kills them on
//! completion. This module is that client and the `[mcp_servers]` config
//! import helper; it speaks **no other** process protocol.
//!
//! This is deliberately a hand-rolled client rather than the `rmcp` crate:
//! the surface we need (spawn → initialize → list → call → close) is three
//! JSON-RPC methods over a newline-delimited stdio transport, and `rmcp`
//! (v0.8, two majors behind the current 3.x) would pull ~75 crates — `darling`,
//! `schemars`, `futures`, `tokio-util` — in for that. A ~300-line client keeps
//! the workspace's deliberately lean dependency tree lean (every shared dep in
//! the root `Cargo.toml` carries a comment justifying it) and gives us full
//! control over the lifecycle guarantees the runner needs: kill on completion,
//! per-request timeouts, and skipping unsolicited notification lines.
//!
//! ## Wire protocol
//!
//! MCP over stdio is JSON-RPC 2.0, **one message per line**, no embedded
//! newlines. The client writes a request, then reads lines until a response
//! carrying a matching `id` arrives. Anything else — a `notifications/*`
//! message (logs, progress), a response to a stale id, even a line that is not
//! valid JSON at all — is traced and skipped, so a chatty or slightly
//! misbehaving server can never wedge a pending request; only EOF or the
//! per-request timeout ends the wait.
//!
//! In-flight requests are strictly serialized: the runner awaits each
//! `tools/call` before issuing the next, which is exactly how the agent loop
//! in CP2 is shaped anyway.
//!
//! Not implemented in v1 (the plan's "Non-goals"): streaming, parallel
//! requests, the HTTP `StreamableHTTP` transport, MCP resource/prompt methods,
//! and server pooling (a fresh server is spawned and killed per run).

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::config::McpServerConfig;
use crate::error::AxiomataError;

/// Protocol version proposed on `initialize`. The server answers with the
/// version *it* supports; nothing this client uses is version-specific, so
/// whatever comes back is accepted.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Default wall-clock limit for a single MCP request (`initialize`, `tools/list`,
/// `tools/call`). Generous — a `tools/call` on the Apple connectors can
/// legitimately take tens of seconds — but still bounded, so a hung server (a
/// permission dialog nobody is there to click) can't block the agent loop
/// forever. The runner's own `timeout_secs` still bounds the whole run.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Grace period granted to a server to exit on its own after
/// [`McpClient::shutdown`] closes its stdin (the stdio transport's shutdown
/// signal), before it is SIGKILLed. Short by design: a server that respects
/// EOF exits almost at once, and one that ignores it is not worth waiting on.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// One tool a connected server advertises via `tools/list`.
#[derive(Debug, Clone)]
pub struct McpTool {
    /// The tool's unique name, used as `tools/call`'s `name`.
    pub name: String,
    /// The server's human-readable description, if any.
    pub description: Option<String>,
    /// The JSON schema the server declares for the call's `arguments` object.
    pub input_schema: Value,
    /// The raw `annotations` object (may carry `readOnlyHint` /
    /// `destructiveHint`), `Null` when the server omits it. Useful for
    /// screening out destructive tools on a read-only connector run.
    pub annotations: Value,
}

/// The outcome of a single `tools/call`.
#[derive(Debug, Clone)]
pub struct McpCallResult {
    /// The concatenation of every `content[].text` block, joined with `\n`.
    /// Empty when the server returned only structured or non-text content.
    pub text: String,
    /// The tool's own `isError` flag: the call itself succeeded, but the tool
    /// reports the operation failed. Surfaces as data, not as an error, so
    /// the agent loop can decide what to do with a failed tool result.
    pub is_error: bool,
    /// The raw `result` object, for callers that want `structuredContent`.
    pub raw: Value,
}

/// A live stdio MCP session: the spawned child plus its piped stdin/stdout.
///
/// One request is in flight at a time (see the module docs). The child is
/// spawned with `kill_on_drop`, so a leaked client at least kills its server
/// on drop; [`McpClient::shutdown`] is the full-cleanup path — close stdin
/// (the stdio transport's shutdown signal), a short grace period, then a hard
/// kill and a reap, so no zombie lingers.
pub struct McpClient {
    /// The `[mcp_servers]` key, for error attribution.
    server: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawns `config` and performs the `initialize` handshake, then sends
    /// the mandatory `notifications/initialized` message.
    ///
    /// Args:
    ///     server: The `[mcp_servers]` key, used for error attribution.
    ///     config: The spawn `command` / `args` / `env` for `server`.
    ///     timeout: Wall-clock limit for the `initialize` exchange.
    ///
    /// Errors:
    ///     [`AxiomataError::Mcp`] if the process cannot be spawned or the
    ///     handshake fails; [`AxiomataError::McpTimeout`] if the server does
    ///     not acknowledge the handshake within `timeout`.
    pub async fn connect(
        server: &str,
        config: &McpServerConfig,
        timeout: Duration,
    ) -> Result<Self, AxiomataError> {
        let mut spawn = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Server logging goes to the app's stderr (visible in the CLI,
            // captured by the Tauri shell), separate from the JSON-RPC
            // channel on stdout.
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| AxiomataError::Mcp {
                server: server.to_owned(),
                message: format!("could not spawn `{}`: {source}", config.command),
            })?;

        let stdin = spawn.stdin.take().ok_or_else(|| AxiomataError::Mcp {
            server: server.to_owned(),
            message: "server stdin was not piped".to_owned(),
        })?;
        let stdout = spawn.stdout.take().ok_or_else(|| AxiomataError::Mcp {
            server: server.to_owned(),
            message: "server stdout was not piped".to_owned(),
        })?;

        let mut client = Self {
            server: server.to_owned(),
            child: spawn,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        };

        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "axiomata",
                "version": env!("CARGO_PKG_VERSION"),
            },
        });
        client.request("initialize", params, timeout).await?;
        // The spec mandates this notification right after `initialize`; it
        // has no response, so it is fire-and-forget.
        client
            .send_line(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await?;
        Ok(client)
    }

    /// Calls `tools/list` and parses the advertised tools.
    ///
    /// Errors:
    ///     [`AxiomataError::Mcp`] / [`AxiomataError::McpTimeout`], plus an
    ///     [`AxiomataError::Mcp`] when the result has no `tools` array.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, AxiomataError> {
        let result = self
            .request("tools/list", json!({}), DEFAULT_CALL_TIMEOUT)
            .await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| AxiomataError::Mcp {
                server: self.server.clone(),
                message: "tools/list result has no `tools` array".to_owned(),
            })?;
        Ok(tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name").and_then(Value::as_str)?.to_owned();
                Some(McpTool {
                    name,
                    description: tool
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    input_schema: tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Default::default())),
                    annotations: tool.get("annotations").cloned().unwrap_or(Value::Null),
                })
            })
            .collect())
    }

    /// Calls `tools/call` for `name` with `arguments`.
    ///
    /// A tool that reports its own failure (`isError: true`) is returned as a
    /// normal [`McpCallResult`] with `is_error` set — it is not an error of
    /// this call.
    ///
    /// Errors:
    ///     [`AxiomataError::Mcp`] for a JSON-RPC `error` object (e.g. an
    ///     unknown tool name) or a broken response;
    ///     [`AxiomataError::McpTimeout`] if the server does not answer.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<McpCallResult, AxiomataError> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
                DEFAULT_CALL_TIMEOUT,
            )
            .await?;
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|block| block.get("text").and_then(Value::as_str))
                    .collect::<Vec<&str>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(McpCallResult {
            text,
            is_error,
            raw: result,
        })
    }

    /// Shuts the server down the MCP way and reaps it. Consumes `self`.
    ///
    /// MCP over stdio defines **no** shutdown message (unlike LSP's
    /// `shutdown`/`exit`): closing the transport is the signal. So this drops
    /// our stdin — the server sees EOF on its input — waits [`SHUTDOWN_GRACE`]
    /// for it to exit on its own (the common case, reaped cleanly here), then
    /// SIGKILLs whatever is left. Best-effort; the kill is authoritative.
    pub async fn shutdown(mut self) {
        drop(self.stdin);
        if timeout(SHUTDOWN_GRACE, self.child.wait()).await.is_err() {
            let _ = self.child.kill().await;
        }
        let _ = self.child.wait().await;
    }

    /// Sends one JSON-RPC message without waiting for a response.
    async fn send_line(&mut self, message: &Value) -> Result<(), AxiomataError> {
        let mut line = serde_json::to_string(message).map_err(|err| AxiomataError::Mcp {
            server: self.server.clone(),
            message: format!("could not serialize a JSON-RPC message: {err}"),
        })?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|source| self.io_error("writing to stdin", source))?;
        self.stdin
            .flush()
            .await
            .map_err(|source| self.io_error("flushing stdin", source))
    }

    /// Sends a JSON-RPC request and awaits the matching response.
    ///
    /// Reads stdout line-by-line; anything that is not the response carrying
    /// `id` (an unsolicited notification, a stale id, a non-JSON line) is
    /// traced and skipped. Returns the `result` member, or an error for a
    /// JSON-RPC `error` object, server EOF, or a malformed response.
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout_dur: Duration,
    ) -> Result<Value, AxiomataError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        match timeout(timeout_dur, self.exchange(&request, id)).await {
            Ok(result) => result,
            Err(_elapsed) => Err(AxiomataError::McpTimeout {
                server: self.server.clone(),
                timeout: timeout_dur,
            }),
        }
    }

    /// The read/write half of a request: write it, then read lines until the
    /// matching response arrives.
    async fn exchange(&mut self, request: &Value, id: u64) -> Result<Value, AxiomataError> {
        let mut line = serde_json::to_string(request).map_err(|err| AxiomataError::Mcp {
            server: self.server.clone(),
            message: format!("could not serialize a JSON-RPC request: {err}"),
        })?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|source| self.io_error("writing to stdin", source))?;
        self.stdin
            .flush()
            .await
            .map_err(|source| self.io_error("flushing stdin", source))?;

        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|source| self.io_error("reading from stdout", source))?;
            if read == 0 {
                return Err(AxiomataError::Mcp {
                    server: self.server.clone(),
                    message: "server closed its stdout before answering the request".to_owned(),
                });
            }
            let response: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => {
                    tracing::warn!(
                        server = %self.server,
                        raw = %line.trim(),
                        "ignoring a non-JSON line from an MCP server"
                    );
                    continue;
                }
            };
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                tracing::debug!(
                    server = %self.server,
                    "ignoring an MCP notification or a response to a stale request"
                );
                continue;
            }
            if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
                return Err(AxiomataError::Mcp {
                    server: self.server.clone(),
                    message: format!("JSON-RPC error: {error}"),
                });
            }
            return Ok(response.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Maps a pipe I/O failure onto [`AxiomataError::Mcp`] with a phrase like
    /// `"reading from stdout"`.
    fn io_error(&self, phase: &str, source: std::io::Error) -> AxiomataError {
        AxiomataError::Mcp {
            server: self.server.clone(),
            message: format!("{phase} failed: {source}"),
        }
    }
}

/// Seeds `[mcp_servers]` from Claude Code's `~/.claude.json` MCP block — the
/// one-time import helper of the Stufe-2 plan (CP1).
///
/// Claude Code is where the owner already declared the Apple connector
/// servers, and its `mcpServers` block is a stable, well-known shape:
///
/// ```json
/// "mcpServers": {
///   "apple-mail": {
///     "type": "stdio",
///     "command": "uvx",
///     "args": ["--with", "mcp<2", "mcp-apple-mail"],
///     "env": {}
///   }
/// }
/// ```
///
/// Entries with a non-empty `command` and a `type` of `"stdio"` **or no
/// `type` at all** (stdio is the default, and `claude mcp add` / `.mcp.json`
/// routinely omit it) are imported. Explicit `http` / `sse` transports and
/// `command`-less or malformed entries are skipped. Only the top-level
/// `mcpServers` block is read, not the per-project `projects.<path>.mcpServers`
/// ones. The servers are **copied** into Axiomata-owned config so a subsequent
/// change to `~/.claude.json` never silently alters what skill runs can reach.
///
/// Returns the imported `(name, config)` pairs in the file's (arbitrary)
/// order, ready to merge. `None` when `~/.claude.json` or its `mcpServers`
/// block is missing.
pub fn import_from_claude_code() -> Option<Vec<(String, McpServerConfig)>> {
    let path = home::home_dir()?.join(".claude.json");
    let raw = std::fs::read_to_string(path).ok()?;
    Some(import_from_claude_json(&raw))
}

/// The pure part of [`import_from_claude_code`]: parses a `~/.claude.json`
/// blob without touching the filesystem, so it is unit-testable. Empty when
/// the blob has no importable stdio server.
fn import_from_claude_json(raw: &str) -> Vec<(String, McpServerConfig)> {
    let Ok(root) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(block) = root.get("mcpServers").and_then(Value::as_object) else {
        return Vec::new();
    };
    block
        .iter()
        .filter_map(|(name, config)| {
            let obj = config.as_object()?;
            // `type` is optional; absent means stdio (the default). Skip only
            // an explicit non-stdio transport (`http`, `sse`).
            match obj.get("type").and_then(Value::as_str) {
                None | Some("stdio") => {}
                Some(_) => return None,
            }
            let command = obj.get("command")?.as_str()?.trim();
            if command.is_empty() {
                return None;
            }
            let args = obj
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let env = obj
                .get("env")
                .and_then(Value::as_object)
                .map(|env| {
                    env.iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|v| (key.clone(), v.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some((
                name.clone(),
                McpServerConfig {
                    command: command.to_owned(),
                    args,
                    env,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Switches this test binary into mock-MCP-server mode when re-invoked
    /// via `--exact` by [`mock_server_config`].
    const MOCK_ENV: &str = "AXIOMATA_MOCK_MCP_SERVER";
    const MOCK_ENABLED: &str = "1";

    /// Test-only entry point that acts as a tiny MCP stdio server. Outside
    /// mock mode it is a no-op, so the normal `cargo test` run never enters
    /// the server loop; the client tests spawn this exact test via
    /// `--exact mcp::tests::mock_server_self_entry`.
    #[test]
    fn mock_server_self_entry() {
        if std::env::var(MOCK_ENV).as_deref() == Ok(MOCK_ENABLED) {
            serve_mock();
        }
    }

    /// An `McpServerConfig` that runs the current test binary as the mock MCP
    /// server: self-exec with `--exact`, so only [`mock_server_self_entry`]
    /// runs (the client tests live in this same process image but are not
    /// re-run by the child). The reliable no-extra-files fixture pattern.
    fn mock_server_config() -> McpServerConfig {
        let mut env = BTreeMap::new();
        env.insert(MOCK_ENV.to_string(), MOCK_ENABLED.to_string());
        McpServerConfig {
            command: std::env::current_exe()
                .expect("current_exe")
                .to_string_lossy()
                .into_owned(),
            args: vec![
                "--exact".to_owned(),
                "mcp::tests::mock_server_self_entry".to_owned(),
                "--test-threads".to_owned(),
                "1".to_owned(),
                // --quiet drops libtest's per-test progress lines, whose
                // `test <name> ... ` prefix would otherwise merge with the
                // mock's JSON responses on the same line and wedge the
                // client's parser; --nocapture lets the responses reach the
                // child's real stdout instead of libtest's capture buffer.
                "--quiet".to_owned(),
                "--nocapture".to_owned(),
            ],
            env,
        }
    }

    /// A hand-rolled MCP server for the client tests: greets `initialize`,
    /// lists two tools (`echo`, `boom`), echoes text on `echo`, fails on
    /// `boom`, and exits when its stdin is closed (the stdio transport's
    /// shutdown signal). It also emits one unsolicited `notifications/message`
    /// after the `initialized` notification, exercising the client's skip path.
    fn serve_mock() {
        use std::io::{BufRead, Write};
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut reader = stdin.lock();
        let mut writer = stdout.lock();
        let mut line = String::new();

        loop {
            line.clear();
            let read = match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => line.len(),
            };
            if read == 0 {
                return;
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let id = message.get("id").and_then(Value::as_u64);
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let respond = |writer: &mut std::io::StdoutLock<'_>, id: u64, result: Value| {
                let _ = writeln!(
                    writer,
                    "{}",
                    json!({ "jsonrpc": "2.0", "id": id, "result": result })
                );
                let _ = writer.flush();
            };

            match (id, method) {
                (Some(id), "initialize") => respond(
                    &mut writer,
                    id,
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "mock", "version": "1.0.0" },
                    }),
                ),
                (None, "notifications/initialized") => {
                    // Unsolicited log message: the client must skip it and
                    // keep waiting for the real response to its next request.
                    let _ = writeln!(
                        writer,
                        "{}",
                        json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/message",
                            "params": { "level": "info", "data": "mock ready" },
                        })
                    );
                    let _ = writer.flush();
                }
                (Some(id), "tools/list") => respond(
                    &mut writer,
                    id,
                    json!({
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echo back the text argument",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": { "text": { "type": "string" } },
                                    "required": ["text"],
                                },
                                "annotations": { "readOnlyHint": true },
                            },
                            {
                                "name": "boom",
                                "description": "Always fail",
                                "inputSchema": { "type": "object", "properties": {} },
                            },
                        ]
                    }),
                ),
                (Some(id), "tools/call") => {
                    let name = message
                        .pointer("/params/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let arguments = message
                        .pointer("/params/arguments")
                        .cloned()
                        .unwrap_or(Value::Null);
                    match name {
                        "echo" => {
                            let text = arguments
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            respond(
                                &mut writer,
                                id,
                                json!({
                                    "content": [{ "type": "text", "text": text }],
                                    "isError": false,
                                }),
                            );
                        }
                        "boom" => respond(
                            &mut writer,
                            id,
                            json!({
                                "content": [{ "type": "text", "text": "boom called" }],
                                "isError": true,
                            }),
                        ),
                        other => {
                            let _ = writeln!(
                                writer,
                                "{}",
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32602,
                                        "message": format!("unknown tool {other:?}"),
                                    },
                                })
                            );
                            let _ = writer.flush();
                        }
                    }
                }
                (None, "notifications/cancelled") => return,
                _ => {
                    // Unknown method: reply nothing, keep reading — the
                    // client's own timeout decides.
                }
            }
        }
    }

    #[tokio::test]
    async fn connect_handshakes_then_lists_and_calls_tools() {
        let mut client = McpClient::connect("mock", &mock_server_config(), Duration::from_secs(10))
            .await
            .expect("connect + initialize should succeed");

        let tools = client
            .list_tools()
            .await
            .expect("tools/list should succeed");
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["echo", "boom"], "tools in the mock's listed order");

        let echo = tools.iter().find(|t| t.name == "echo").unwrap();
        assert_eq!(
            echo.description.as_deref(),
            Some("Echo back the text argument")
        );
        assert_eq!(
            echo.annotations.get("readOnlyHint"),
            Some(&Value::Bool(true))
        );

        let result = client
            .call_tool("echo", json!({ "text": "hello world" }))
            .await
            .expect("echo should succeed");
        assert!(!result.is_error);
        assert_eq!(result.text, "hello world");

        client.shutdown().await;
    }

    #[tokio::test]
    async fn a_tool_reporting_is_error_surfaces_as_data_not_an_error() {
        let mut client = McpClient::connect("mock", &mock_server_config(), Duration::from_secs(10))
            .await
            .unwrap();

        let result = client
            .call_tool("boom", json!({}))
            .await
            .expect("the call itself succeeds even when the tool fails");
        assert!(result.is_error);
        assert_eq!(result.text, "boom called");

        client.shutdown().await;
    }

    #[tokio::test]
    async fn a_jsonrpc_error_object_is_returned_as_an_error() {
        let mut client = McpClient::connect("mock", &mock_server_config(), Duration::from_secs(10))
            .await
            .unwrap();

        let err = client
            .call_tool("nonexistent", json!({}))
            .await
            .expect_err("an unknown tool is a JSON-RPC error, not a result");
        assert!(
            matches!(err, AxiomataError::Mcp { .. }),
            "expected Mcp error, got {err:?}"
        );

        client.shutdown().await;
    }

    #[test]
    fn server_eof_without_an_answer_is_an_error() {
        // A server that exits immediately without answering `initialize` must
        // surface an `Mcp` error, not hang or panic.
        let config = McpServerConfig {
            command: "/usr/bin/true".to_owned(),
            args: Vec::new(),
            env: BTreeMap::new(),
        };
        let err = tokio::runtime::Runtime::new().unwrap().block_on(async {
            match McpClient::connect("gone", &config, Duration::from_secs(10)).await {
                Ok(_) => panic!("expected an error for a server that exits immediately"),
                Err(err) => err,
            }
        });
        assert!(
            matches!(err, AxiomataError::Mcp { .. }),
            "expected Mcp error, got {err:?}"
        );
    }

    #[test]
    fn spawn_failure_is_reported_with_a_message() {
        let config = McpServerConfig {
            command: "definitely-a-missing-binary-xyz".to_owned(),
            args: Vec::new(),
            env: BTreeMap::new(),
        };
        let err = tokio::runtime::Runtime::new().unwrap().block_on(async {
            match McpClient::connect("ghost", &config, Duration::from_secs(5)).await {
                Ok(_) => panic!("expected a spawn error"),
                Err(err) => err,
            }
        });
        assert!(
            matches!(err, AxiomataError::Mcp { .. }),
            "expected Mcp error, got {err:?}"
        );
    }

    #[test]
    fn import_from_claude_json_parses_the_known_connector_shape() {
        let raw = r#"{
            "someOtherKey": 42,
            "mcpServers": {
                "apple-mail": {
                    "type": "stdio",
                    "command": "uvx",
                    "args": ["--with", "mcp<2", "mcp-apple-mail"],
                    "env": {}
                },
                "apple-reminders": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "mcp-server-apple-events"],
                    "env": { "LANG": "en_US.UTF-8" }
                },
                "typeless": { "command": "some-server", "args": ["--stdio"] },
                "http-server": { "type": "http", "url": "https://example.com/mcp" },
                "shellies": { "type": "stdio", "command": "   ", "args": [] },
                "no-command": { "type": "stdio", "args": [] }
            }
        }"#;

        let imported = import_from_claude_json(raw);
        let by_name: BTreeMap<_, _> = imported.into_iter().collect();
        assert_eq!(
            by_name.len(),
            3,
            "http / blank / missing-command entries are dropped; a typeless one is kept"
        );

        let mail = by_name.get("apple-mail").expect("apple-mail imported");
        assert_eq!(mail.command, "uvx");
        assert_eq!(mail.args, ["--with", "mcp<2", "mcp-apple-mail"]);
        assert!(mail.env.is_empty());

        let typeless = by_name
            .get("typeless")
            .expect("an entry with no `type` is imported as stdio");
        assert_eq!(typeless.command, "some-server");
        assert_eq!(typeless.args, ["--stdio"]);

        let reminders = by_name
            .get("apple-reminders")
            .expect("apple-reminders imported");
        assert_eq!(reminders.command, "npx");
        assert_eq!(reminders.args, ["-y", "mcp-server-apple-events"]);
        assert_eq!(
            reminders.env.get("LANG").map(String::as_str),
            Some("en_US.UTF-8")
        );
    }

    #[test]
    fn import_from_claude_json_is_empty_on_missing_or_unparseable_input() {
        assert!(import_from_claude_json("not json at all").is_empty());
        assert!(import_from_claude_json(r#"{ "mcpServers": {} }"#).is_empty());
        assert!(import_from_claude_json(r#"{"no": "servers"}"#).is_empty());
    }
}
