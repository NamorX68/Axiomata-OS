//! Signing the app's own Claude Code in and out (`docs/plans/harness-choice.md`, D7).
//!
//! The login lives in the app's config folder (`~/.axiomata/claude`, D5), never the owner's `~/.claude`. Every command
//! line is fixed here: a caller (the Settings dialog through Tauri) can only ask for the state, a sign-in, a code for a
//! running sign-in, or a sign-out — never name a program, a flag or a folder.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin};

use crate::agents::BACKEND_CLAUDE_CODE;
use crate::agents::claude_code::{CLAUDE_BIN, app_command};
use crate::error::AxiomataError;

/// How long a sign-in may wait for the owner in the browser before it is given up.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How long a status or sign-out call may take; both answer at once when Claude Code works.
const QUICK_TIMEOUT: Duration = Duration::from_secs(20);

/// The longest code a sign-in takes: the OAuth code Claude Code shows, with its `#state` part.
const MAX_CODE_LEN: usize = 512;

/// The login state of the app's Claude Code, as the Settings dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginStatus {
    pub logged_in: bool,
    /// The account's address, when signed in.
    pub email: Option<String>,
    /// The subscription (`max`, `pro`, …), when Claude Code names it.
    pub subscription: Option<String>,
}

/// `claude auth status` as it prints it (JSON is its default).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusJson {
    #[serde(default)]
    logged_in: bool,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    subscription_type: Option<String>,
}

fn api_err(message: String) -> AxiomataError {
    AxiomataError::AgentApi {
        backend: BACKEND_CLAUDE_CODE,
        message,
    }
}

fn spawn_err(source: std::io::Error) -> AxiomataError {
    AxiomataError::AgentSpawn {
        backend: BACKEND_CLAUDE_CODE,
        program: CLAUDE_BIN,
        source,
    }
}

/// Whether the app's Claude Code is signed in.
///
/// # Errors
///
/// [`AxiomataError::AgentSpawn`] when `claude` cannot be started; [`AxiomataError::AgentApi`] when its answer does not
/// read; [`AxiomataError::AgentTimeout`] when it does not answer.
pub async fn status() -> Result<LoginStatus, AxiomataError> {
    status_of(Path::new(CLAUDE_BIN)).await
}

async fn status_of(program: &Path) -> Result<LoginStatus, AxiomataError> {
    let output = quick(app_command(program).args(["auth", "status", "--json"])).await?;
    parse_status(&String::from_utf8_lossy(&output.stdout))
}

fn parse_status(stdout: &str) -> Result<LoginStatus, AxiomataError> {
    let start = stdout.find('{').ok_or_else(|| {
        api_err(format!(
            "`claude auth status` gave no state: {}",
            stdout.trim()
        ))
    })?;
    let json: StatusJson = serde_json::from_str(stdout[start..].trim())
        .map_err(|err| api_err(format!("`claude auth status` does not read: {err}")))?;
    Ok(LoginStatus {
        logged_in: json.logged_in,
        email: json.email.filter(|_| json.logged_in),
        subscription: json.subscription_type.filter(|_| json.logged_in),
    })
}

/// Signs the app's Claude Code out.
///
/// # Errors
///
/// As [`status`].
pub async fn sign_out() -> Result<LoginStatus, AxiomataError> {
    quick(app_command(Path::new(CLAUDE_BIN)).args(["auth", "logout"])).await?;
    status().await
}

async fn quick(
    command: &mut tokio::process::Command,
) -> Result<std::process::Output, AxiomataError> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(spawn_err)?;
    tokio::time::timeout(QUICK_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| AxiomataError::AgentTimeout {
            backend: BACKEND_CLAUDE_CODE,
            timeout: QUICK_TIMEOUT,
        })?
        .map_err(spawn_err)
}

/// A running sign-in: Claude Code has opened the browser and waits for the owner there — or, when the browser cannot
/// hand the answer back, for the code the page shows, given with [`SignIn::send_code`].
pub struct SignIn {
    child: Child,
    /// The sign-in leads its own process group (a browser callback helper of Claude Code's included); its pid is the
    /// group's id.
    group: Option<Pid>,
}

/// The part of a running sign-in that takes a code; held apart from the [`SignIn`] so a code can be sent while
/// another task waits for the sign-in to end.
pub struct CodeInput {
    stdin: ChildStdin,
}

impl SignIn {
    /// Starts `claude auth login --claudeai` for the app's folder: the subscription login, never an API key.
    ///
    /// # Errors
    ///
    /// [`AxiomataError::AgentSpawn`] when `claude` cannot be started.
    pub fn start() -> Result<(Self, CodeInput), AxiomataError> {
        Self::start_with(Path::new(CLAUDE_BIN))
    }

    fn start_with(program: &Path) -> Result<(Self, CodeInput), AxiomataError> {
        let mut child = app_command(program)
            .args(["auth", "login", "--claudeai"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(spawn_err)?;
        let group = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw);
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| api_err("the sign-in has no input".to_owned()))?;
        Ok((Self { child, group }, CodeInput { stdin }))
    }

    /// Waits for the sign-in to end — at most [`SIGN_IN_TIMEOUT`], after which it is stopped — and reads the state
    /// it left.
    ///
    /// # Errors
    ///
    /// [`AxiomataError::AgentTimeout`] when the owner did not finish in time; otherwise as [`status`].
    pub async fn finish(self) -> Result<LoginStatus, AxiomataError> {
        self.finish_within(SIGN_IN_TIMEOUT, Path::new(CLAUDE_BIN))
            .await
    }

    async fn finish_within(
        mut self,
        limit: Duration,
        program: &Path,
    ) -> Result<LoginStatus, AxiomataError> {
        match tokio::time::timeout(limit, self.child.wait()).await {
            Ok(waited) => {
                waited.map_err(spawn_err)?;
            }
            Err(_) => {
                if let Some(group) = self.group {
                    // Already gone is fine: the group ended by itself.
                    let _ = kill_process_group(group, Signal::KILL);
                }
                let _ = self.child.kill().await;
                return Err(AxiomataError::AgentTimeout {
                    backend: BACKEND_CLAUDE_CODE,
                    timeout: limit,
                });
            }
        }
        status_of(program).await
    }
}

impl CodeInput {
    /// Hands the code the sign-in page shows to the waiting sign-in.
    ///
    /// # Errors
    ///
    /// [`AxiomataError::AgentApi`] for a code that is empty, too long or holds anything but the characters such a code
    /// has (letters, digits, `-`, `_`, `#`, `.`); [`AxiomataError::AgentSpawn`] when the sign-in no longer listens.
    pub async fn send_code(&mut self, code: &str) -> Result<(), AxiomataError> {
        let code = code.trim();
        let plain = code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '#' | '.'));
        if code.is_empty() || code.len() > MAX_CODE_LEN || !plain {
            return Err(api_err("that does not look like a sign-in code".to_owned()));
        }
        self.stdin
            .write_all(format!("{code}\n").as_bytes())
            .await
            .map_err(spawn_err)?;
        self.stdin.flush().await.map_err(spawn_err)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;

    /// A folder of its own with an executable stand-in for `claude`, removed when dropped.
    struct Fake(PathBuf);

    impl Fake {
        fn new(name: &str, body: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "axiomata-claude-auth-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let program = dir.join("claude");
            std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self(dir)
        }

        fn program(&self) -> PathBuf {
            self.0.join("claude")
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_state_reads_as_claude_code_prints_it() {
        let signed_in = parse_status(
            "{\n  \"loggedIn\": true,\n  \"email\": \"a@b.c\",\n  \"subscriptionType\": \"max\",\n  \"orgId\": \"x\"\n}",
        )
        .unwrap();
        assert_eq!(
            signed_in,
            LoginStatus {
                logged_in: true,
                email: Some("a@b.c".into()),
                subscription: Some("max".into())
            }
        );
        let out = parse_status("{\"loggedIn\": false, \"email\": \"stale@b.c\"}").unwrap();
        assert!(!out.logged_in);
        assert_eq!(out.email, None);
        assert!(matches!(
            parse_status("boom"),
            Err(AxiomataError::AgentApi { .. })
        ));
    }

    /// The fake answers what it was asked, so the test sees the fixed command line and the app's folder.
    #[tokio::test]
    async fn the_status_asks_the_apps_folder_with_a_fixed_command() {
        let fake = Fake::new(
            "status",
            "printf '{\"loggedIn\": true, \"email\": \"%s|%s\"}' \"$*\" \"$CLAUDE_CONFIG_DIR\"",
        );
        let status = status_of(&fake.program()).await.unwrap();
        let email = status.email.unwrap();
        let (args, folder) = email.split_once('|').unwrap();
        assert_eq!(args, "auth status --json");
        assert!(folder.ends_with("claude"), "{folder}");
    }

    /// A sign-in reads the code from its input, ends, and the state it left is read back.
    #[tokio::test]
    async fn a_sign_in_takes_a_code_and_reports_the_state_after() {
        let fake = Fake::new(
            "sign-in",
            "case \"$*\" in\n\
             \"auth login --claudeai\") read code; echo \"$code\" > \"$(dirname \"$0\")/got\" ;;\n\
             \"auth status --json\") echo '{\"loggedIn\": true, \"email\": \"a@b.c\"}' ;;\n\
             esac",
        );
        let (sign_in, mut input) = SignIn::start_with(&fake.program()).unwrap();
        input.send_code("abc-123#state_x").await.unwrap();
        let status = sign_in
            .finish_within(Duration::from_secs(10), &fake.program())
            .await
            .unwrap();
        assert!(status.logged_in);
        assert_eq!(
            std::fs::read_to_string(fake.0.join("got")).unwrap().trim(),
            "abc-123#state_x"
        );
    }

    #[tokio::test]
    async fn a_code_with_anything_else_is_refused_before_it_reaches_claude() {
        let fake = Fake::new("bad-code", "sleep 5");
        let (_sign_in, mut input) = SignIn::start_with(&fake.program()).unwrap();
        for bad in [
            "",
            "a b",
            "x\ny",
            "$(rm -rf /)",
            &"a".repeat(MAX_CODE_LEN + 1),
        ] {
            assert!(
                matches!(
                    input.send_code(bad).await,
                    Err(AxiomataError::AgentApi { .. })
                ),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_sign_in_nobody_finishes_is_stopped_at_its_limit() {
        let fake = Fake::new("stuck", "sleep 30");
        let (sign_in, _input) = SignIn::start_with(&fake.program()).unwrap();
        let err = sign_in
            .finish_within(Duration::from_millis(200), &fake.program())
            .await
            .unwrap_err();
        assert!(matches!(err, AxiomataError::AgentTimeout { .. }));
    }
}
