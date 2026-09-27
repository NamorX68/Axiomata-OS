//! Finding, logging in to, and starting Opencode 2's background service.
//!
//! Discovery is documented: the service registers itself in
//! `<state>/service.json` (troubleshooting page), and `opencode debug paths
//! state` prints `<state>` (CLI page). What is **not** documented is how a
//! local client logs in with what that file holds — Opencode's own clients use
//! HTTP Basic with the user `opencode` and the file's `password`. That one
//! detail lives in [`LOGIN_USER`] and [`Service::authorized`] and nowhere
//! else; a refused login surfaces as [`OpencodeError::Unauthorized`] rather
//! than a vague failure (plan Q10, upstream issue #51724).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use crate::error::OpencodeError;

/// The user name Opencode's own clients log in with (undocumented, see the module doc).
const LOGIN_USER: &str = "opencode";
/// The discovery file inside Opencode's state directory.
const DISCOVERY_FILE: &str = "service.json";
/// The major version this client speaks.
const SUPPORTED_MAJOR: u64 = 2;
/// The newest version this client was checked against; a newer 2.x is logged.
pub const TESTED_VERSION: &str = "2.0.18";
/// How long one ordinary request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long `opencode debug paths` / `opencode service start` may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest answer read from the service. A page of messages is far smaller;
/// the cap keeps a misbehaving service from exhausting the app's memory.
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// What `service.json` holds (the fields this client reads).
#[derive(Deserialize)]
struct Discovery {
    url: String,
    password: String,
}

/// A connection to the running Opencode service.
///
/// Cheap to clone (the HTTP client is reference-counted). The password is
/// never printed: [`std::fmt::Debug`] leaves it out.
#[derive(Clone)]
pub struct Service {
    http: reqwest::Client,
    base: String,
    password: String,
    version: String,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("base", &self.base)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl Service {
    /// Connects to the running service, starting it first when it is not
    /// running (plan Q6), and checks its version (plan Q8).
    ///
    /// Args:
    ///     opencode: Path of the `opencode` binary.
    ///     child_env: The complete environment for the `opencode` commands
    ///         this runs (`debug paths`, `service start`) — a service started
    ///         here keeps it, so the caller passes its sanitised allowlist.
    ///
    /// Errors:
    ///     [`OpencodeError::Unavailable`] when the service cannot be found or
    ///     reached even after starting it, [`OpencodeError::Unauthorized`]
    ///     when it refuses the login, [`OpencodeError::UnsupportedVersion`]
    ///     for another major version.
    pub async fn connect(
        opencode: &Path,
        child_env: &[(String, String)],
    ) -> Result<Self, OpencodeError> {
        let state = state_dir(opencode, child_env).await?;
        let file = state.join(DISCOVERY_FILE);
        let service = match Self::from_discovery(&file).await {
            Ok(service) => service,
            Err(OpencodeError::Unavailable(reason)) => {
                tracing::info!(%reason, "opencode: service not reachable, starting it");
                start_service(opencode, child_env).await?;
                Self::from_discovery(&file).await?
            }
            Err(err) => return Err(err),
        };
        check_version(&service.version)?;
        Ok(service)
    }

    /// Connects to the service only if it is already running — for a watcher
    /// that must not start Opencode just to look (nothing runs on a service
    /// that is not there). Checks the version like [`Service::connect`].
    ///
    /// Errors:
    ///     [`OpencodeError::Unavailable`] when no service is running; the
    ///     other errors as for [`Service::connect`].
    pub async fn find(
        opencode: &Path,
        child_env: &[(String, String)],
    ) -> Result<Self, OpencodeError> {
        let state = state_dir(opencode, child_env).await?;
        let service = Self::from_discovery(&state.join(DISCOVERY_FILE)).await?;
        check_version(&service.version)?;
        Ok(service)
    }

    /// Reads the discovery file and asks the service who it is.
    async fn from_discovery(file: &Path) -> Result<Self, OpencodeError> {
        let text = tokio::fs::read_to_string(file)
            .await
            .map_err(|err| OpencodeError::Unavailable(format!("{}: {err}", file.display())))?;
        check_owner(file)?;
        let discovery: Discovery = serde_json::from_str(&text)
            .map_err(|err| OpencodeError::Protocol(format!("{}: {err}", file.display())))?;
        let base = loopback_base(&discovery.url)?;
        let http = reqwest::Client::builder()
            // The service is on the loopback; a proxy from the environment
            // must never see the login.
            .no_proxy()
            .build()?;
        let mut service = Self {
            http,
            base,
            password: discovery.password,
            version: String::new(),
        };
        let info = match service.get("/api/info").await {
            Ok(info) => info,
            Err(OpencodeError::Transport(reason)) => {
                return Err(OpencodeError::Unavailable(reason));
            }
            Err(err) => return Err(err),
        };
        service.version = info
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| OpencodeError::Protocol("/api/info has no version".into()))?
            .to_string();
        Ok(service)
    }

    /// The version the service reported.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The service's base URL (`http://127.0.0.1:<port>`).
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// A request to `path` with the login attached.
    pub(crate) fn authorized(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .basic_auth(LOGIN_USER, Some(&self.password))
    }

    /// `GET path`, answered JSON.
    pub(crate) async fn get(&self, path: &str) -> Result<Value, OpencodeError> {
        self.send("GET", self.authorized(reqwest::Method::GET, path), path)
            .await
    }

    /// `POST path` with a JSON body, answered JSON (or `null` for no content).
    pub(crate) async fn post(&self, path: &str, body: &Value) -> Result<Value, OpencodeError> {
        let request = self
            .authorized(reqwest::Method::POST, path)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        self.send("POST", request, path).await
    }

    pub(crate) async fn send(
        &self,
        method: &'static str,
        request: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<Value, OpencodeError> {
        let mut response = request.timeout(REQUEST_TIMEOUT).send().await?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(OpencodeError::Protocol(format!(
                    "{method} {path}: answer larger than {MAX_BODY_BYTES} bytes"
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8_lossy(&bytes);
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(OpencodeError::Unauthorized);
        }
        if !status.is_success() {
            return Err(OpencodeError::Http {
                method,
                path: path.to_string(),
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body)
            .map_err(|err| OpencodeError::Protocol(format!("{method} {path}: {err}")))
    }
}

/// The discovery file must belong to this user and be writable by nobody
/// else: another account that could plant one would receive every prompt.
fn check_owner(file: &Path) -> Result<(), OpencodeError> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(file)
        .map_err(|err| OpencodeError::Unavailable(format!("{}: {err}", file.display())))?;
    let own = meta.uid() == rustix::process::getuid().as_raw();
    if !own || meta.mode() & 0o022 != 0 {
        return Err(OpencodeError::Protocol(format!(
            "{} is not owned by this user or is writable by others",
            file.display()
        )));
    }
    Ok(())
}

/// The service's base URL, accepted only on the loopback: the login travels
/// with every request, so a discovery file pointing elsewhere is refused.
fn loopback_base(url: &str) -> Result<String, OpencodeError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|err| OpencodeError::Protocol(format!("service url: {err}")))?;
    let host = parsed.host_str().unwrap_or_default();
    let loopback = host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if parsed.scheme() != "http" || !loopback {
        return Err(OpencodeError::Protocol(format!(
            "the service url {url} is not a plain-HTTP loopback address"
        )));
    }
    Ok(url.trim_end_matches('/').to_string())
}

/// Refuses another major version; logs a newer 2.x (plan Q8).
fn check_version(version: &str) -> Result<(), OpencodeError> {
    let parts = parse_version(version).ok_or_else(|| {
        OpencodeError::Protocol(format!("unreadable Opencode version {version:?}"))
    })?;
    if parts.0 != SUPPORTED_MAJOR {
        return Err(OpencodeError::UnsupportedVersion {
            found: version.to_string(),
        });
    }
    if parse_version(TESTED_VERSION).is_some_and(|tested| parts > tested) {
        tracing::info!(
            version,
            tested = TESTED_VERSION,
            "opencode: newer than the version Axiomata was checked against — check one real skill run"
        );
    }
    Ok(())
}

/// `major.minor.patch` of a version like `2.0.18` (a suffix after `-` is ignored).
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().split(['-', '+']).next()?;
    let mut numbers = core.split('.').map(str::parse::<u64>);
    let major = numbers.next()?.ok()?;
    let minor = numbers.next().unwrap_or(Ok(0)).ok()?;
    let patch = numbers.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

/// Opencode's state directory, as `opencode debug paths state` prints it.
async fn state_dir(
    opencode: &Path,
    child_env: &[(String, String)],
) -> Result<PathBuf, OpencodeError> {
    let output = run_command(opencode, &["debug", "paths", "state"], child_env).await?;
    let line = output.trim();
    if line.is_empty() {
        return Err(OpencodeError::Protocol(
            "`opencode debug paths state` printed nothing".into(),
        ));
    }
    Ok(PathBuf::from(line))
}

/// `opencode service start` (plan Q6).
async fn start_service(
    opencode: &Path,
    child_env: &[(String, String)],
) -> Result<(), OpencodeError> {
    run_command(opencode, &["service", "start"], child_env)
        .await
        .map(|_| ())
        .map_err(|err| {
            OpencodeError::Unavailable(format!("`opencode service start` failed: {err}"))
        })
}

/// Runs `opencode <args>` with exactly `child_env`, returning its stdout.
async fn run_command(
    opencode: &Path,
    args: &[&str],
    child_env: &[(String, String)],
) -> Result<String, OpencodeError> {
    let mut command = Command::new(opencode);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_clear()
        .envs(child_env.iter().map(|(k, v)| (k, v)));
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            OpencodeError::Unavailable(format!("`opencode {}` did not finish", args.join(" ")))
        })?
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => OpencodeError::NotInstalled,
            _ => OpencodeError::Unavailable(format!("`opencode {}`: {err}", args.join(" "))),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OpencodeError::Unavailable(format!(
            "`opencode {}` exited with {}: {}",
            args.join(" "),
            output.status,
            stderr.trim().chars().take(300).collect::<String>()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_with_and_without_suffixes() {
        assert_eq!(parse_version("2.0.18"), Some((2, 0, 18)));
        assert_eq!(parse_version("2.1"), Some((2, 1, 0)));
        assert_eq!(parse_version("2.0.18-beta.1"), Some((2, 0, 18)));
        assert_eq!(parse_version("v2"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn only_major_version_2_is_accepted() {
        assert!(check_version("2.0.18").is_ok());
        assert!(check_version("2.9.0").is_ok(), "a newer 2.x is only logged");
        assert!(matches!(
            check_version("1.18.29"),
            Err(OpencodeError::UnsupportedVersion { .. })
        ));
        assert!(matches!(
            check_version("3.0.0"),
            Err(OpencodeError::UnsupportedVersion { .. })
        ));
        assert!(matches!(
            check_version("nonsense"),
            Err(OpencodeError::Protocol(_))
        ));
    }

    #[test]
    fn the_service_url_must_be_plain_http_on_the_loopback() {
        assert_eq!(
            loopback_base("http://127.0.0.1:49374").unwrap(),
            "http://127.0.0.1:49374"
        );
        assert_eq!(
            loopback_base("http://localhost:4096/").unwrap(),
            "http://localhost:4096"
        );
        assert!(loopback_base("http://[::1]:4096").is_ok());
        for bad in [
            "https://127.0.0.1:4096",
            "http://192.168.1.10:4096",
            "http://example.com",
            "not a url",
        ] {
            assert!(loopback_base(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_discovery_file_writable_by_others_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let file = std::env::temp_dir().join(format!("axiomata-discovery-{}", std::process::id()));
        std::fs::write(&file, "{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(check_owner(&file).is_ok());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o622)).unwrap();
        assert!(matches!(
            check_owner(&file),
            Err(OpencodeError::Protocol(_))
        ));
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn debug_output_leaves_the_password_out() {
        let service = Service {
            http: reqwest::Client::new(),
            base: "http://127.0.0.1:1".into(),
            password: "top-secret".into(),
            version: "2.0.18".into(),
        };
        assert!(!format!("{service:?}").contains("top-secret"));
    }
}
