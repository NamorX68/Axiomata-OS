//! The service's MCP servers, per location (`docs/plans/opencode2.md`).
//!
//! Opencode 2 runs a location's MCP servers per directory, and a server whose
//! process ended stays `failed` there — nothing restarts it (the service
//! offers no retry). A connector skill on such a location then finds no mail
//! tool and answers with an empty digest while counting as a success. So
//! before a session is created, [`Service::reconnect_failed_mcp`] asks for the
//! location's servers and connects the failed ones again.

use serde_json::Value;

use crate::error::OpencodeError;
use crate::service::Service;

/// The header that names the location (the directory) a request is about.
const LOCATION_HEADER: &str = "x-opencode-directory";

/// One MCP server of a location and its status word (`connected`, `failed`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub status: String,
    /// Why it failed, when it did.
    pub error: Option<String>,
}

/// Reads `GET /api/mcp`'s answer.
fn parse_servers(answer: &Value) -> Vec<McpServer> {
    answer
        .get("data")
        .and_then(Value::as_array)
        .map(|servers| {
            servers
                .iter()
                .filter_map(|s| {
                    Some(McpServer {
                        name: s.get("name")?.as_str()?.to_string(),
                        status: s.pointer("/status/status")?.as_str()?.to_string(),
                        error: s
                            .pointer("/status/error")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A server name as a path segment: the characters a config key uses.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

impl Service {
    /// The MCP servers of the location `directory`, with their status.
    pub async fn mcp_servers(&self, directory: &str) -> Result<Vec<McpServer>, OpencodeError> {
        let request = self
            .authorized(reqwest::Method::GET, "/api/mcp")
            .header(LOCATION_HEADER, directory);
        Ok(parse_servers(&self.send("GET", request, "/api/mcp").await?))
    }

    /// Connects every `failed` MCP server of `directory` again; returns the
    /// names it reconnected. A server that will not connect is skipped (and
    /// stays failed) — the run goes on and says what it could not do.
    pub async fn reconnect_failed_mcp(
        &self,
        directory: &str,
    ) -> Result<Vec<String>, OpencodeError> {
        let mut reconnected = Vec::new();
        for server in self.mcp_servers(directory).await? {
            if server.status != "failed" || !valid_name(&server.name) {
                continue;
            }
            let path = format!("/api/experimental/mcp/{}/connect", server.name);
            let request = self
                .authorized(reqwest::Method::POST, &path)
                .header(LOCATION_HEADER, directory)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body("{}");
            match self.send("POST", request, &path).await {
                Ok(_) => reconnected.push(server.name),
                Err(err) => {
                    tracing::warn!(server = %server.name, %err, "MCP server would not reconnect")
                }
            }
        }
        Ok(reconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_servers_and_their_status() {
        let answer = serde_json::json!({
            "location": {"directory": "/v"},
            "data": [
                {"name": "apple-mail", "status": {"status": "failed", "error": "Connection closed"}},
                {"name": "apple-reminders", "status": {"status": "connected"}},
                {"broken": true}
            ]
        });
        assert_eq!(
            parse_servers(&answer),
            vec![
                McpServer {
                    name: "apple-mail".into(),
                    status: "failed".into(),
                    error: Some("Connection closed".into())
                },
                McpServer {
                    name: "apple-reminders".into(),
                    status: "connected".into(),
                    error: None
                },
            ]
        );
    }

    #[test]
    fn only_plain_names_become_a_path() {
        assert!(valid_name("apple-mail"));
        assert!(!valid_name("../x"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(""));
    }
}
