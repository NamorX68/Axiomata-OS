//! The agents' permission requests, answered by the owner in the app (`docs/plans/permission-asks.md`, card #88).
//!
//! A Claude Code session's `PermissionRequest` hook runs `axiomata-cli permission-hook`, which stores the request here
//! ([`ask`]) and waits for its decision ([`wait_for_decision`]); the app answers through [`decide`], as the owner and
//! only as the owner. The hook prints [`hook_output`] for Claude Code, or nothing — then Claude Code shows its own
//! prompt in the pane, as it did before this existed.

use std::time::{Duration, Instant};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::AxiomataError;

/// How long the hook waits for the owner before it hands the question back to the pane. Below the hook's own timeout
/// in the Claude Code settings (`axiomata_ide::lifecycle`), so the hook ends by itself rather than being killed.
pub const HOOK_WAIT: Duration = Duration::from_secs(25 * 60);

/// How often the hook looks for the owner's decision: often enough to feel immediate, and one small query each time.
pub const HOOK_POLL: Duration = Duration::from_millis(500);

/// How long an answered request is kept: long enough to look back on a day's work, short enough that the table stays
/// small however many requests a session makes.
const KEEP_ANSWERED_DAYS: i64 = 30;

/// The most characters of a request's detail that are kept. The detail is written by the agent; it is shown on one
/// line in the app, so a page of it would only push the buttons away.
const DETAIL_CHARS: usize = 300;

/// What the owner decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    /// Answer it in the pane: the hook ends without a decision and Claude Code shows its own prompt.
    Terminal,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
            Decision::Terminal => "terminal",
        }
    }

    /// Reads a decision as the app sends it.
    ///
    /// # Errors
    ///
    /// Anything but `allow`, `deny` or `terminal`.
    pub fn parse(text: &str) -> Result<Self, AxiomataError> {
        match text {
            "allow" => Ok(Decision::Allow),
            "deny" => Ok(Decision::Deny),
            "terminal" => Ok(Decision::Terminal),
            other => Err(refused(format!("unknown decision {other:?}"))),
        }
    }
}

/// An open request, as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAsk {
    pub id: i64,
    pub tool: String,
    pub detail: String,
}

fn refused(reason: String) -> AxiomataError {
    AxiomataError::PermissionAsk { reason }
}

/// The tool and its main argument from a `PermissionRequest` payload (`tool_name`, `tool_input`): the command of a
/// shell call, the file of an edit, the address of a fetch — else the input as compact JSON. Cut to one line.
pub fn describe(payload: &Value) -> (String, String) {
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned();
    let input = payload.get("tool_input").unwrap_or(&Value::Null);
    let main = [
        "command",
        "file_path",
        "notebook_path",
        "url",
        "path",
        "pattern",
    ]
    .iter()
    .find_map(|key| input.get(key).and_then(Value::as_str))
    .map(str::to_owned)
    .unwrap_or_else(|| match input {
        Value::Null => String::new(),
        other => other.to_string(),
    });
    (cut_line(&tool, DETAIL_CHARS), cut_line(&main, DETAIL_CHARS))
}

/// [`cut_line`] at the length a request's detail keeps — for a detail that did not come through [`describe`].
pub fn one_line(text: &str) -> String {
    cut_line(text, DETAIL_CHARS)
}

/// `text` on one line — every control character, line breaks included, becomes a space — and at most `max` characters.
fn cut_line(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        return flat.to_owned();
    }
    let mut cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Stores a request of `agent_id` and returns its id. An earlier request of the same agent that is still open is
/// closed first: Claude Code asks one thing at a time, so an open one is a hook that died without ending.
///
/// # Errors
///
/// A database error.
pub fn ask(db: &Connection, agent_id: i64, tool: &str, detail: &str) -> Result<i64, AxiomataError> {
    let now = Utc::now();
    close_open(db, agent_id, "system:replaced")?;
    db.execute(
        "DELETE FROM permission_asks WHERE decision IS NOT NULL AND asked_at < ?1",
        params![(now - chrono::Duration::days(KEEP_ANSWERED_DAYS)).to_rfc3339()],
    )?;
    let now = now.to_rfc3339();
    db.execute(
        "INSERT INTO permission_asks (agent_id, tool, detail, asked_at) VALUES (?1, ?2, ?3, ?4)",
        params![agent_id, tool, detail, now],
    )?;
    Ok(db.last_insert_rowid())
}

/// Closes the agent's open requests as `terminal`, by `by` (`system:…`): a new request replaces them, and a restarted
/// session's hook is gone — its row must not be offered in the app.
///
/// # Errors
///
/// A database error.
pub fn close_open(db: &Connection, agent_id: i64, by: &str) -> Result<(), AxiomataError> {
    db.execute(
        "UPDATE permission_asks SET decision = 'terminal', decided_by = ?2, decided_at = ?3 \
         WHERE agent_id = ?1 AND decision IS NULL",
        params![agent_id, by, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// The agent's open request, if it has one.
///
/// # Errors
///
/// A database error.
pub fn open_ask(db: &Connection, agent_id: i64) -> Result<Option<OpenAsk>, AxiomataError> {
    Ok(db
        .query_row(
            "SELECT id, tool, detail FROM permission_asks WHERE agent_id = ?1 AND decision IS NULL \
             ORDER BY id DESC LIMIT 1",
            params![agent_id],
            |row| {
                Ok(OpenAsk {
                    id: row.get(0)?,
                    tool: row.get(1)?,
                    detail: row.get(2)?,
                })
            },
        )
        .optional()?)
}

/// The decision on request `id`, `None` while it is open (or unknown).
///
/// # Errors
///
/// A database error, or a stored decision this version does not know.
pub fn decision_of(db: &Connection, id: i64) -> Result<Option<Decision>, AxiomataError> {
    let stored: Option<Option<String>> = db
        .query_row(
            "SELECT decision FROM permission_asks WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    stored
        .flatten()
        .map(|text| Decision::parse(&text))
        .transpose()
}

/// Answers request `id` of `agent_id`. Only the owner answers (`human:` actor), only an open request, and only the
/// agent's own — the app names both, so an answer can never land on another agent's question.
///
/// # Errors
///
/// [`AxiomataError::PermissionAsk`] when one of those does not hold; a database error.
pub fn decide(
    db: &Connection,
    agent_id: i64,
    id: i64,
    decision: Decision,
    actor: &str,
) -> Result<(), AxiomataError> {
    if !actor.starts_with("human:") {
        return Err(refused(format!(
            "{actor} may not answer an agent's permission request"
        )));
    }
    let changed = db.execute(
        "UPDATE permission_asks SET decision = ?3, decided_by = ?4, decided_at = ?5 \
         WHERE id = ?1 AND agent_id = ?2 AND decision IS NULL",
        params![
            id,
            agent_id,
            decision.as_str(),
            actor,
            Utc::now().to_rfc3339()
        ],
    )?;
    if changed == 0 {
        return Err(refused(format!(
            "request {id} of agent {agent_id} is not open (answered already, or not that agent's)"
        )));
    }
    Ok(())
}

/// Settles request `id` with the answer the owner typed in the session's pane (a Glut session, which asks in the app
/// and in its pane at once, `docs/plans/glut-cards.md` C5). Returns `false` when the request was settled already —
/// the app's answer came first, and that one stands.
///
/// # Errors
///
/// A database error.
pub fn answer_in_pane(
    db: &Connection,
    agent_id: i64,
    id: i64,
    decision: Decision,
) -> Result<bool, AxiomataError> {
    let changed = db.execute(
        "UPDATE permission_asks SET decision = ?2, decided_by = 'human:pane', decided_at = ?3 \
         WHERE id = ?1 AND agent_id = ?4 AND decision IS NULL",
        params![id, decision.as_str(), Utc::now().to_rfc3339(), agent_id],
    )?;
    Ok(changed > 0)
}

/// Withdraws request `id` of `agent_id` when nobody answered it and nobody will: the pane's input ended, or the turn
/// was called off while it asked. The app stops offering it.
///
/// # Errors
///
/// A database error.
pub fn withdraw(db: &Connection, agent_id: i64, id: i64) -> Result<(), AxiomataError> {
    db.execute(
        "UPDATE permission_asks SET decision = 'terminal', decided_by = 'system:withdrawn', decided_at = ?3 \
         WHERE id = ?1 AND agent_id = ?2 AND decision IS NULL",
        params![id, agent_id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// Waits for the decision on request `id`, reading it through `read` every `poll` until `wait` has passed. On the way
/// out without a decision the request is closed as `terminal`, so the app stops offering it while the pane's own
/// prompt takes over.
///
/// # Errors
///
/// A database error from `read`.
pub fn wait_for_decision(
    db: &Connection,
    id: i64,
    wait: Duration,
    poll: Duration,
) -> Result<Option<Decision>, AxiomataError> {
    let until = Instant::now() + wait;
    loop {
        if let Some(decision) = decision_of(db, id)? {
            return Ok(Some(decision));
        }
        if Instant::now() >= until {
            db.execute(
                "UPDATE permission_asks SET decision = 'terminal', decided_by = 'system:timeout', decided_at = ?2 \
                 WHERE id = ?1 AND decision IS NULL",
                params![id, Utc::now().to_rfc3339()],
            )?;
            return Ok(None);
        }
        std::thread::sleep(poll);
    }
}

/// What the hook prints for Claude Code: allow or deny, or nothing at all for `terminal` and for no decision — then
/// Claude Code asks in the pane.
pub fn hook_output(decision: Option<Decision>) -> Option<String> {
    let decision = match decision? {
        Decision::Allow => json!({ "behavior": "allow" }),
        Decision::Deny => json!({
            "behavior": "deny",
            "message": "The owner declined this in the Axiomata app.",
        }),
        Decision::Terminal => return None,
    };
    Some(
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": decision,
            }
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-permission-ask-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        crate::db::open_and_migrate_at(&dir.join("t.db")).unwrap()
    }

    #[test]
    fn a_shell_call_is_described_by_its_command_on_one_line() {
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "npm ci\n&& rm -rf x", "description": "install" },
        });
        assert_eq!(
            describe(&payload),
            ("Bash".into(), "npm ci && rm -rf x".into())
        );
    }

    #[test]
    fn an_edit_names_its_file_and_an_unknown_input_shows_as_json() {
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/src/a.rs", "old_string": "x" } });
        assert_eq!(describe(&edit).1, "/w/src/a.rs");
        let other = json!({ "tool_name": "mcp__x__y", "tool_input": { "n": 1 } });
        assert_eq!(describe(&other).1, r#"{"n":1}"#);
        assert_eq!(describe(&json!({})), ("?".into(), String::new()));
    }

    #[test]
    fn a_long_detail_is_cut_with_an_ellipsis() {
        let long = "a".repeat(DETAIL_CHARS + 50);
        let (_, detail) =
            describe(&json!({ "tool_name": "Bash", "tool_input": { "command": long } }));
        assert_eq!(detail.chars().count(), DETAIL_CHARS);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn the_owner_answers_an_open_request_of_that_agent_once() {
        let db = db();
        let id = ask(&db, 7, "Bash", "npm ci").unwrap();
        assert_eq!(
            open_ask(&db, 7).unwrap(),
            Some(OpenAsk {
                id,
                tool: "Bash".into(),
                detail: "npm ci".into()
            })
        );
        assert_eq!(decision_of(&db, id).unwrap(), None);

        assert!(matches!(
            decide(&db, 8, id, Decision::Allow, "human:owner"),
            Err(AxiomataError::PermissionAsk { .. })
        ));
        assert!(matches!(
            decide(&db, 7, id, Decision::Allow, "agent:worker-7"),
            Err(AxiomataError::PermissionAsk { .. })
        ));
        decide(&db, 7, id, Decision::Allow, "human:owner").unwrap();
        assert_eq!(decision_of(&db, id).unwrap(), Some(Decision::Allow));
        assert_eq!(open_ask(&db, 7).unwrap(), None);
        assert!(matches!(
            decide(&db, 7, id, Decision::Deny, "human:owner"),
            Err(AxiomataError::PermissionAsk { .. })
        ));
    }

    #[test]
    fn a_new_request_closes_the_agents_earlier_open_one() {
        let db = db();
        let first = ask(&db, 3, "Bash", "a").unwrap();
        let second = ask(&db, 3, "Bash", "b").unwrap();
        assert_eq!(decision_of(&db, first).unwrap(), Some(Decision::Terminal));
        assert_eq!(open_ask(&db, 3).unwrap().map(|a| a.id), Some(second));
    }

    #[test]
    fn the_wait_returns_the_decision_or_hands_the_request_back_to_the_pane() {
        let db = db();
        let answered = ask(&db, 1, "Bash", "ls").unwrap();
        decide(&db, 1, answered, Decision::Deny, "human:owner").unwrap();
        assert_eq!(
            wait_for_decision(&db, answered, Duration::ZERO, Duration::ZERO).unwrap(),
            Some(Decision::Deny)
        );

        let unanswered = ask(&db, 2, "Bash", "ls").unwrap();
        assert_eq!(
            wait_for_decision(&db, unanswered, Duration::ZERO, Duration::ZERO).unwrap(),
            None
        );
        assert_eq!(
            decision_of(&db, unanswered).unwrap(),
            Some(Decision::Terminal)
        );
        assert_eq!(open_ask(&db, 2).unwrap(), None);
    }

    #[test]
    fn the_hook_prints_allow_or_deny_and_nothing_for_the_terminal() {
        let allow: Value =
            serde_json::from_str(&hook_output(Some(Decision::Allow)).unwrap()).unwrap();
        assert_eq!(
            allow["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let deny: Value =
            serde_json::from_str(&hook_output(Some(Decision::Deny)).unwrap()).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(deny["hookSpecificOutput"]["decision"]["message"].is_string());
        assert_eq!(hook_output(Some(Decision::Terminal)), None);
        assert_eq!(hook_output(None), None);
    }

    #[test]
    fn a_restart_closes_the_open_request_and_old_answered_ones_are_dropped() {
        let db = db();
        let open = ask(&db, 4, "Bash", "ls").unwrap();
        close_open(&db, 4, "system:restart").unwrap();
        assert_eq!(decision_of(&db, open).unwrap(), Some(Decision::Terminal));
        assert_eq!(open_ask(&db, 4).unwrap(), None);

        let old = (Utc::now() - chrono::Duration::days(KEEP_ANSWERED_DAYS + 1)).to_rfc3339();
        db.execute("UPDATE permission_asks SET asked_at = ?1", params![old])
            .unwrap();
        ask(&db, 5, "Bash", "pwd").unwrap();
        let rows: i64 = db
            .query_row("SELECT COUNT(*) FROM permission_asks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "only the new request is left");
    }

    #[test]
    fn decisions_parse_from_the_apps_words_only() {
        assert_eq!(Decision::parse("terminal").unwrap(), Decision::Terminal);
        assert!(matches!(
            Decision::parse("always"),
            Err(AxiomataError::PermissionAsk { .. })
        ));
    }

    #[test]
    fn the_pane_settles_only_its_own_open_request_and_a_withdrawn_one_stays_closed() {
        let db = db();
        let id = ask(&db, 4, "bash", "wc -l README.md").unwrap();
        assert!(
            !answer_in_pane(&db, 5, id, Decision::Allow).unwrap(),
            "another session's request"
        );
        assert!(answer_in_pane(&db, 4, id, Decision::Allow).unwrap());
        assert_eq!(decision_of(&db, id).unwrap(), Some(Decision::Allow));
        assert!(
            !answer_in_pane(&db, 4, id, Decision::Deny).unwrap(),
            "settled once"
        );
        let second = ask(&db, 4, "bash", "od -c README.md").unwrap();
        withdraw(&db, 4, second).unwrap();
        assert_eq!(open_ask(&db, 4).unwrap(), None);
        assert!(!answer_in_pane(&db, 4, second, Decision::Allow).unwrap());
    }
}
