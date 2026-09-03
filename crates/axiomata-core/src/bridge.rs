//! The agent ↔ module bridge ("small MCP"), two halves:
//!
//! 1. **Manifest** — the dashboard hands over the mounted module instances and
//!    their callable actions; [`write_manifest`] renders them as Markdown into
//!    `~/.axiomata/module-context.md`, which the agent gets appended to its
//!    system prompt (`--append-system-prompt-file`, see `agents`).
//! 2. **Action queue** — a file queue under `~/.axiomata/module-actions/`: the
//!    CLI (called by the agent) drops `inbox/<id>.json`, the running dashboard
//!    polls, dispatches the action in the frontend and writes
//!    `outbox/<id>.json`; the CLI waits for that file. Filesystem is the truth,
//!    nobody watches — everything polls.
//!
//! No permissions layer: whatever is mounted is callable.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;
use crate::paths;

/// Opening / closing markers of the generated manifest (the whole file is
/// generated; the markers just make that obvious to a reader).
pub const MANIFEST_START: &str = "<!-- AXIOMATA-MODULES:START -->";
pub const MANIFEST_END: &str = "<!-- AXIOMATA-MODULES:END -->";

/// Default wait for a dashboard response.
pub const DEFAULT_ACTION_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the CLI re-checks the outbox.
pub const RESPONSE_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Outbox files older than this are pruned by the dashboard's poll.
pub const OUTBOX_MAX_AGE: Duration = Duration::from_secs(600);
/// Largest queue file read.
const MAX_QUEUE_FILE_BYTES: u64 = 1024 * 1024;

// ---------------------------------------------------------------- manifest

/// One callable action, as declared by a module (`ModuleAction` in the
/// frontend). `params` is a JSON-Schema object passed through verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestAction {
    pub name: String,
    pub description: String,
    pub params: serde_json::Value,
}

/// One mounted module instance with at least one action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub instance_id: String,
    #[serde(rename = "type")]
    pub module_type: String,
    pub title: String,
    pub actions: Vec<ManifestAction>,
}

/// Neutralises anything from module metadata that could break out of the
/// manifest's structure: control characters, HTML comments, the markers.
fn inline(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("<!--", "<!\u{2011}\u{2011}")
        .replace("-->", "\u{2011}\u{2011}>")
        .replace("AXIOMATA-MODULES", "AXIOMATA\u{2011}MODULES")
        .chars()
        .take(400)
        .collect()
}

/// Renders the manifest Markdown. `cli` is how the agent should invoke the
/// CLI (a bare name on PATH or an absolute path).
pub fn render_manifest(entries: &[ManifestEntry], cli: &str) -> String {
    let mut out = String::new();
    out.push_str(MANIFEST_START);
    out.push_str("\n# Axiomata-OS dashboard modules\n\n");
    out.push_str(
        "The user's Axiomata-OS dashboard is running with the module instances listed \
         below. You can call an instance's action from a shell:\n\n",
    );
    out.push_str(&format!(
        "```\n{cli} module-action <instance_id> <action> --json '<params as JSON object>'\n```\n\n"
    ));
    out.push_str(
        "The command prints the action's result as JSON (or an error) and exits non-zero \
         if the dashboard is not running. Only call actions listed here, with the \
         parameters described by each action's JSON schema.\n",
    );
    if entries.is_empty() {
        out.push_str("\n_No module instances with actions are mounted right now._\n");
    }
    for entry in entries {
        out.push_str(&format!(
            "\n## {} (`{}`) — instance `{}`\n",
            inline(&entry.title),
            inline(&entry.module_type),
            inline(&entry.instance_id)
        ));
        for action in &entry.actions {
            out.push_str(&format!(
                "- `{}` — {}\n  params: `{}`\n",
                inline(&action.name),
                inline(&action.description),
                inline(&action.params.to_string())
            ));
        }
    }
    out.push('\n');
    out.push_str(MANIFEST_END);
    out.push('\n');
    out
}

/// How the agent should call the CLI: `$AXIOMATA_CLI` if set, else an
/// `axiomata-cli` binary next to the running executable, else the bare name.
pub fn cli_invocation() -> String {
    if let Some(explicit) = std::env::var_os("AXIOMATA_CLI") {
        return explicit.to_string_lossy().into_owned();
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("axiomata-cli");
        if sibling.is_file() {
            return sibling.to_string_lossy().into_owned();
        }
    }
    "axiomata-cli".to_string()
}

/// Renders and atomically writes `~/.axiomata/module-context.md`. Returns
/// `true` if the file changed.
pub fn write_manifest(entries: &[ManifestEntry]) -> Result<bool, AxiomataError> {
    let path = paths::module_context_path();
    let text = render_manifest(entries, &cli_invocation());
    if fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    write_atomic(&path, &text)?;
    Ok(true)
}

// ------------------------------------------------------------ action queue

/// A request dropped into `inbox/` by the CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub id: String,
    pub instance_id: String,
    pub action: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// The dashboard's answer, written to `outbox/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResponse {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    pub completed_at: DateTime<Utc>,
}

fn inbox_dir() -> PathBuf {
    paths::module_actions_dir().join("inbox")
}

fn outbox_dir() -> PathBuf {
    paths::module_actions_dir().join("outbox")
}

/// Ids are file names; keep them to a safe alphabet.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A process-unique id without a random-number dependency.
pub fn new_action_id() -> String {
    format!(
        "{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}

/// Writes `request` to `inbox/<id>.json`. Returns the file path.
pub fn enqueue(request: &ActionRequest) -> Result<PathBuf, AxiomataError> {
    if !valid_id(&request.id) {
        return Err(AxiomataError::ModuleAction {
            id: request.id.clone(),
            reason: "malformed id".to_string(),
        });
    }
    let path = inbox_dir().join(format!("{}.json", request.id));
    let json = serde_json::to_string_pretty(request).map_err(|e| AxiomataError::ModuleAction {
        id: request.id.clone(),
        reason: e.to_string(),
    })?;
    write_atomic(&path, &json)?;
    Ok(path)
}

/// Takes every pending request out of `inbox/` (files are removed as they are
/// read, so each request is dispatched at most once). Unparseable files are
/// removed too and reported as failed responses by the caller. Also prunes
/// stale `outbox/` files.
pub fn drain_inbox() -> Result<Vec<ActionRequest>, AxiomataError> {
    prune_outbox(OUTBOX_MAX_AGE);
    let dir = inbox_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(AxiomataError::Io { path: dir, source }),
    };
    let mut requests = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let parsed = read_capped(&path).and_then(|text| {
            serde_json::from_str::<ActionRequest>(&text).map_err(|e| e.to_string())
        });
        let _ = fs::remove_file(&path);
        match parsed {
            Ok(req) if valid_id(&req.id) => requests.push(req),
            Ok(_) | Err(_) => {}
        }
    }
    requests.sort_by_key(|r| r.created_at);
    Ok(requests)
}

/// Writes the dashboard's response to `outbox/<id>.json`.
pub fn complete(response: &ActionResponse) -> Result<(), AxiomataError> {
    if !valid_id(&response.id) {
        return Err(AxiomataError::ModuleAction {
            id: response.id.clone(),
            reason: "malformed id".to_string(),
        });
    }
    let path = outbox_dir().join(format!("{}.json", response.id));
    let json = serde_json::to_string_pretty(response).map_err(|e| AxiomataError::ModuleAction {
        id: response.id.clone(),
        reason: e.to_string(),
    })?;
    write_atomic(&path, &json)
}

/// Blocks until `outbox/<id>.json` appears (then removes it) or `timeout`
/// elapses. On timeout the pending inbox file is withdrawn so a dashboard that
/// starts later doesn't run a stale request.
pub fn wait_for_response(id: &str, timeout: Duration) -> Result<ActionResponse, AxiomataError> {
    let path = outbox_dir().join(format!("{id}.json"));
    let deadline = Instant::now() + timeout;
    loop {
        if path.is_file() {
            let text = read_capped(&path).map_err(|reason| AxiomataError::ModuleAction {
                id: id.to_string(),
                reason,
            })?;
            let _ = fs::remove_file(&path);
            return serde_json::from_str(&text).map_err(|e| AxiomataError::ModuleAction {
                id: id.to_string(),
                reason: format!("unreadable response: {e}"),
            });
        }
        if Instant::now() >= deadline {
            let _ = fs::remove_file(inbox_dir().join(format!("{id}.json")));
            return Err(AxiomataError::ModuleAction {
                id: id.to_string(),
                reason: format!(
                    "no response within {timeout:?} — is the Axiomata-OS dashboard running?"
                ),
            });
        }
        thread::sleep(RESPONSE_POLL_INTERVAL);
    }
}

/// Removes `outbox/` files older than `max_age` (responses nobody collected).
pub fn prune_outbox(max_age: Duration) {
    let Ok(entries) = fs::read_dir(outbox_dir()) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn read_capped(path: &Path) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_QUEUE_FILE_BYTES {
        return Err("file too large".to_string());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Creates the parent, writes a temp file with `O_EXCL`, renames it over
/// `path` (mode 0600 on Unix: the queue may carry file contents).
fn write_atomic(path: &Path, content: &str) -> Result<(), AxiomataError> {
    let io = |p: &Path| {
        let p = p.to_path_buf();
        move |source| AxiomataError::Io { path: p, source }
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io(parent))?;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{file_name}.{}.axiomata-tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(io(&tmp))?;
    file.write_all(content.as_bytes()).map_err(io(&tmp))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    drop(file);
    fs::rename(&tmp, path).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        AxiomataError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    fn with_temp_home(body: impl FnOnce(&Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-bridge");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        body(&home);
        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    fn entry() -> ManifestEntry {
        ManifestEntry {
            instance_id: "inst-1".to_string(),
            module_type: "memory-status".to_string(),
            title: "Memory <!-- x --> AXIOMATA-MODULES:END".to_string(),
            actions: vec![ManifestAction {
                name: "sync".to_string(),
                description: "Regenerate\nthe router".to_string(),
                params: serde_json::json!({"type":"object","properties":{}}),
            }],
        }
    }

    #[test]
    fn manifest_renders_entries_and_neutralises_markers() {
        let text = render_manifest(&[entry()], "axiomata-cli");
        assert!(text.starts_with(MANIFEST_START));
        assert!(text.trim_end().ends_with(MANIFEST_END));
        assert!(text.contains("instance `inst-1`"));
        assert!(text.contains("- `sync` — Regenerate the router"));
        assert!(text.contains("axiomata-cli module-action <instance_id>"));
        // Only the real markers, not the ones smuggled through the title.
        assert_eq!(text.matches(MANIFEST_END).count(), 1);
        assert!(!text.contains("<!-- x -->"));
    }

    #[test]
    fn write_manifest_is_idempotent() {
        with_temp_home(|home| {
            assert!(write_manifest(&[entry()]).unwrap());
            assert!(!write_manifest(&[entry()]).unwrap());
            assert!(home.join("module-context.md").is_file());
            assert!(write_manifest(&[]).unwrap());
            let text = fs::read_to_string(home.join("module-context.md")).unwrap();
            assert!(text.contains("No module instances"));
        });
    }

    #[test]
    fn enqueue_drain_complete_wait_round_trip() {
        with_temp_home(|home| {
            let req = ActionRequest {
                id: new_action_id(),
                instance_id: "inst-1".to_string(),
                action: "sync".to_string(),
                params: serde_json::json!({"a": 1}),
                created_at: Utc::now(),
            };
            let path = enqueue(&req).unwrap();
            assert!(path.starts_with(home.join("module-actions/inbox")));

            let drained = drain_inbox().unwrap();
            assert_eq!(drained, vec![req.clone()]);
            assert!(!path.exists(), "drained requests are removed");
            assert!(drain_inbox().unwrap().is_empty());

            let resp = ActionResponse {
                id: req.id.clone(),
                ok: true,
                result: Some(serde_json::json!({"written": 2})),
                error: None,
                completed_at: Utc::now(),
            };
            complete(&resp).unwrap();
            let got = wait_for_response(&req.id, Duration::from_secs(2)).unwrap();
            assert_eq!(got, resp);
            assert!(
                !home
                    .join(format!("module-actions/outbox/{}.json", req.id))
                    .exists()
            );
        });
    }

    #[test]
    fn waiting_times_out_and_withdraws_the_request() {
        with_temp_home(|home| {
            let req = ActionRequest {
                id: "abc-1".to_string(),
                instance_id: "i".to_string(),
                action: "a".to_string(),
                params: serde_json::Value::Null,
                created_at: Utc::now(),
            };
            let path = enqueue(&req).unwrap();
            let err = wait_for_response("abc-1", Duration::from_millis(300)).unwrap_err();
            assert!(matches!(err, AxiomataError::ModuleAction { .. }), "{err}");
            assert!(!path.exists(), "timed-out request is withdrawn");
            let _ = home;
        });
    }

    #[test]
    fn malformed_ids_and_garbage_files_are_rejected() {
        with_temp_home(|home| {
            let bad = ActionRequest {
                id: "../x".to_string(),
                instance_id: "i".to_string(),
                action: "a".to_string(),
                params: serde_json::Value::Null,
                created_at: Utc::now(),
            };
            assert!(enqueue(&bad).is_err());
            let inbox = home.join("module-actions/inbox");
            fs::create_dir_all(&inbox).unwrap();
            fs::write(inbox.join("garbage.json"), "{ nope").unwrap();
            fs::write(inbox.join("notes.txt"), "ignored").unwrap();
            assert!(drain_inbox().unwrap().is_empty());
            assert!(!inbox.join("garbage.json").exists());
            assert!(inbox.join("notes.txt").exists());
        });
    }
}
