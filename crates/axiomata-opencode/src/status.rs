//! What each session is doing, followed from the event stream (plan OC3).
//!
//! [`Tracker`] is a pure reducer: fed [`Event`]s, it keeps one state word per
//! session — idle, working, or waiting for a person — and the latest answer of
//! Opencode's plan agent. The embedder runs the stream and asks the tracker;
//! after every (re)connect it seeds the tracker from the service
//! ([`Service::session_snapshot`]), because the stream is volatile and replays
//! nothing.
//!
//! The events were measured against Opencode 2.0.18 (`docs/plans/opencode2.md`):
//! `session.execution.started|succeeded|failed|interrupted`, `permission.asked`
//! / `permission.replied`, `form.created` (a question for the user; its session
//! sits at `data.form.sessionID`) / `form.cancelled` and the other `form.*`
//! endings, and for the plan agent `session.step.started` (`agent: "plan"`) plus
//! `session.text.ended` (the text of that step's answer). Sub-agents run in
//! sessions of their own and never touch the parent's entry.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::OpencodeError;
use crate::events::Event;
use crate::service::Service;

/// Messages looked at for the latest plan-agent answer when seeding.
const PLAN_LOOKBACK: u32 = 50;

/// What a session is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Finished its turn; ready for the next prompt.
    Idle,
    /// In the middle of a turn.
    Working,
    /// Blocked on a person: a permission request or a question.
    Waiting,
}

/// The plan agent's latest answer in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanAnswer {
    pub markdown: String,
    /// When it was finished, in milliseconds since the Unix epoch.
    pub at_ms: i64,
}

/// One session's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    pub state: SessionState,
    /// When `state` began, in milliseconds since the Unix epoch.
    pub since_ms: i64,
    pub plan: Option<PlanAnswer>,
}

/// Per-session state, fed from the event stream.
#[derive(Debug, Default)]
pub struct Tracker {
    sessions: HashMap<String, SessionStatus>,
    /// The newest plan-agent step under way per session: (message id, text
    /// parts). A newer step replaces an older one — the plan is the answer
    /// that ends the turn, the same one seeding reads back.
    plan_steps: HashMap<String, (String, Vec<String>)>,
}

impl Tracker {
    /// The session's entry, if the tracker has seen or been told about it.
    pub fn get(&self, session_id: &str) -> Option<&SessionStatus> {
        self.sessions.get(session_id)
    }

    /// Sets a session's state (seeding after a reconnect); `since_ms` moves
    /// only when the state really changes.
    pub fn set(&mut self, session_id: &str, state: SessionState, now_ms: i64) {
        let entry = self
            .sessions
            .entry(session_id.to_string())
            .or_insert(SessionStatus {
                state,
                since_ms: now_ms,
                plan: None,
            });
        if entry.state != state {
            entry.state = state;
            entry.since_ms = now_ms;
        }
    }

    /// Records a session's latest plan-agent answer (seeding after a reconnect).
    pub fn set_plan(&mut self, session_id: &str, plan: PlanAnswer, now_ms: i64) {
        if !self.sessions.contains_key(session_id) {
            self.set(session_id, SessionState::Idle, now_ms);
        }
        if let Some(entry) = self.sessions.get_mut(session_id) {
            entry.plan = Some(plan);
        }
    }

    /// Applies one event.
    pub fn apply(&mut self, event: &Event, now_ms: i64) {
        let data = &event.data;
        let session = event
            .session_id()
            .or_else(|| data.pointer("/form/sessionID").and_then(Value::as_str))
            .map(str::to_owned);
        let Some(session) = session else {
            return;
        };
        let kind = event.kind.as_str();
        match kind {
            "session.execution.started" | "permission.replied" => {
                self.set(&session, SessionState::Working, now_ms);
            }
            "session.execution.succeeded"
            | "session.execution.failed"
            | "session.execution.interrupted" => {
                self.set(&session, SessionState::Idle, now_ms);
                self.finish_plans(&session, now_ms);
            }
            "permission.asked" | "form.created" => {
                self.set(&session, SessionState::Waiting, now_ms)
            }
            // Answered, cancelled or otherwise closed: the turn goes on.
            _ if kind.starts_with("form.") => self.set(&session, SessionState::Working, now_ms),
            "session.step.started" if data.get("agent").and_then(Value::as_str) == Some("plan") => {
                if let Some(message) = data.get("assistantMessageID").and_then(Value::as_str) {
                    self.plan_steps
                        .insert(session, (message.to_string(), Vec::new()));
                }
            }
            "session.text.ended" => {
                let message = data.get("assistantMessageID").and_then(Value::as_str);
                if let Some((step, parts)) = self.plan_steps.get_mut(&session)
                    && message == Some(step.as_str())
                    && let Some(text) = data.get("text").and_then(Value::as_str)
                {
                    parts.push(text.to_string());
                }
            }
            _ => {}
        }
    }

    /// Turns the session's plan-agent answers under way into its plan.
    fn finish_plans(&mut self, session: &str, now_ms: i64) {
        let Some((_, parts)) = self.plan_steps.remove(session) else {
            return;
        };
        let markdown = parts.join("\n\n").trim().to_string();
        if !markdown.is_empty() {
            self.set_plan(
                session,
                PlanAnswer {
                    markdown,
                    at_ms: now_ms,
                },
                now_ms,
            );
        }
    }
}

/// A session's state as the service reports it right now (for seeding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub state: SessionState,
    pub plan: Option<PlanAnswer>,
}

impl Service {
    /// The sessions executing right now (`GET /api/session/active`).
    pub async fn active_sessions(&self) -> Result<Vec<String>, OpencodeError> {
        let answer = self.get("/api/session/active").await?;
        Ok(answer
            .get("data")
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default())
    }

    /// What `session_id` is doing and its latest plan-agent answer, read from
    /// the service; `active` is [`Service::active_sessions`]'s answer.
    pub async fn session_snapshot(
        &self,
        session_id: &str,
        active: &[String],
    ) -> Result<SessionSnapshot, OpencodeError> {
        let waiting = !self.list(session_id, "/permission").await?.is_empty()
            || !self.list(session_id, "/form").await?.is_empty();
        let state = if waiting {
            SessionState::Waiting
        } else if active.iter().any(|s| s == session_id) {
            SessionState::Working
        } else {
            SessionState::Idle
        };
        let recent = self.recent_messages(session_id, PLAN_LOOKBACK).await?;
        Ok(SessionSnapshot {
            state,
            plan: latest_plan(&recent),
        })
    }

    /// `GET /api/session/<id><rest>`'s `data` array.
    async fn list(&self, session_id: &str, rest: &str) -> Result<Vec<Value>, OpencodeError> {
        let path = crate::session::session_path(session_id, rest)?;
        let answer = self.get(&path).await?;
        Ok(answer
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }
}

/// The newest finished plan-agent answer in `messages` (newest first).
fn latest_plan(messages: &[Value]) -> Option<PlanAnswer> {
    let answer = messages.iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("assistant")
            && m.get("agent").and_then(Value::as_str) == Some("plan")
            && m.get("finish").and_then(Value::as_str) == Some("stop")
    })?;
    let markdown = answer
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string();
    let at_ms = answer
        .pointer("/time/completed")
        .or_else(|| answer.pointer("/time/created"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    (!markdown.is_empty()).then_some(PlanAnswer { markdown, at_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, data: Value) -> Event {
        Event {
            kind: kind.into(),
            data,
        }
    }

    #[test]
    fn a_turn_goes_working_waiting_working_idle() {
        let mut t = Tracker::default();
        let s = json!({"sessionID": "ses_a"});
        t.apply(&event("session.execution.started", s.clone()), 10);
        assert_eq!(t.get("ses_a").unwrap().state, SessionState::Working);
        t.apply(
            &event(
                "permission.asked",
                json!({"id": "per_1", "sessionID": "ses_a"}),
            ),
            20,
        );
        assert_eq!(t.get("ses_a").unwrap().state, SessionState::Waiting);
        assert_eq!(t.get("ses_a").unwrap().since_ms, 20);
        t.apply(
            &event(
                "permission.replied",
                json!({"sessionID": "ses_a", "reply": "once"}),
            ),
            30,
        );
        assert_eq!(t.get("ses_a").unwrap().state, SessionState::Working);
        t.apply(&event("session.execution.succeeded", s), 40);
        assert_eq!(
            t.get("ses_a").unwrap(),
            &SessionStatus {
                state: SessionState::Idle,
                since_ms: 40,
                plan: None
            }
        );
    }

    #[test]
    fn a_question_is_waiting_and_its_session_is_found_inside_the_form() {
        let mut t = Tracker::default();
        t.apply(
            &event("session.execution.started", json!({"sessionID": "ses_q"})),
            1,
        );
        t.apply(
            &event(
                "form.created",
                json!({"form": {"id": "frm_1", "sessionID": "ses_q"}}),
            ),
            2,
        );
        assert_eq!(t.get("ses_q").unwrap().state, SessionState::Waiting);
        t.apply(
            &event(
                "form.cancelled",
                json!({"id": "frm_1", "sessionID": "ses_q"}),
            ),
            3,
        );
        assert_eq!(t.get("ses_q").unwrap().state, SessionState::Working);
        t.apply(
            &event(
                "session.execution.interrupted",
                json!({"sessionID": "ses_q", "reason": "user"}),
            ),
            4,
        );
        assert_eq!(t.get("ses_q").unwrap().state, SessionState::Idle);
    }

    #[test]
    fn repeating_a_state_keeps_its_start_and_other_sessions_are_untouched() {
        let mut t = Tracker::default();
        t.apply(
            &event("session.execution.started", json!({"sessionID": "ses_a"})),
            10,
        );
        t.apply(
            &event("session.execution.started", json!({"sessionID": "ses_a"})),
            50,
        );
        assert_eq!(t.get("ses_a").unwrap().since_ms, 10);
        t.apply(
            &event(
                "session.execution.started",
                json!({"sessionID": "ses_child"}),
            ),
            60,
        );
        assert_eq!(t.get("ses_a").unwrap().since_ms, 10);
        t.apply(&event("server.connected", json!({})), 70);
        assert!(t.get("ses_b").is_none());
    }

    #[test]
    fn a_plan_agent_answer_becomes_the_plan_once_the_turn_ends() {
        let mut t = Tracker::default();
        let step = |agent: &str, msg: &str| {
            event(
                "session.step.started",
                json!({"sessionID": "ses_p", "agent": agent, "assistantMessageID": msg}),
            )
        };
        let text = |msg: &str, text: &str| {
            event(
                "session.text.ended",
                json!({"sessionID": "ses_p", "assistantMessageID": msg, "text": text}),
            )
        };
        t.apply(
            &event("session.execution.started", json!({"sessionID": "ses_p"})),
            1,
        );
        t.apply(&step("plan", "msg_0"), 2);
        t.apply(&text("msg_0", "a first try"), 2);
        t.apply(&step("plan", "msg_1"), 2);
        t.apply(&text("msg_1", "1. Read the code"), 3);
        t.apply(&step("build", "msg_2"), 4);
        t.apply(&text("msg_2", "not a plan"), 5);
        assert!(
            t.get("ses_p").unwrap().plan.is_none(),
            "only when the turn ends"
        );
        t.apply(
            &event("session.execution.succeeded", json!({"sessionID": "ses_p"})),
            6,
        );
        assert_eq!(
            t.get("ses_p").unwrap().plan,
            Some(PlanAnswer {
                markdown: "1. Read the code".into(),
                at_ms: 6
            })
        );
    }

    #[test]
    fn seeding_reads_the_newest_finished_plan_answer() {
        let answer = |agent: &str, finish: &str, text: &str| {
            json!({"type": "assistant", "agent": agent, "finish": finish,
                   "content": [{"type": "text", "text": text}]})
        };
        let mut newest = answer("plan", "stop", "## Plan\n1. A");
        newest["time"] = json!({"created": 5, "completed": 9});
        newest["content"] = json!([{"type": "reasoning", "text": "hmm"}, {"type": "text", "text": "## Plan\n1. A"}]);
        let messages = vec![
            answer("build", "stop", "done"),
            answer("plan", "tool-calls", "half"),
            newest,
            answer("plan", "stop", "older"),
        ];
        assert_eq!(
            latest_plan(&messages),
            Some(PlanAnswer {
                markdown: "## Plan\n1. A".into(),
                at_ms: 9
            })
        );
        assert_eq!(latest_plan(&messages[..2]), None);
    }
}
