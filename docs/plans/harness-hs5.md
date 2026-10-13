# HS5 — MCP client: build instructions

Self-contained instructions for building step HS5 of the harness (`docs/plans/harness.md`, card #37). Written
2026-10-13 for another model to build without the conversation. Build HS4 first (`docs/plans/harness-hs4.md`); HS5
starts from its commit. Owner decisions (2026-10-13) are marked **[owner]**; points the planner fixed so the step is
buildable are marked **[default]** — build them as written, do not redesign.

## Where and how

Same as HS4 (read that file's "Where and how"): worktree `.claude/worktrees/harness`, branch `worktree-harness`, no
`axiomata-core` dependency, conventions of `src/permissions/settings.rs` / `src/tools/shell.rs`, gate
`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, one commit
`feat(harness): MCP client, step HS5 (#37)`, no push. No new dependencies: `reqwest` (rustls), `tokio`, `serde_json`
are there; parse Server-Sent Events by hand.

Template for the stdio client: the MCP client Axiomata once had, removed in commit `54173bf`. Find it with
`git show 54173bf --stat` and read the removed file with `git show 54173bf^:<path>`. Reuse its JSON-RPC line
handling, adapted to this crate (no `axiomata-core` types).

## Decisions

- **M1 [owner] — servers from the owner and the project, like Claude Code.** `~/.axh/mcp.json` (the owner's) and
  `<project>/.mcp.json` (Claude Code's file, so a repo's servers work as they are). Format of both:
  `{"mcpServers": {"<name>": {"command": "…", "args": […], "env": {…}}}}` for stdio and
  `{"mcpServers": {"<name>": {"type": "http", "url": "https://…", "headers": {…}}}}` for HTTP. Unknown fields →
  error (`deny_unknown_fields`), except `type: "stdio"` which is the default. Server names `[a-zA-Z0-9_-]{1,32}`.
  On a name in both files the owner's wins.
- **M2 [default] — a project's server runs only after the owner approved *that* server.** A project's server starts
  processes or sends data out, so trusting the folder is not enough: each server is approved on its own, keyed by
  project path + server name + SHA-256 of its JSON config; a changed command, argument, env or URL asks again.
  Approvals live in `~/.axh/mcp-approved.json` (written atomically, mode 0600, like `Trust` in `settings.rs`). The CLI
  shows the server's command line / URL (through `permissions::printable`) and asks y/n; headless (`axh`, no
  terminal) skips unapproved project servers and says so on stderr. The owner's own servers need no approval.
- **M3 [owner] — transports stdio and HTTP.**
  - stdio: the server is a child process; JSON-RPC 2.0, one message per line on stdin/stdout; stderr discarded.
    Environment: the cleaned tool environment (`axiomata_files::toolenv`, as `tools/shell.rs` builds it) plus the
    server's `env`. Its own process group, killed with the run (`kill_on_drop`, as in `tools/shell.rs`).
  - HTTP ("Streamable HTTP"): every message is a `POST` to `url` with `Content-Type: application/json` and
    `Accept: application/json, text/event-stream`. The answer is either JSON or an SSE stream (`text/event-stream`):
    read `data:` lines until the response with the request's `id` arrives. Keep the `Mcp-Session-Id` header from the
    `initialize` answer and send it on every later request. No OAuth in HS5 **[default]**: `headers` values may
    contain `${VAR}`, replaced from the harness's own environment (missing variable → that server fails to start
    with a clear message); the URL must be `https://` or loopback `http://` (same rule as `model.rs` uses for keys).
    Same 16 MiB answer cap and no redirects as `model.rs`.
- **M4 [owner] — servers run outside the sandbox, like Claude Code.** They are configured by the owner (or approved,
  M2) and often need the network or apps (`apple-mail`). What protects the owner is the permission on their tools
  (M5).
- **M5 [owner] — permissions like Claude Code.** A tool appears to the model as `mcp__<server>__<tool>`. Rules:
  `mcp__server__tool` (one tool) or `mcp__server` (every tool of that server), in allow / ask / deny like any rule.
  Without a rule: `auto` allows, `default` and `accept_edits` ask, `plan` and `dont_ask` refuse.

## What to build

1. **`src/mcp/config.rs`** — read and merge the two files (M1); `ServerConfig::{Stdio{command,args,env}, Http{url,
   headers}}`; `config_hash` (SHA-256 of the canonical JSON, with the workspace's `sha2` — add `sha2.workspace = true` to the
   crate's `Cargo.toml`).
2. **`src/mcp/approval.rs`** — `Approvals::{load, is_approved(project, name, hash), approve(...)}` (M2).
3. **`src/mcp/client.rs`** — `trait Transport { async send(request) -> response; async notify(..) }` with
   `Stdio` and `Http`; `Client::connect(name, config) -> Result<Client, HarnessError>` does the handshake:
   `initialize` (params `{"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "axh",
   "version": <crate version>}}`), then the notification `notifications/initialized`; then `tools/list`, following
   `nextCursor` (at most 10 pages, at most 200 tools per server — named consts). `Client::call(tool, arguments)` sends
   `tools/call` and turns the result into text: `text` items joined by newlines; any other item becomes a line
   `[<type> omitted]`; `isError: true` makes the tool call fail with that text. Timeouts (named consts): 30 s to
   start and list, 120 s per call; a stop of the run (`CallContext.stop`) ends a waiting call.
4. **`src/mcp/tools.rs`** — one `Tool` per MCP tool: `spec()` gives the name `mcp__<server>__<tool>` (the model
   API allows `[a-zA-Z0-9_-]{1,64}`: replace other characters with `_`, skip a tool whose full name would be longer
   than 64 and say so on stderr), the server's description and its `inputSchema` as parameters. `access()` returns a
   new `Access::Mcp { server, tool }`. The results go through the same output cut as other tools.
5. **`src/permissions/mod.rs`** — `Access::Mcp`, a rule kind for `mcp__server` / `mcp__server__tool` (parse:
   starts with `mcp__`, no parentheses), matching as M5, the mode fallback as M5, `describe` gives
   `server/tool`. Add `"mcp"` handling to `KNOWN_KINDS` or the rule parser so `mcp__x` is not rejected as an unknown
   tool.
6. **Wiring** — `bin/axh.rs` and `crates/axiomata-cli/src/harness_cmd.rs`: after the gate and before the shell,
   load the config, start every server (in parallel, `JoinSet`), add their tools to the `Toolbox`. A server that
   fails to start is reported (stderr / CLI line) and left out — the run goes on without it. New event
   `Event::McpServer { name, status: "connected" | "failed", tools: usize, reason: Option<String> }` (tagged,
   snake_case like the others). Servers end with the run.
7. **Docs** in the same commit — `docs/plans/harness.md`: row HS5 ✅, a section "HS5 — MCP (decided 2026-10-13)"
   with M1–M5 and "HS5 — what exists"; one line in `docs/architecture.md`'s status log; `AGENTS.md` status
   `HS1–HS5 built, HS6–HS8 open`.

## Tests (inline, all must pass)

- config: both files merged, owner wins; unknown field → error; bad server name → error; `type: http` with an
  `http://` non-loopback URL → error; `${VAR}` replaced, missing → that server fails.
- approval: unapproved project server skipped; approved runs; changed `args` → not approved any more; file mode 0600.
- stdio client against a **fake server written as a small shell or Rust test helper** (a script that answers
  `initialize`, `tools/list` with two pages, `tools/call` with text and with `isError`): handshake, pagination,
  result text, error, timeout (a server that never answers), the process is gone after the client drops.
- HTTP client against a fake server on `127.0.0.1:0` (`tokio::net::TcpListener`, as the `model.rs` tests do):
  plain JSON answer; SSE answer with a notification before the response; `Mcp-Session-Id` sent back; 500 → error.
- names: `mcp__mail__list_emails`; a tool name with `.` → `_`; a 70-char name skipped.
- permissions: `mcp__mail` allows every mail tool; `mcp__mail__send` deny beats it; no rule + `default` → asked
  (headless refused); `auto` → allowed; `plan` → refused.
- live test, `#[ignore]`d, against a stdio server command named by the env var `AXH_LIVE_MCP` — never one of the
  owner's real app servers (`apple-mail`, `apple-reminders`) in a test.

## Out of scope (do not build)

OAuth, the SSE-only legacy transport, MCP resources and prompts (prompts as slash commands later), sampling,
roots, elicitation, server notifications beyond ignoring them, tool-list changes during a run, running servers in the
sandbox. Each is a later step.
