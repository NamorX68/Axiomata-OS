//! Sessions on the service: create, prompt, read, and steer them.
//!
//! Endpoints and request shapes are the ones in the API reference
//! (<https://opencode.ai/v2/docs/api/>); responses wrap their payload in
//! `{data: …}`.

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::OpencodeError;
use crate::service::Service;

/// Messages read per page when walking a session backwards.
const PAGE_SIZE: u32 = 50;
/// Upper bound on pages walked: for one turn's messages (a turn is far shorter) and for a whole card session's.
const MAX_PAGES: u32 = 40;

/// A model as the service names it: provider plus model id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelRef {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    pub id: String,
}

impl ModelRef {
    /// Splits Axiomata's `provider/model` form at the first `/` — the model
    /// id itself may contain more (`openrouter/deepseek/deepseek-v4-flash`).
    pub fn parse(full: &str) -> Option<Self> {
        let (provider, model) = full.split_once('/')?;
        (!provider.is_empty() && !model.is_empty()).then(|| Self {
            provider_id: provider.to_string(),
            id: model.to_string(),
        })
    }

    /// Reads a `Model.Ref` object (`{providerID, id}`).
    pub fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            provider_id: value.get("providerID")?.as_str()?.to_string(),
            id: value.get("id")?.as_str()?.to_string(),
        })
    }
}

/// One permission rule (`action`, `resource`, `effect`); the last matching
/// rule wins, and `*` matches anything — in `action` too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionRule {
    pub action: String,
    pub resource: String,
    pub effect: String,
}

impl PermissionRule {
    /// A rule with the given parts.
    pub fn new(action: &str, resource: &str, effect: &str) -> Self {
        Self {
            action: action.to_string(),
            resource: resource.to_string(),
            effect: effect.to_string(),
        }
    }
}

/// What a new session is created with.
#[derive(Debug, Clone, Default)]
pub struct NewSession {
    /// The directory the session works in (its location).
    pub directory: String,
    /// A title; giving one spares the service a model call to make one up.
    pub title: Option<String>,
    pub model: Option<ModelRef>,
    /// The agent (`build`, `plan`, …); `None` = the service default.
    pub agent: Option<String>,
    /// Rules layered over the configured ones for this session.
    pub permissions: Vec<PermissionRule>,
}

/// The user message a prompt created.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompted {
    pub message_id: String,
    /// `time.created` of that message, in milliseconds.
    pub created: i64,
}

/// The `data` of a `{data: …}` answer.
fn data(value: Value, what: &str) -> Result<Value, OpencodeError> {
    match value {
        Value::Object(mut map) => map
            .remove("data")
            .ok_or_else(|| OpencodeError::Protocol(format!("{what}: answer has no data"))),
        _ => Err(OpencodeError::Protocol(format!(
            "{what}: answer is not an object"
        ))),
    }
}

/// A session id as a path segment: only what the service itself hands out.
pub(crate) fn session_path(session_id: &str, rest: &str) -> Result<String, OpencodeError> {
    let valid = session_id.starts_with("ses")
        && session_id.len() <= 128
        && session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return Err(OpencodeError::Protocol(format!(
            "malformed session id {session_id:?}"
        )));
    }
    Ok(format!("/api/session/{session_id}{rest}"))
}

impl Service {
    /// Creates a session and returns its id.
    pub async fn create_session(&self, new: &NewSession) -> Result<String, OpencodeError> {
        let mut body = json!({ "location": { "directory": new.directory } });
        if let Some(title) = &new.title {
            body["title"] = json!(title);
        }
        if let Some(model) = &new.model {
            body["model"] = json!(model);
        }
        if let Some(agent) = &new.agent {
            body["agent"] = json!(agent);
        }
        if !new.permissions.is_empty() {
            body["permissions"] = json!(new.permissions);
        }
        let session = data(self.post("/api/session", &body).await?, "create session")?;
        session
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| OpencodeError::Protocol("created session has no id".into()))
    }

    /// The session's info (`Session.Info`).
    pub async fn session(&self, session_id: &str) -> Result<Value, OpencodeError> {
        data(
            self.get(&session_path(session_id, "")?).await?,
            "get session",
        )
    }

    /// Sends `text` as the next user input; the agent loop starts on it.
    pub async fn prompt(&self, session_id: &str, text: &str) -> Result<Prompted, OpencodeError> {
        let path = session_path(session_id, "/prompt")?;
        let message = data(self.post(&path, &json!({ "text": text })).await?, "prompt")?;
        Ok(Prompted {
            message_id: message
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| OpencodeError::Protocol("prompt answer has no message id".into()))?
                .to_string(),
            created: message
                .pointer("/time/created")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        })
    }

    /// Switches the model for the session's next turns.
    pub async fn switch_model(
        &self,
        session_id: &str,
        model: &ModelRef,
    ) -> Result<(), OpencodeError> {
        let path = session_path(session_id, "/model")?;
        self.post(&path, &json!({ "model": model }))
            .await
            .map(|_| ())
    }

    /// Answers a permission request: `once`, `always` or `reject`.
    pub async fn reply_permission(
        &self,
        session_id: &str,
        request_id: &str,
        decision: &str,
    ) -> Result<(), OpencodeError> {
        if !request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(OpencodeError::Protocol(format!(
                "malformed permission id {request_id:?}"
            )));
        }
        let path = session_path(session_id, &format!("/permission/{request_id}/reply"))?;
        self.post(&path, &json!({ "decision": decision }))
            .await
            .map(|_| ())
    }

    /// Deletes a session and its child sessions.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), OpencodeError> {
        let path = session_path(session_id, "")?;
        let request = self.authorized(reqwest::Method::DELETE, &path);
        self.send("DELETE", request, &path).await.map(|_| ())
    }

    /// Interrupts the session's running execution (a no-op when idle).
    pub async fn interrupt(&self, session_id: &str) -> Result<(), OpencodeError> {
        let path = session_path(session_id, "/interrupt")?;
        self.post(&path, &Value::Null).await.map(|_| ())
    }

    /// The session's newest `limit` messages, newest first.
    pub async fn recent_messages(
        &self,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<Value>, OpencodeError> {
        let path = format!(
            "{}?order=desc&limit={limit}",
            session_path(session_id, "/message")?
        );
        let page = self.get(&path).await?;
        Ok(page
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Every message of the session, newest first, up to a bound of `MAX_PAGES` pages. For counting what a session used:
    /// a session past the bound reads as having used what its newest part did, so a caller keeps the highest figure it
    /// ever read (`Meter` does) rather than trusting one reading to fall no further.
    pub async fn all_messages(&self, session_id: &str) -> Result<Vec<Value>, OpencodeError> {
        let base = session_path(session_id, "/message")?;
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let path = page_path(&base, cursor.as_deref());
            let page = self.get(&path).await?;
            let items = page
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| OpencodeError::Protocol("message list has no data".into()))?;
            all.extend(items.iter().cloned());
            cursor = page
                .pointer("/cursor/next")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() || items.is_empty() {
                break;
            }
        }
        Ok(all)
    }

    /// The messages after `after_message_id`, oldest first. Walks the
    /// session backwards page by page until it reaches that message, so a
    /// long chat session is not read in full for one turn.
    pub async fn messages_after(
        &self,
        session_id: &str,
        after_message_id: &str,
    ) -> Result<Vec<Value>, OpencodeError> {
        let base = session_path(session_id, "/message")?;
        let mut newer = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let path = page_path(&base, cursor.as_deref());
            let page = self.get(&path).await?;
            let items = page
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| OpencodeError::Protocol("message list has no data".into()))?;
            for message in items {
                if message.get("id").and_then(Value::as_str) == Some(after_message_id) {
                    newer.reverse();
                    return Ok(newer);
                }
                newer.push(message.clone());
            }
            cursor = page
                .pointer("/cursor/next")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() || items.is_empty() {
                break;
            }
        }
        Err(OpencodeError::Protocol(format!(
            "the prompt message {after_message_id} is not in the session"
        )))
    }
}

/// The path of one page of a session's messages, newest first. The first page names the order; a later page names only
/// its cursor, which carries the order itself — the service refuses `cursor` together with `order` (HTTP 400,
/// `InvalidCursorError`, Opencode 2.0.23).
fn page_path(base: &str, cursor: Option<&str>) -> String {
    match cursor {
        None => format!("{base}?order=desc&limit={PAGE_SIZE}"),
        Some(cursor) => format!("{base}?limit={PAGE_SIZE}&cursor={}", percent_encode(cursor)),
    }
}

/// Percent-encodes a query value (the cursor is opaque and may hold anything).
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_page_names_the_order_a_later_one_names_its_cursor() {
        let first = page_path("/api/session/s/message", None);
        assert!(
            first.contains("order=desc") && !first.contains("cursor"),
            "{first}"
        );
        let later = page_path("/api/session/s/message", Some("a b+c"));
        assert!(
            later.contains("cursor=a%20b%2Bc") && !later.contains("order"),
            "{later}"
        );
    }

    #[test]
    fn model_refs_split_at_the_first_slash() {
        assert_eq!(
            ModelRef::parse("openrouter/deepseek/deepseek-v4-flash-0731"),
            Some(ModelRef {
                provider_id: "openrouter".into(),
                id: "deepseek/deepseek-v4-flash-0731".into()
            })
        );
        assert_eq!(
            ModelRef::parse("ollama/qwen3.8:27b-mlx").unwrap().id,
            "qwen3.8:27b-mlx"
        );
        for bad in ["no-slash", "/model", "provider/", ""] {
            assert_eq!(ModelRef::parse(bad), None, "{bad}");
        }
        let value =
            json!({"providerID": "anthropic", "id": "claude-sonnet-5", "variant": "default"});
        assert_eq!(
            ModelRef::from_value(&value),
            ModelRef::parse("anthropic/claude-sonnet-5")
        );
    }

    #[test]
    fn model_refs_and_rules_serialise_as_the_api_expects() {
        let model = ModelRef::parse("ollama/lfm2.5:8b").unwrap();
        assert_eq!(
            json!(model),
            json!({"providerID": "ollama", "id": "lfm2.5:8b"})
        );
        let rule = PermissionRule::new("shell", "git push *", "deny");
        assert_eq!(
            json!(rule),
            json!({"action": "shell", "resource": "git push *", "effect": "deny"})
        );
    }

    #[test]
    fn only_service_issued_session_ids_become_paths() {
        assert_eq!(
            session_path("ses_f1b70b674ffeHqDO9q4XR03c9z", "/prompt").unwrap(),
            "/api/session/ses_f1b70b674ffeHqDO9q4XR03c9z/prompt"
        );
        for bad in ["", "abc", "ses/../x", "ses x", "ses?y=1"] {
            assert!(session_path(bad, "").is_err(), "{bad}");
        }
    }

    #[test]
    fn answers_are_unwrapped_from_data() {
        assert_eq!(
            data(json!({"data": {"id": 1}}), "x").unwrap(),
            json!({"id": 1})
        );
        assert!(data(json!({"nope": 1}), "x").is_err());
        assert!(data(json!([1]), "x").is_err());
    }

    #[test]
    fn cursors_are_percent_encoded() {
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("x=1&y/z"), "x%3D1%26y%2Fz");
    }
}
