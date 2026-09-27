//! The service's event stream, `GET /api/event` (server-sent events).
//!
//! Every event is one `data:` line of JSON — `{id, type, created, location,
//! data}` — and `: heartbeat` comment lines keep the connection alive. The
//! stream is volatile by contract (API reference): a slow reader loses it and
//! nothing is replayed, so a caller that must not miss an outcome re-reads the
//! session after the stream ends (see [`crate::turn`]).
//!
//! The event payloads are not described in the OpenAPI document (it types them
//! as a JSON string); the ones this crate reads were measured against 2.0.18
//! and are listed in `docs/plans/opencode2.md`.

use serde_json::Value;

use crate::error::OpencodeError;
use crate::service::Service;

/// One event from the service.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The event type, e.g. `session.execution.succeeded`.
    pub kind: String,
    /// The event's `data` object (`Null` when it has none).
    pub data: Value,
}

impl Event {
    /// The session the event belongs to, when it names one (`data.sessionID`).
    pub fn session_id(&self) -> Option<&str> {
        self.data.get("sessionID").and_then(Value::as_str)
    }

    /// Reads one SSE `data` payload; `None` for anything that is not an event object.
    fn from_payload(payload: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(payload).ok()?;
        let kind = value.get("type")?.as_str()?.to_string();
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        Some(Self { kind, data })
    }
}

/// Largest line or event payload accepted from the stream; a service that
/// never ends a line must not grow the buffer until the app runs out of memory.
const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

/// Splits a byte stream into SSE event payloads (the joined `data` lines of
/// each event). Pure, so it is tested without a server.
#[derive(Default)]
struct SseParser {
    buffer: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// Feeds bytes and returns the payloads of every event they completed.
    /// Fails when a line or an event outgrows [`MAX_EVENT_BYTES`].
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, OpencodeError> {
        self.buffer.extend_from_slice(bytes);
        let mut payloads = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    payloads.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_string());
                if self.data.iter().map(String::len).sum::<usize>() > MAX_EVENT_BYTES {
                    return Err(OpencodeError::Protocol(
                        "an event outgrew the size limit".into(),
                    ));
                }
            }
            // `:` comments (heartbeats) and the `id:`/`event:`/`retry:`
            // fields are not needed: the type travels inside the JSON.
        }
        if self.buffer.len() > MAX_EVENT_BYTES {
            return Err(OpencodeError::Protocol(
                "an event line outgrew the size limit".into(),
            ));
        }
        Ok(payloads)
    }
}

/// An open `GET /api/event` connection.
pub struct EventStream {
    response: reqwest::Response,
    parser: SseParser,
    pending: std::collections::VecDeque<Event>,
}

impl EventStream {
    /// Opens the stream. Events that happen after this returns are delivered.
    pub async fn open(service: &Service) -> Result<Self, OpencodeError> {
        let response = service
            .authorized(reqwest::Method::GET, "/api/event")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => Err(OpencodeError::Unauthorized),
            status if !status.is_success() => Err(OpencodeError::Http {
                method: "GET",
                path: "/api/event".into(),
                status: status.as_u16(),
                body: String::new(),
            }),
            _ => Ok(Self {
                response,
                parser: SseParser::default(),
                pending: Default::default(),
            }),
        }
    }

    /// The next event; `Ok(None)` when the service closed the stream.
    pub async fn next(&mut self) -> Result<Option<Event>, OpencodeError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            let Some(chunk) = self.response.chunk().await? else {
                return Ok(None);
            };
            self.pending.extend(
                self.parser
                    .push(&chunk)?
                    .iter()
                    .filter_map(|p| Event::from_payload(p)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_split_across_chunk_boundaries_and_heartbeats_are_skipped() {
        let mut parser = SseParser::default();
        let mut payloads = parser
            .push(b"data: {\"type\":\"server.connected\",\"data\":{}}\n\n: heart")
            .unwrap();
        payloads.extend(
            parser
                .push(b"beat\n\ndata: {\"type\":\"session.execution.started\",")
                .unwrap(),
        );
        assert_eq!(payloads.len(), 1, "the second event is not complete yet");
        payloads.extend(
            parser
                .push(b"\"data\":{\"sessionID\":\"ses_a\"}}\r\n\r\n")
                .unwrap(),
        );
        let events: Vec<Event> = payloads
            .iter()
            .filter_map(|p| Event::from_payload(p))
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "server.connected");
        assert_eq!(events[1].kind, "session.execution.started");
        assert_eq!(events[1].session_id(), Some("ses_a"));
    }

    #[test]
    fn multi_line_data_is_joined_and_junk_is_ignored() {
        let mut parser = SseParser::default();
        let payloads = parser
            .push(
                b"data: {\"type\":\ndata: \"x\"}\n\ndata: not json\n\ndata: {\"no\":\"type\"}\n\n",
            )
            .unwrap();
        let events: Vec<Event> = payloads
            .iter()
            .filter_map(|p| Event::from_payload(p))
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "x");
        assert_eq!(events[0].data, Value::Null);
        assert_eq!(events[0].session_id(), None);
    }

    #[test]
    fn a_line_or_an_event_past_the_limit_fails_the_stream() {
        let mut endless = SseParser::default();
        let chunk = vec![b'x'; 1024 * 1024];
        let mut result = Ok(Vec::new());
        for _ in 0..=2 * MAX_EVENT_BYTES / chunk.len() {
            result = endless.push(&chunk);
            if result.is_err() {
                break;
            }
        }
        assert!(
            matches!(result, Err(OpencodeError::Protocol(_))),
            "a line without an end"
        );

        let mut huge = SseParser::default();
        let line = format!("data: {}\n", "y".repeat(1024 * 1024));
        let mut result = Ok(Vec::new());
        for _ in 0..=2 * MAX_EVENT_BYTES / line.len() {
            result = huge.push(line.as_bytes());
            if result.is_err() {
                break;
            }
        }
        assert!(
            matches!(result, Err(OpencodeError::Protocol(_))),
            "an event without an end"
        );
    }
}
