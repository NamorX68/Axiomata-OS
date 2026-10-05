//! What a session is doing right now, read from what its harness leaves behind (`docs/plans/a2a.md`, CP-A9).
//!
//! The Flow's team pane shows each session as a tile with a live line — "edits `src/foo.rs`" — and its last few steps.
//! Neither harness has an API for "what are you doing"; both write a record of every step as they go: Claude Code a JSONL
//! transcript, Opencode its session's messages. This module only **parses** those into [`Activity`] entries, newest last.
//! It is pure: finding the files, asking the service and keeping the cost of a look bounded are the caller's
//! ([`tail_of`] is the one helper that touches a file, and it reads a bounded tail).
//!
//! What comes out is text a model and its tools wrote: shown to the owner on their own machine, never used for a decision
//! and never put in a command line.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// How many steps a tile keeps: the live line and a short trail behind it.
pub const KEEP: usize = 8;
/// The longest detail or sentence kept, in characters. A tile is a tile, not a log.
const MAX_CHARS: usize = 160;
/// How much of a transcript's end one look reads. A step is a line or two, but one big tool result is a line of its own
/// (a file read in full): a megabyte holds the last few steps even then, and bounds the cost of a look however large the
/// file has grown.
const TAIL_BYTES: u64 = 1024 * 1024;

/// One thing a session did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Activity {
    pub kind: ActivityKind,
    /// The tool's name (`Edit`, `Bash`, `report_done`), or empty for something the session said.
    pub name: String,
    /// What the tool was pointed at (a command, a path, a pattern), or the first line of what was said.
    pub detail: String,
    /// When, as the harness wrote it (RFC 3339); `None` where the record has no time.
    pub at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// A tool call.
    Tool,
    /// A line of the model's own words.
    Say,
}

/// The keys a tool's input names its target by, most telling first.
const TARGET_KEYS: [&str; 8] = [
    "command",
    "file_path",
    "filePath",
    "path",
    "pattern",
    "url",
    "query",
    "description",
];

/// The first line of `text`, shortened to [`MAX_CHARS`] characters.
fn one_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(MAX_CHARS).collect();
    if line.chars().count() > MAX_CHARS {
        out.push('…');
    }
    out
}

/// What a tool call was pointed at, out of its input.
fn target_of(input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    TARGET_KEYS
        .iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .map(one_line)
        .unwrap_or_default()
}

/// The tool's name as the owner knows it: the team tools arrive as `mcp__axiomata__report_done`.
fn tool_name(raw: &str) -> String {
    raw.strip_prefix("mcp__axiomata__")
        .unwrap_or(raw)
        .to_owned()
}

/// A time as the service wrote it — an RFC 3339 string, or epoch milliseconds — in RFC 3339, which is what the Studio reads.
fn rfc3339_of(time: &Value) -> Option<String> {
    if let Some(text) = time.as_str() {
        return Some(text.to_owned());
    }
    chrono::DateTime::from_timestamp_millis(time.as_i64()?).map(|at| at.to_rfc3339())
}

fn keep_last(mut steps: Vec<Activity>) -> Vec<Activity> {
    if steps.len() > KEEP {
        steps.drain(..steps.len() - KEEP);
    }
    steps
}

/// The last steps in the text of a Claude Code transcript (JSONL), oldest first. Lines that are not JSON, not a reply, or
/// not a step are skipped: the first line of a tail may be cut in half, and the file is not ours.
pub fn claude_recent(text: &str) -> Vec<Activity> {
    let mut steps = Vec::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let at = entry
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned);
        for block in entry
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => steps.push(Activity {
                    kind: ActivityKind::Tool,
                    name: tool_name(block.get("name").and_then(Value::as_str).unwrap_or("tool")),
                    detail: target_of(block.get("input")),
                    at: at.clone(),
                }),
                Some("text") => {
                    let said = one_line(block.get("text").and_then(Value::as_str).unwrap_or(""));
                    if !said.is_empty() {
                        steps.push(Activity {
                            kind: ActivityKind::Say,
                            name: String::new(),
                            detail: said,
                            at: at.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    keep_last(steps)
}

/// The last steps in an Opencode session's messages, **newest message first** as the service lists them; the result is
/// oldest first. A tool part names its tool as `name` or `tool` and keeps its input at `input` or `state.input`, depending
/// on the service's version.
pub fn opencode_recent(newest_first: &[Value]) -> Vec<Activity> {
    let mut steps = Vec::new();
    for message in newest_first.iter().rev() {
        if message.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let at = message
            .pointer("/time/created")
            .or_else(|| message.get("created"))
            .and_then(rfc3339_of);
        for part in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match part.get("type").and_then(Value::as_str) {
                Some("tool") => {
                    let raw = part
                        .get("name")
                        .or_else(|| part.get("tool"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool");
                    steps.push(Activity {
                        kind: ActivityKind::Tool,
                        name: tool_name(raw),
                        detail: target_of(
                            part.get("input").or_else(|| part.pointer("/state/input")),
                        ),
                        at: at.clone(),
                    });
                }
                Some("text") => {
                    let said = one_line(part.get("text").and_then(Value::as_str).unwrap_or(""));
                    if !said.is_empty() {
                        steps.push(Activity {
                            kind: ActivityKind::Say,
                            name: String::new(),
                            detail: said,
                            at: at.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    keep_last(steps)
}

/// The last [`TAIL_BYTES`] of the plain file at `path`, from the start of a line: a bounded read, however large the file
/// is. A link is not followed.
///
/// # Errors
///
/// Whatever opening or reading says, and [`io::ErrorKind::InvalidInput`] for something that is not a plain file.
pub fn tail_of(path: &Path) -> io::Result<String> {
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a transcript is a plain file",
        ));
    }
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // Cut in the middle of a line: drop the piece, its JSON is not whole.
    Ok(if start > 0 {
        text.split_once('\n')
            .map_or(String::new(), |(_, rest)| rest.to_owned())
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn assistant(blocks: Value, at: &str) -> String {
        json!({"type": "assistant", "timestamp": at, "message": {"content": blocks}}).to_string()
    }

    #[test]
    fn a_claude_transcript_gives_tools_with_their_target_and_the_first_line_said() {
        let text = [
            json!({"type": "user", "message": {"content": "hi"}}).to_string(),
            assistant(
                json!([{"type": "text", "text": "\n  Let me look.\nmore"},
                       {"type": "tool_use", "name": "Edit", "input": {"file_path": "src/a.rs", "old_string": "x"}}]),
                "2026-10-06T10:00:00Z",
            ),
            assistant(
                json!([{"type": "tool_use", "name": "mcp__axiomata__report_done", "input": {"summary": "s"}}]),
                "2026-10-06T10:01:00Z",
            ),
        ]
        .join("\n");
        let steps = claude_recent(&text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].kind, ActivityKind::Say);
        assert_eq!(steps[0].detail, "Let me look.");
        assert_eq!(
            (steps[1].name.as_str(), steps[1].detail.as_str()),
            ("Edit", "src/a.rs")
        );
        assert_eq!(steps[2].name, "report_done");
        assert_eq!(steps[2].at.as_deref(), Some("2026-10-06T10:01:00Z"));
    }

    #[test]
    fn only_the_last_steps_are_kept_and_broken_lines_are_skipped() {
        let mut lines = vec!["{\"type\": \"assis".to_owned(), "not json".to_owned()];
        for n in 0..(KEEP + 5) {
            lines.push(assistant(
                json!([{"type": "tool_use", "name": "Bash", "input": {"command": format!("echo {n}")}}]),
                "2026-10-06T10:00:00Z",
            ));
        }
        let steps = claude_recent(&lines.join("\n"));
        assert_eq!(steps.len(), KEEP);
        assert_eq!(steps.last().unwrap().detail, format!("echo {}", KEEP + 4));
        assert_eq!(steps[0].detail, "echo 5");
    }

    #[test]
    fn a_long_detail_is_cut_to_one_short_line() {
        let long = "x".repeat(500);
        let steps = claude_recent(&assistant(
            json!([{"type": "tool_use", "name": "Bash", "input": {"command": format!("{long}\nsecond")}}]),
            "t",
        ));
        assert_eq!(steps[0].detail.chars().count(), MAX_CHARS + 1);
        assert!(steps[0].detail.ends_with('…'));
    }

    #[test]
    fn opencode_messages_come_newest_first_and_leave_oldest_first_in_either_spelling() {
        let newest_first = vec![
            json!({"type": "assistant", "content": [
                {"type": "tool", "tool": "bash", "state": {"input": {"command": "cargo test"}}}]}),
            json!({"type": "user", "content": [{"type": "text", "text": "go"}]}),
            json!({"type": "assistant", "content": [
                {"type": "text", "text": "Reading."},
                {"type": "tool", "name": "read", "input": {"filePath": "src/b.rs"}}]}),
        ];
        let steps = opencode_recent(&newest_first);
        let seen: Vec<(&str, &str)> = steps
            .iter()
            .map(|s| (s.name.as_str(), s.detail.as_str()))
            .collect();
        assert_eq!(
            seen,
            [
                ("", "Reading."),
                ("read", "src/b.rs"),
                ("bash", "cargo test")
            ]
        );
    }

    #[test]
    fn opencode_times_in_epoch_milliseconds_become_rfc_3339() {
        let messages = vec![
            json!({"type": "assistant", "time": {"created": 1_728_212_345_000_i64}, "content": [
                {"type": "tool", "name": "bash", "input": {"command": "ls"}}]}),
            json!({"type": "assistant", "created": "2026-10-06T10:00:00Z", "content": [
                {"type": "text", "text": "hi"}]}),
        ];
        let steps = opencode_recent(&messages);
        assert_eq!(steps[0].at.as_deref(), Some("2026-10-06T10:00:00Z"));
        assert_eq!(steps[1].at.as_deref(), Some("2024-10-06T10:59:05+00:00"));
    }

    #[test]
    fn a_big_last_line_still_leaves_the_steps_before_it_in_the_tail() {
        let dir =
            std::env::temp_dir().join(format!("axiomata-activity-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.jsonl");
        let step = assistant(
            json!([{"type": "tool_use", "name": "Bash", "input": {"command": "cargo test"}}]),
            "2026-10-06T10:00:00Z",
        );
        // A tool result of a file read in full: hundreds of kilobytes on one line.
        let big =
            json!({"type": "user", "message": {"content": "x".repeat(400 * 1024)}}).to_string();
        std::fs::write(&file, format!("{step}\n{big}\n")).unwrap();
        let steps = claude_recent(&tail_of(&file).unwrap());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].detail, "cargo test");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tail_of_a_file_starts_at_a_whole_line_and_a_link_is_refused() {
        let dir = std::env::temp_dir().join(format!("axiomata-activity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.jsonl");
        let line = format!("{}\n", "a".repeat(1000));
        let many = line.repeat(400);
        std::fs::write(&file, &many).unwrap();
        let tail = tail_of(&file).unwrap();
        assert!(tail.len() as u64 <= TAIL_BYTES);
        assert!(
            tail.lines().all(|l| l.len() == 1000),
            "no half line at the start"
        );
        std::fs::write(dir.join("small"), "x\n").unwrap();
        assert_eq!(tail_of(&dir.join("small")).unwrap(), "x\n");
        #[cfg(unix)]
        {
            let link = dir.join("link");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            assert!(tail_of(&link).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
