//! One unattended agent turn: a prompt, its execution, and what it produced.
//!
//! The flow (plan OC1): open the event stream *first* (so no event of the turn
//! is missed), create or reuse the session, send the prompt, and wait for
//! `session.execution.succeeded|failed` of that session. Nobody is there to
//! answer a permission request, so every `permission.asked` is rejected and
//! noted as the turn's failure — a rejection interrupts the execution
//! (`session.execution.interrupted`, and no `idle` entry follows). Afterwards the turn's messages are read back —
//! the reply text, tokens, cost and the `idle` entry's outcome come from
//! there, not from the stream. If the stream breaks mid-turn (it is volatile
//! by contract), the messages are polled for the `idle` entry instead.

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::error::OpencodeError;
use crate::events::EventStream;
use crate::service::Service;
use crate::session::{ModelRef, NewSession, PermissionRule};

/// How long to wait for the `idle` entry once the execution has finished.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(5);
/// Pause between two reads while waiting for the `idle` entry.
const SETTLE_POLL: Duration = Duration::from_millis(200);
/// How often, and how far apart, a new unattended session looks whether its MCP servers are up: 30 s at most,
/// for a service that just started (a connector skill without its tools answers empty).
const MCP_SETTLE_LOOKS: u32 = 60;
const MCP_SETTLE_PAUSE: Duration = Duration::from_millis(500);
/// Pause between two reads when the event stream broke mid-turn.
const FALLBACK_POLL: Duration = Duration::from_secs(1);

/// The session a turn runs in.
#[derive(Debug, Clone)]
pub enum TurnSession {
    /// A fresh session.
    New(NewSession),
    /// A session from an earlier turn; `model` is switched to first when the
    /// session currently uses another one.
    Existing { id: String, model: Option<ModelRef> },
}

/// How the stream said an execution ended.
#[derive(Debug, Clone, PartialEq)]
enum Ended {
    /// `session.execution.succeeded` or `…failed`: an `idle` entry follows.
    Settled,
    /// `session.execution.interrupted` (its `reason`): no `idle` entry follows.
    Interrupted(String),
}

/// What a turn starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnInput {
    /// A prompt, as typed.
    Prompt(String),
    /// One of the service's commands (`GET /api/command`) with the rest of the line.
    Command { name: String, arguments: String },
    /// Summarise the session so far (`POST …/compact`).
    Compact,
}

/// How often, and how far apart, a command's user message is looked for: the service answers the command before it
/// has made the message (measured: ~0.1 s).
const COMMAND_MESSAGE_LOOKS: u32 = 20;
const COMMAND_MESSAGE_PAUSE: Duration = Duration::from_millis(100);

/// What to run.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub session: TurnSession,
    pub text: String,
    /// The turn is interrupted and fails with [`OpencodeError::Timeout`] after this.
    pub timeout: Duration,
}

/// What a finished turn produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnOutcome {
    pub session_id: String,
    /// The text parts of the turn's answers, joined by newlines.
    pub reply: String,
    /// Why the turn failed; `None` when it succeeded.
    pub failure: Option<String>,
    /// Every prompt token the answers processed: fresh, written to and read from the provider's cache — what the
    /// turn used, counted as Glut and Claude Code count it (glut-round CP0: `tokens.input` alone left out ~60 %).
    pub input_tokens: u64,
    /// The part of [`Self::input_tokens`] read from the cache (`tokens.cache.read`).
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    /// The cost the service computed, in USD.
    pub cost: f64,
    /// How many answering messages (model steps) the turn took.
    pub turns: u32,
    /// Whether any answer carried token counts.
    pub has_usage: bool,
    pub duration: Duration,
}

impl TurnOutcome {
    /// Whether the turn succeeded.
    pub fn succeeded(&self) -> bool {
        self.failure.is_none()
    }
}

/// The rules for an unattended session (plan Q7). With `auto_approve`,
/// everything not explicitly denied elsewhere is allowed — the counterpart of
/// `opencode run --auto`; without, nothing is pre-allowed and every request is
/// rejected by [`Service::run_turn`]. Either way the `question` tool is denied:
/// a question nobody answers would stall the turn until its time limit.
pub fn unattended_permissions(auto_approve: bool) -> Vec<PermissionRule> {
    let mut rules = Vec::new();
    if auto_approve {
        rules.push(PermissionRule::new("*", "*", "allow"));
    }
    rules.push(PermissionRule::new("question", "*", "deny"));
    rules
}

impl Service {
    /// Runs one unattended turn.
    ///
    /// Errors:
    ///     [`OpencodeError::Timeout`] when the turn outlives `request.timeout`
    ///     (it is interrupted first); any error talking to the service. A
    ///     turn that ran but failed is an `Ok` whose
    ///     [`TurnOutcome::failure`] says why.
    pub async fn run_turn(&self, request: TurnRequest) -> Result<TurnOutcome, OpencodeError> {
        self.run_input(
            request.session,
            TurnInput::Prompt(request.text),
            request.timeout,
        )
        .await
    }

    /// Runs one unattended turn that starts with `input` — a prompt, a command or a compaction — as
    /// [`Self::run_turn`] does.
    ///
    /// Errors:
    ///     as [`Self::run_turn`]; a command whose user message never appears is an [`OpencodeError::Protocol`].
    pub async fn run_input(
        &self,
        session: TurnSession,
        input: TurnInput,
        timeout: Duration,
    ) -> Result<TurnOutcome, OpencodeError> {
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + timeout;
        let mut events = EventStream::open(self).await?;
        let session_id = match session {
            TurnSession::New(new) => {
                // A location's MCP server that died stays dead (`mcp.rs`): bring it back first,
                // or a connector skill finds no tool and answers empty. Best effort.
                match self.reconnect_failed_mcp(&new.directory).await {
                    Ok(names) if !names.is_empty() => {
                        tracing::info!(?names, directory = %new.directory, "reconnected failed MCP servers")
                    }
                    Ok(_) => {}
                    Err(err) => tracing::warn!(%err, "could not check the MCP servers"),
                }
                match self
                    .await_mcp_settled(&new.directory, MCP_SETTLE_LOOKS, MCP_SETTLE_PAUSE)
                    .await
                {
                    Ok(pending) if !pending.is_empty() => {
                        tracing::warn!(?pending, directory = %new.directory, "MCP servers still connecting")
                    }
                    Ok(_) => {}
                    Err(err) => tracing::warn!(%err, "could not check the MCP servers"),
                }
                self.create_session(&new).await?
            }
            TurnSession::Existing { id, model } => {
                self.apply_environment(&id).await?;
                if let Some(model) = model {
                    let current = self.session(&id).await?;
                    if current.get("model").and_then(ModelRef::from_value).as_ref() != Some(&model)
                    {
                        self.switch_model(&id, &model).await?;
                    }
                }
                id
            }
        };
        let start_id = match self.start(&session_id, input).await {
            Ok(id) => id,
            Err(err) => {
                // A command the service took but whose message never showed runs on unattended: stop it.
                let _ = self.interrupt(&session_id).await;
                return Err(err);
            }
        };

        let mut rejected = Vec::new();
        let waited = tokio::time::timeout_at(
            deadline,
            self.wait_for_execution(&mut events, &session_id, &mut rejected),
        )
        .await;
        drop(events);
        let mut interrupted = None;
        let messages = match waited {
            Err(_) => return Err(self.give_up(&session_id, timeout).await),
            Ok(Ok(Ended::Interrupted(reason))) => {
                interrupted = Some(reason);
                self.messages_after(&session_id, &start_id).await?
            }
            Ok(Ok(Ended::Settled)) => {
                let settle_by = deadline.min(tokio::time::Instant::now() + SETTLE_TIMEOUT);
                self.read_until_idle(&session_id, &start_id, settle_by, SETTLE_POLL)
                    .await?
            }
            Ok(Err(err)) => {
                tracing::warn!(%err, session_id, "opencode: event stream broke mid-turn, polling");
                let messages = self
                    .read_until_idle(&session_id, &start_id, deadline, FALLBACK_POLL)
                    .await?;
                if !has_idle(&messages) {
                    return Err(self.give_up(&session_id, timeout).await);
                }
                messages
            }
        };

        let mut outcome = summarize(&messages);
        if let Some(reason) = interrupted.filter(|_| !has_idle(&messages)) {
            outcome.failure = Some(format!("the turn was interrupted ({reason})"));
        }
        if !rejected.is_empty() {
            let note = format!(
                "permission rejected (unattended run): {}",
                rejected.join("; ")
            );
            outcome.failure = Some(match outcome.failure {
                Some(other) => format!("{note}; {other}"),
                None => note,
            });
        }
        outcome.session_id = session_id;
        outcome.duration = started.elapsed();
        Ok(outcome)
    }

    /// Sends `input` and returns the id of the entry the turn's messages follow.
    async fn start(&self, session_id: &str, input: TurnInput) -> Result<String, OpencodeError> {
        match input {
            TurnInput::Prompt(text) => Ok(self.prompt(session_id, &text).await?.message_id),
            TurnInput::Compact => self.compact(session_id).await,
            TurnInput::Command { name, arguments } => {
                let before = self.newest_user_message(session_id).await?;
                self.command(session_id, &name, &arguments).await?;
                for _ in 0..COMMAND_MESSAGE_LOOKS {
                    let newest = self.newest_user_message(session_id).await?;
                    if let Some(id) = newest.filter(|id| Some(id) != before.as_ref()) {
                        return Ok(id);
                    }
                    tokio::time::sleep(COMMAND_MESSAGE_PAUSE).await;
                }
                Err(OpencodeError::Protocol(format!(
                    "the command /{name} made no message"
                )))
            }
        }
    }

    /// Follows the stream until the session's execution ends, rejecting every
    /// permission request on the way. `Err` when the stream breaks first.
    async fn wait_for_execution(
        &self,
        events: &mut EventStream,
        session_id: &str,
        rejected: &mut Vec<String>,
    ) -> Result<Ended, OpencodeError> {
        while let Some(event) = events.next().await? {
            if event.session_id() != Some(session_id) {
                continue;
            }
            match event.kind.as_str() {
                "session.execution.succeeded" | "session.execution.failed" => {
                    return Ok(Ended::Settled);
                }
                "session.execution.interrupted" => {
                    let reason = event
                        .data
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("no reason given");
                    return Ok(Ended::Interrupted(reason.to_string()));
                }
                "permission.asked" => {
                    rejected.push(describe_permission(&event.data));
                    if let Some(request_id) = event.data.get("id").and_then(Value::as_str) {
                        self.reply_permission(session_id, request_id, "reject")
                            .await?;
                    }
                }
                _ => {}
            }
        }
        Err(OpencodeError::Transport("the event stream ended".into()))
    }

    /// Reads the turn's messages until they hold the `idle` entry or `until`
    /// passes (then the last read is returned as it is).
    async fn read_until_idle(
        &self,
        session_id: &str,
        prompt_id: &str,
        until: tokio::time::Instant,
        poll: Duration,
    ) -> Result<Vec<Value>, OpencodeError> {
        loop {
            let messages = self.messages_after(session_id, prompt_id).await?;
            if has_idle(&messages) || tokio::time::Instant::now() + poll > until {
                return Ok(messages);
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// Interrupts a turn that ran out of time and returns the timeout error.
    async fn give_up(&self, session_id: &str, timeout: Duration) -> OpencodeError {
        if let Err(err) = self.interrupt(session_id).await {
            tracing::warn!(%err, session_id, "opencode: could not interrupt a timed-out turn");
        }
        OpencodeError::Timeout(timeout)
    }
}

/// Whether the messages hold the `idle` entry that ends a turn.
fn has_idle(messages: &[Value]) -> bool {
    messages
        .iter()
        .any(|m| m.get("type").and_then(Value::as_str) == Some("idle"))
}

/// `shell: git push origin` — what a permission request asked for.
fn describe_permission(data: &Value) -> String {
    let action = data.get("action").and_then(Value::as_str).unwrap_or("?");
    let resources: Vec<&str> = data
        .get("resources")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    format!("{action}: {}", resources.join(", "))
}

/// Reads a turn's messages: the reply text, the counts, and whether it
/// succeeded. The last `idle` entry's `outcome` decides; a success whose last
/// answer ended on something other than `stop` still fails (e.g. `length`).
fn summarize(messages: &[Value]) -> TurnOutcome {
    let mut outcome = TurnOutcome::default();
    let mut texts = Vec::new();
    let mut errors = Vec::new();
    let mut finish: Option<String> = None;
    let mut idle: Option<String> = None;
    for message in messages {
        match message.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                outcome.turns += 1;
                if let Some(tokens) = message.get("tokens") {
                    outcome.has_usage = true;
                    let count =
                        |path: &str| tokens.pointer(path).and_then(Value::as_u64).unwrap_or(0);
                    let cached = count("/cache/read");
                    outcome.input_tokens += count("/input") + cached + count("/cache/write");
                    outcome.cached_input_tokens += cached;
                    outcome.output_tokens +=
                        tokens.get("output").and_then(Value::as_u64).unwrap_or(0);
                }
                outcome.cost += message.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
                finish = message
                    .get("finish")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
                    errors.push(error.to_string());
                }
                for part in message
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if part.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(text) = part.get("text").and_then(Value::as_str)
                    {
                        texts.push(text.to_string());
                    }
                }
            }
            Some("idle") => {
                idle = message
                    .get("outcome")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            _ => {}
        }
    }
    outcome.reply = texts.join("\n");
    outcome.failure = match (idle.as_deref(), finish.as_deref()) {
        (Some("succeeded"), None | Some("stop")) => None,
        (Some("succeeded"), Some(other)) => Some(format!("the last answer ended with `{other}`")),
        (Some(other), _) => Some(format!("the turn ended {other}")),
        (None, _) => Some("the turn did not report an outcome".to_string()),
    };
    if !errors.is_empty() {
        let joined = errors.join("; ");
        outcome.failure = Some(match outcome.failure {
            Some(reason) => format!("{reason}: {joined}"),
            None => joined,
        });
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(finish: &str, text: &str, input: u64, output: u64, cost: f64) -> Value {
        json!({
            "type": "assistant", "finish": finish, "cost": cost,
            "tokens": {"input": input, "output": output, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "content": [{"type": "reasoning", "text": "thinking"}, {"type": "text", "text": text}]
        })
    }

    #[test]
    fn a_turn_sums_every_step_and_keeps_only_text_parts() {
        let messages = vec![
            answer("tool-calls", "", 10, 2, 0.001),
            json!({"type": "assistant", "finish": "tool-calls", "content": [{"type": "tool", "name": "shell"}]}),
            answer("stop", "{\"ok\": true}", 30, 5, 0.003),
            json!({"type": "idle", "outcome": "succeeded"}),
        ];
        let outcome = summarize(&messages);
        assert!(outcome.succeeded());
        assert_eq!(outcome.turns, 3);
        assert_eq!((outcome.input_tokens, outcome.output_tokens), (40, 7));
        assert!((outcome.cost - 0.004).abs() < 1e-12);
        assert_eq!(outcome.reply, "\n{\"ok\": true}");
        assert!(outcome.has_usage);
    }

    /// glut-round CP0/CP4: what the cache wrote and read is part of the input — run #1703 had 54k fresh and 81k
    /// read from the cache — and the read part is kept apart for its price.
    #[test]
    fn cached_tokens_count_as_input_and_the_read_ones_stand_apart() {
        let cached = json!({
            "type": "assistant", "finish": "stop", "cost": 0.0,
            "tokens": {"input": 100, "output": 5, "reasoning": 0, "cache": {"read": 900, "write": 20}},
            "content": [{"type": "text", "text": "ok"}]
        });
        let outcome = summarize(&[cached.clone(), cached]);
        assert_eq!(outcome.input_tokens, 2_040);
        assert_eq!(outcome.cached_input_tokens, 1_800);
    }

    #[test]
    fn the_idle_outcome_and_the_last_finish_decide_success() {
        let failed = summarize(&[
            answer("stop", "x", 1, 1, 0.0),
            json!({"type": "idle", "outcome": "failed"}),
        ]);
        assert_eq!(failed.failure.as_deref(), Some("the turn ended failed"));

        let cut = summarize(&[
            answer("length", "x", 1, 1, 0.0),
            json!({"type": "idle", "outcome": "succeeded"}),
        ]);
        assert_eq!(
            cut.failure.as_deref(),
            Some("the last answer ended with `length`")
        );

        let open = summarize(&[answer("stop", "x", 1, 1, 0.0)]);
        assert_eq!(
            open.failure.as_deref(),
            Some("the turn did not report an outcome")
        );

        let empty = summarize(&[json!({"type": "idle", "outcome": "succeeded"})]);
        assert!(empty.succeeded());
        assert!(!empty.has_usage);
    }

    #[test]
    fn provider_errors_are_part_of_the_failure() {
        let mut broken = answer("error", "", 1, 0, 0.0);
        broken["error"] = json!({"type": "provider", "message": "Model unavailable"});
        let outcome = summarize(&[broken, json!({"type": "idle", "outcome": "failed"})]);
        assert_eq!(
            outcome.failure.as_deref(),
            Some("the turn ended failed: Model unavailable")
        );
    }

    #[test]
    fn unattended_rules_allow_all_only_when_asked_and_always_deny_questions() {
        assert_eq!(
            unattended_permissions(true),
            vec![
                PermissionRule::new("*", "*", "allow"),
                PermissionRule::new("question", "*", "deny")
            ]
        );
        assert_eq!(
            unattended_permissions(false),
            vec![PermissionRule::new("question", "*", "deny")]
        );
    }

    #[test]
    fn permission_requests_are_described_for_the_failure() {
        let data = json!({"id": "per_1", "action": "shell", "resources": ["git push origin", "x"]});
        assert_eq!(describe_permission(&data), "shell: git push origin, x");
        assert_eq!(describe_permission(&json!({})), "?: ");
    }
}
