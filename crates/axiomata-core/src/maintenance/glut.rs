//! Glut's row in Settings → Storage (`docs/plans/maintenance.md` §5, A5).
//!
//! Glut cleans `~/.glut` itself (`glut clean`, and once a day on its own). Axiomata only asks it what it would free
//! (`glut clean --dry-run --json`) and, on "Clean up now", lets it clean — it never reads or deletes anything in
//! `~/.glut` on its own. No Glut, or an older one without `clean`, gives no row; a Glut whose clean-up fails says why
//! in one stderr line (`glut clean: …`), which the row shows; an answer in another format version shows "Glut: unknown
//! format".

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The one format of `glut clean --json` this Axiomata reads.
pub const GLUT_FORMAT_VERSION: u64 = 1;

/// How long the measuring call may take before it is stopped: it only walks `~/.glut`.
const DRY_RUN_TIMEOUT: Duration = Duration::from_secs(15);

/// How long Glut's own clean-up may take.
const CLEAN_TIMEOUT: Duration = Duration::from_secs(120);

/// More than this on stdout is no report of Glut's.
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

/// Where a `glut` installed by hand or by a package manager lies, beside the `PATH` (the app adopts the login shell's
/// `PATH` at start, but a Finder-started app may still miss one of these).
const USUAL_DIRS: [&str; 3] = ["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"];

/// Glut's answer, format version 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlutReport {
    pub version: u64,
    pub dry_run: bool,
    pub total_bytes: u64,
    pub freeable_bytes: u64,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub items: Vec<GlutItem>,
}

/// One of Glut's items, as Glut names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlutItem {
    pub name: String,
    pub bytes: u64,
    pub freeable_bytes: u64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub freeable_count: u64,
    /// Glut's rule in its own words (`7d`); absent for an item that is not kept by age.
    #[serde(default, deserialize_with = "keep_text")]
    pub keep: Option<String>,
}

/// What Settings → Storage shows for Glut.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GlutRow {
    /// Glut answered in the format this Axiomata reads.
    Known { report: GlutReport },
    /// Glut answered, but in another format: the row says "Glut: unknown format".
    UnknownFormat,
    /// Glut has `clean` but it failed; `message` is its one line (`glut clean: another Glut is cleaning`).
    Failed { message: String },
}

/// The prefix of the one stderr line a failing `glut clean` writes — what tells its own failure apart from an older
/// Glut that does not know the command at all.
const FAILURE_PREFIX: &str = "glut clean:";

/// The longest failure line shown.
const MAX_FAILURE_CHARS: usize = 300;

/// How a run of `glut` ended.
#[derive(Debug, PartialEq, Eq)]
enum Ran {
    /// Exit 0, with its stdout.
    Answered(String),
    /// A failure of `glut clean` itself, with its line.
    Failed(String),
    /// Anything else: not started, killed, or an error that is not `glut clean`'s.
    Nothing,
}

/// Any JSON value as the text shown for it: a string as it is, `null` as none, anything else as JSON.
fn keep_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(text) => Some(text),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    })
}

/// Reads Glut's answer: `None` for no JSON object at all (then there is no row), [`GlutRow::UnknownFormat`] for another
/// version or a version-1 answer that does not have the fields of version 1.
pub fn parse(stdout: &str) -> Option<GlutRow> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    if !value.is_object() {
        return None;
    }
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(GLUT_FORMAT_VERSION) {
        return Some(GlutRow::UnknownFormat);
    }
    Some(match serde_json::from_value::<GlutReport>(value) {
        Ok(report) => GlutRow::Known { report },
        Err(_) => GlutRow::UnknownFormat,
    })
}

/// The `glut` program: the first on `PATH`, else in one of [`USUAL_DIRS`] or `~/.cargo/bin`.
fn find_glut() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let cargo = home::home_dir().map(|home| home.join(".cargo").join("bin"));
    on_path
        .into_iter()
        .chain(USUAL_DIRS.iter().map(PathBuf::from))
        .chain(cargo)
        .map(|dir| dir.join("glut"))
        .find(|candidate| candidate.is_file())
}

/// Runs `program args…` without a terminal or input, for at most `timeout` (a run that outlives it is killed).
fn run(program: &Path, args: &[&str], timeout: Duration) -> Ran {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return Ran::Nothing;
    };
    // Read on the side, so a large answer cannot fill a pipe and stall the program while this waits for it.
    let collect = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            pipe.map(|pipe| pipe.take(MAX_OUTPUT_BYTES).read_to_string(&mut text));
            text
        })
    };
    let stdout = collect(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = collect(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ran::Nothing;
            }
        }
    };
    let (Ok(stdout), Ok(stderr)) = (stdout.join(), stderr.join()) else {
        return Ran::Nothing;
    };
    if status.success() {
        return Ran::Answered(stdout);
    }
    match stderr
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(FAILURE_PREFIX))
    {
        Some(line) => Ran::Failed(line.chars().take(MAX_FAILURE_CHARS).collect()),
        None => Ran::Nothing,
    }
}

/// Glut's row from one run of `glut clean … --json`.
fn row(ran: Ran) -> Option<GlutRow> {
    match ran {
        Ran::Answered(stdout) => parse(&stdout),
        Ran::Failed(message) => Some(GlutRow::Failed { message }),
        Ran::Nothing => None,
    }
}

/// What Glut would free, or `None` when there is no Glut that can say.
pub fn report() -> Option<GlutRow> {
    let glut = find_glut()?;
    row(run(
        &glut,
        &["clean", "--dry-run", "--json"],
        DRY_RUN_TIMEOUT,
    ))
}

/// Lets Glut clean `~/.glut` and returns what it says it freed.
///
/// Only after a measuring call answered in format version 1: that proves this Glut has a `clean` command, so an older
/// one is never handed words it would read as something else (a first message of a new session, say).
///
/// A clean-up that started but gave no answer (it outlived its time, say) is a [`GlutRow::Failed`], never no row: the
/// owner asked for it and should hear that it did not finish.
pub fn clean() -> Option<GlutRow> {
    clean_with(&find_glut()?, CLEAN_TIMEOUT)
}

fn clean_with(glut: &Path, timeout: Duration) -> Option<GlutRow> {
    let measured = row(run(
        glut,
        &["clean", "--dry-run", "--json"],
        DRY_RUN_TIMEOUT,
    ))?;
    if !matches!(measured, GlutRow::Known { .. }) {
        return Some(measured);
    }
    Some(
        row(run(glut, &["clean", "--json"], timeout)).unwrap_or_else(|| GlutRow::Failed {
            message: "glut clean: gave no answer".to_owned(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_one_answer_is_read_and_anything_else_is_unknown_or_nothing() {
        let v1 = r#"{"version":1,"dryRun":true,"totalBytes":300,"freeableBytes":100,"lastRun":null,
            "items":[{"name":"snapshots","bytes":300,"freeableBytes":100,"count":3,"freeableCount":1,"keep":"7d"},
                     {"name":"images","bytes":0,"freeableBytes":0,"count":0,"freeableCount":0},
                     {"name":"someday","bytes":1,"freeableBytes":0,"count":1,"freeableCount":0,"keep":14}]}"#;
        let Some(GlutRow::Known { report }) = parse(v1) else {
            panic!("version 1 should be read");
        };
        assert_eq!(report.freeable_bytes, 100);
        assert_eq!(report.items[0].keep.as_deref(), Some("7d"));
        assert_eq!(report.items[1].keep, None, "not kept by age");
        assert_eq!(
            report.items[2].keep.as_deref(),
            Some("14"),
            "an unknown item is kept as Glut names it"
        );

        assert_eq!(
            parse(r#"{"version":2,"anything":true}"#),
            Some(GlutRow::UnknownFormat)
        );
        assert_eq!(
            parse(r#"{"version":1,"items":"nope"}"#),
            Some(GlutRow::UnknownFormat)
        );
        assert_eq!(parse(r#"{"items":[]}"#), Some(GlutRow::UnknownFormat));
        assert_eq!(parse("[1,2]"), None);
        assert_eq!(parse("not json"), None);
    }

    /// A stand-in `glut` that answers the dry run with `dry_run_answer` and writes `marker` when it is asked to clean.
    fn fake_glut(dir: &Path, dry_run_answer: &str, marker: &Path) -> PathBuf {
        let program = dir.join("glut");
        let script = format!(
            "#!/bin/sh\nif [ \"$2\" = \"--dry-run\" ]; then echo '{dry_run_answer}'; exit 0; fi\n\
             touch '{}'\necho '{{\"version\":1,\"dryRun\":false,\"totalBytes\":5,\"freeableBytes\":3,\"items\":[]}}'\n",
            marker.display()
        );
        std::fs::write(&program, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    #[test]
    fn glut_cleans_only_after_a_dry_run_in_the_known_format() {
        let dir = crate::test_support::unique_temp_dir("axiomata-test-glut-gate");
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("cleaned");

        let unknown = fake_glut(&dir, r#"{"version":2}"#, &marker);
        assert_eq!(
            clean_with(&unknown, CLEAN_TIMEOUT),
            Some(GlutRow::UnknownFormat)
        );
        assert!(
            !marker.exists(),
            "another format never gets the real clean-up"
        );

        let known = fake_glut(
            &dir,
            r#"{"version":1,"dryRun":true,"totalBytes":5,"freeableBytes":3,"items":[]}"#,
            &marker,
        );
        let Some(GlutRow::Known { report }) = clean_with(&known, CLEAN_TIMEOUT) else {
            panic!("the clean-up's answer");
        };
        assert!(!report.dry_run);
        assert!(marker.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glut_s_own_failure_is_shown_and_anything_else_gives_no_row() {
        let sh = Path::new("/bin/sh");
        assert_eq!(
            row(run(
                sh,
                &[
                    "-c",
                    "echo 'glut clean: another Glut is cleaning' >&2; exit 1"
                ],
                DRY_RUN_TIMEOUT
            )),
            Some(GlutRow::Failed {
                message: "glut clean: another Glut is cleaning".to_owned()
            })
        );
        // An older Glut that does not know `clean`: clap's error, no row.
        assert_eq!(
            row(run(
                sh,
                &[
                    "-c",
                    "echo \"error: unexpected argument '--dry-run'\" >&2; exit 2"
                ],
                DRY_RUN_TIMEOUT
            )),
            None
        );
        let started = Instant::now();
        assert_eq!(
            run(sh, &["-c", "sleep 5"], Duration::from_millis(200)),
            Ran::Nothing
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the hanging program is killed"
        );
        assert_eq!(
            run(sh, &["-c", "echo '{\"version\":1}'"], DRY_RUN_TIMEOUT),
            Ran::Answered("{\"version\":1}\n".to_owned())
        );
    }
}
