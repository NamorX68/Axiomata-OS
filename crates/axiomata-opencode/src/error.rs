//! What can go wrong between Axiomata and the Opencode service.

use std::time::Duration;

/// An error talking to Opencode 2's background service.
#[derive(Debug, thiserror::Error)]
pub enum OpencodeError {
    /// The `opencode` binary is not on `PATH`.
    #[error("opencode was not found on PATH")]
    NotInstalled,

    /// The service could not be found or reached, even after starting it.
    #[error("the Opencode service is not reachable: {0}")]
    Unavailable(String),

    /// The service refused the login. The login scheme is the one piece of
    /// the service API Opencode does not document, so this most likely means
    /// an update changed it.
    #[error(
        "the Opencode service refused the login — an Opencode update may have changed how \
         local clients log in (https://github.com/anomalyco/opencode/issues/51724)"
    )]
    Unauthorized,

    /// The service runs a major version this client was not built for.
    #[error("Opencode {found} is not supported — Axiomata talks to Opencode 2")]
    UnsupportedVersion { found: String },

    /// The service answered a request with an error status.
    #[error("Opencode answered {method} {path} with HTTP {status}: {body}")]
    Http {
        method: &'static str,
        path: String,
        status: u16,
        body: String,
    },

    /// The connection failed mid-request.
    #[error("the connection to the Opencode service failed: {0}")]
    Transport(String),

    /// The service answered with something this client cannot read.
    #[error("unexpected answer from the Opencode service: {0}")]
    Protocol(String),

    /// A turn did not finish within its time limit (and was interrupted).
    #[error("the Opencode turn did not finish within {0:?}")]
    Timeout(Duration),
}

impl From<reqwest::Error> for OpencodeError {
    fn from(err: reqwest::Error) -> Self {
        OpencodeError::Transport(err.to_string())
    }
}
