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

/// Where a registered server stands, judged from the location's list.
#[derive(Debug, PartialEq, Eq)]
enum Standing {
    Connected,
    /// The process ended or was refused; the service's reason, which is the server's own stderr.
    Failed(String),
    /// Not listed yet, or still starting.
    Pending,
}

fn standing(servers: &[McpServer], name: &str) -> Standing {
    match servers.iter().find(|server| server.name == name) {
        Some(server) if server.status == "connected" => Standing::Connected,
        Some(server) if server.status == "failed" => Standing::Failed(
            server
                .error
                .clone()
                .unwrap_or_else(|| "no reason given".to_string()),
        ),
        _ => Standing::Pending,
    }
}

/// A server name as a path segment: the characters a config key uses.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// The path of one server's registration; a name that is no config key is refused before it becomes a path.
fn server_path(name: &str) -> Result<String, OpencodeError> {
    if !valid_name(name) {
        return Err(OpencodeError::Protocol(format!(
            "{name:?} is not a valid MCP server name"
        )));
    }
    Ok(format!("/api/experimental/mcp/{name}"))
}

impl Service {
    /// The MCP servers of the location `directory`, with their status.
    pub async fn mcp_servers(&self, directory: &str) -> Result<Vec<McpServer>, OpencodeError> {
        let request = self
            .authorized(reqwest::Method::GET, "/api/mcp")
            .header(LOCATION_HEADER, directory);
        Ok(parse_servers(&self.send("GET", request, "/api/mcp").await?))
    }

    /// Registers (or replaces) the MCP server `name` at the location `directory`, in the service's memory only — no
    /// file in the directory. A second call with another `config` replaces the first and restarts the server, which
    /// is how a new start of an agent hands the server its new secret. Measured against 2.0.22: the location's own
    /// `opencode.json` is cached after the first load and an edit of it is never read again, so a file could not do
    /// this.
    ///
    /// `config` is a local or remote server config as `opencode.json` has it (`{"type": "local", "command": [...],
    /// "environment": {...}}`). The registration lasts as long as the service does; a restarted service has to be told
    /// again, which the next start of the agent does.
    pub async fn register_mcp(
        &self,
        directory: &str,
        name: &str,
        config: &Value,
    ) -> Result<(), OpencodeError> {
        let path = server_path(name)?;
        let request = self
            .authorized(reqwest::Method::PUT, &path)
            .header(LOCATION_HEADER, directory)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::json!({ "config": config }).to_string());
        self.send("PUT", request, &path).await.map(drop)
    }

    /// Waits until the MCP server `name` at `directory` is `connected`, for at most `attempts` looks `pause` apart.
    /// A `204` from [`Service::register_mcp`] only says the config was accepted; a server that cannot start (a missing
    /// program, a refused secret) shows up here as `failed`. Still starting after the last look counts as fine — the
    /// caller must not hold a start up for a slow server.
    pub async fn await_mcp(
        &self,
        directory: &str,
        name: &str,
        attempts: u32,
        pause: std::time::Duration,
    ) -> Result<(), OpencodeError> {
        for attempt in 0..attempts {
            if attempt > 0 {
                tokio::time::sleep(pause).await;
            }
            match standing(&self.mcp_servers(directory).await?, name) {
                Standing::Connected => return Ok(()),
                Standing::Failed(reason) => {
                    return Err(OpencodeError::Protocol(format!(
                        "the {name} MCP server did not start: {reason}"
                    )));
                }
                Standing::Pending => {}
            }
        }
        Ok(())
    }

    /// Removes the MCP server `name` registered at `directory` (nothing happens when there is none).
    pub async fn remove_mcp(&self, directory: &str, name: &str) -> Result<(), OpencodeError> {
        let path = server_path(name)?;
        let request = self
            .authorized(reqwest::Method::DELETE, &path)
            .header(LOCATION_HEADER, directory);
        self.send("DELETE", request, &path).await.map(drop)
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
    fn a_registered_server_is_judged_from_the_list() {
        let server = |status: &str, error: Option<&str>| McpServer {
            name: "axiomata".into(),
            status: status.into(),
            error: error.map(str::to_owned),
        };
        assert_eq!(standing(&[], "axiomata"), Standing::Pending);
        assert_eq!(
            standing(&[server("connected", None)], "axiomata"),
            Standing::Connected
        );
        assert_eq!(
            standing(&[server("failed", Some("no such file"))], "axiomata"),
            Standing::Failed("no such file".into())
        );
        assert_eq!(
            standing(&[server("failed", None)], "axiomata"),
            Standing::Failed("no reason given".into())
        );
        assert_eq!(
            standing(&[server("connecting", None)], "axiomata"),
            Standing::Pending
        );
        // Another server's trouble is not this one's.
        assert_eq!(
            standing(&[server("failed", Some("x"))], "apple-mail"),
            Standing::Pending
        );
    }

    #[test]
    fn a_registration_path_needs_a_plain_name() {
        assert_eq!(
            server_path("axiomata").unwrap(),
            "/api/experimental/mcp/axiomata"
        );
        assert!(server_path("../session").is_err());
        assert!(server_path("").is_err());
    }

    #[test]
    fn only_plain_names_become_a_path() {
        assert!(valid_name("apple-mail"));
        assert!(!valid_name("../x"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(""));
    }
}
