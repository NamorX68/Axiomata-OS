//! A `git diff` for one file, parsed once so no UI has to (`docs/plans/git-layer.md`, H1–H16),
//! and the patch of one hunk rebuilt for `git apply`.
//!
//! Shared by the IDE (an agent's changes against its base) and the editor's git panel (the
//! project folder against its index or `HEAD`).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{GitError, Result};

/// The most diff output read for one file. Beyond it the diff is cut off and
/// marked `truncated` — a generated lock file is not something to scroll.
pub const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
/// The most diff lines returned for one file.
pub const MAX_DIFF_LINES: usize = 10_000;

/// The kind of one line in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Unchanged, shown for orientation only.
    Context,
    /// Present on the agent's side, not on the base's.
    Add,
    /// Present on the base's side, not on the agent's.
    Remove,
    /// `\ No newline at end of file`, attached to the line above it.
    NoNewline,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    /// Context, an addition, a removal, or a no-newline marker.
    pub kind: LineKind,
    /// Line number on the base side; `None` for an added line.
    pub old_line: Option<u32>,
    /// Line number on the agent's side; `None` for a removed line.
    pub new_line: Option<u32>,
    /// Without the leading `+`/`-`/space.
    pub text: String,
}

/// One `@@` hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// The whole `@@ -a,b +c,d @@ context` line.
    pub header: String,
    /// The hunk's lines, in the order git printed them.
    pub lines: Vec<DiffLine>,
}

/// One file's diff, parsed (CP7) so the UI never has to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// Relative to the worktree root, `/`-separated.
    pub path: String,
    /// True when git reported this as binary content; `hunks` is then empty.
    pub binary: bool,
    /// Empty for a binary file, a pure rename, or a file too large to diff.
    pub hunks: Vec<Hunk>,
    /// Cut off at [`MAX_DIFF_BYTES`] / [`MAX_DIFF_LINES`].
    pub truncated: bool,
    /// Bytes on the base side; `None` when the base has no such file.
    pub old_size: Option<u64>,
    /// Bytes in the worktree; `None` when the file is gone. Together with
    /// `old_size` what a binary file's diff shows (H8).
    pub new_size: Option<u64>,
}

/// A patch of one hunk of `path`, as `git apply` reads it. The names are
/// always C-quoted, so no file name — spaces, quotes, a leading dash — can
/// be misread as something else.
pub fn hunk_patch(path: &str, hunk: &Hunk) -> String {
    let mut patch = format!(
        "--- {}\n+++ {}\n{}\n",
        quote_c(&format!("a/{path}")),
        quote_c(&format!("b/{path}")),
        hunk.header
    );
    for line in &hunk.lines {
        let prefix = match line.kind {
            LineKind::Context => " ",
            LineKind::Add => "+",
            LineKind::Remove => "-",
            LineKind::NoNewline => "",
        };
        patch.push_str(prefix);
        patch.push_str(&line.text);
        patch.push('\n');
    }
    patch
}

/// `name` in git's C-style quotes: `"` and `\` escaped, `\n`/`\t` as git's own
/// letter shortcuts, any other control character as octal. Other characters
/// (UTF-8 included) are taken as they are.
fn quote_c(name: &str) -> String {
    let mut out = String::from("\"");
    for c in name.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("\\{byte:03o}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Checks a path that came from outside: relative, no `..`, not empty.
pub fn checked_path(path: &str) -> Result<PathBuf> {
    let refuse = |why: &str| GitError::Invalid {
        field: "path",
        reason: format!("{path:?} {why}"),
    };
    if path.is_empty() || path.contains('\0') {
        return Err(refuse("is not a usable path"));
    }
    let candidate = Path::new(path);
    let mut clean = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err(refuse("must be relative to the worktree, without `..`")),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(refuse("is not a usable path"));
    }
    Ok(clean)
}

/// Parses `git diff` output for one file into hunks.
pub fn parse_diff(path: &str, raw: &str) -> FileDiff {
    let mut diff = FileDiff {
        path: path.to_string(),
        binary: false,
        hunks: Vec::new(),
        truncated: false,
        old_size: None,
        new_size: None,
    };
    let raw = if raw.len() > MAX_DIFF_BYTES {
        diff.truncated = true;
        // Cut at a line boundary at or before the limit.
        let cut = raw[..raw.floor_char_boundary(MAX_DIFF_BYTES)]
            .rfind('\n')
            .unwrap_or(0);
        &raw[..cut]
    } else {
        raw
    };

    let (mut old_line, mut new_line) = (0u32, 0u32);
    let mut lines_seen = 0usize;
    // Split on `\n` only, not `lines()`: a CRLF file's `\r` stays part of the
    // line, so a hunk rebuilt into a patch (H6) still matches the file.
    for line in raw.strip_suffix('\n').unwrap_or(raw).split('\n') {
        if line.starts_with("Binary files ") || line == "GIT binary patch" {
            diff.binary = true;
            continue;
        }
        if let Some((old_start, new_start)) = parse_hunk_header(line) {
            old_line = old_start;
            new_line = new_start;
            diff.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = diff.hunks.last_mut() else {
            // Still in the file header (`diff --git`, `index`, `---`, `+++`).
            continue;
        };
        if lines_seen >= MAX_DIFF_LINES {
            diff.truncated = true;
            break;
        }
        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Add, &line[1..]),
            Some(b'-') => (LineKind::Remove, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            Some(b'\\') => (LineKind::NoNewline, line),
            // An empty context line whose leading space an editor stripped.
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let (old, new) = match kind {
            LineKind::Add => {
                new_line += 1;
                (None, Some(new_line))
            }
            LineKind::Remove => {
                old_line += 1;
                (Some(old_line), None)
            }
            LineKind::Context => {
                old_line += 1;
                new_line += 1;
                (Some(old_line), Some(new_line))
            }
            LineKind::NoNewline => (None, None),
        };
        hunk.lines.push(DiffLine {
            kind,
            old_line: old,
            new_line: new,
            text: text.to_string(),
        });
        lines_seen += 1;
    }
    diff
}

/// `@@ -12,7 +12,9 @@ …` → the line *before* each side's first line, so the
/// caller can increment before every line it counts.
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let start = |range: &str| -> Option<u32> {
        let first: u32 = range.split(',').next()?.parse().ok()?;
        Some(first.saturating_sub(1))
    };
    Some((start(old)?, start(new)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_c_escapes_what_git_would() {
        assert_eq!(quote_c("a/plain.rs"), "\"a/plain.rs\"");
        assert_eq!(quote_c("a/\"q\"\\x\ty"), "\"a/\\\"q\\\"\\\\x\\ty\"");
        assert_eq!(quote_c("a/\u{1}"), "\"a/\\001\"");
    }

    #[test]
    fn the_parser_marks_a_cut_off_diff() {
        let body: String = (0..MAX_DIFF_LINES + 10)
            .map(|i| format!("+line {i}\n"))
            .collect();
        let raw = format!(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -0,0 +1,{} @@\n{body}",
            MAX_DIFF_LINES + 10
        );
        let diff = parse_diff("x", &raw);
        assert!(diff.truncated);
        assert_eq!(diff.hunks[0].lines.len(), MAX_DIFF_LINES);
    }

    #[test]
    fn the_parser_reads_a_hunk_header_without_explicit_counts() {
        // `@@ -1 +1 @@` — git omits the count when it is 1.
        let raw = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
        let diff = parse_diff("x", raw);
        assert_eq!(diff.hunks.len(), 1);
        let lines: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Remove, Some(1), None),
                (LineKind::Add, None, Some(1)),
            ]
        );
    }

    #[test]
    fn the_parser_handles_a_no_newline_marker_mid_hunk_and_several_hunks() {
        let raw = "diff --git a/x b/x\n--- a/x\n+++ b/x\n\
                    @@ -1,2 +1,2 @@\n-old1\n\\ No newline at end of file\n+new1\n context\n\
                    @@ -10,1 +10,2 @@\n context2\n+added2\n";
        let diff = parse_diff("x", raw);
        assert_eq!(diff.hunks.len(), 2);

        let first: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            first,
            [
                (LineKind::Remove, Some(1), None),
                (LineKind::NoNewline, None, None),
                (LineKind::Add, None, Some(1)),
                (LineKind::Context, Some(2), Some(2)),
            ]
        );

        let second: Vec<_> = diff.hunks[1]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            second,
            [
                (LineKind::Context, Some(10), Some(10)),
                (LineKind::Add, None, Some(11)),
            ]
        );
    }

    #[test]
    fn hunk_patch_builds_a_c_quoted_single_hunk_patch() {
        let hunk = Hunk {
            header: "@@ -1,2 +1,2 @@".to_string(),
            lines: vec![
                DiffLine {
                    kind: LineKind::Context,
                    old_line: Some(1),
                    new_line: Some(1),
                    text: "one".to_string(),
                },
                DiffLine {
                    kind: LineKind::Remove,
                    old_line: Some(2),
                    new_line: None,
                    text: "two".to_string(),
                },
                DiffLine {
                    kind: LineKind::Add,
                    old_line: None,
                    new_line: Some(2),
                    text: "TWO".to_string(),
                },
            ],
        };
        assert_eq!(
            hunk_patch("a \"weird\" name.txt", &hunk),
            "--- \"a/a \\\"weird\\\" name.txt\"\n\
             +++ \"b/a \\\"weird\\\" name.txt\"\n\
             @@ -1,2 +1,2 @@\n \
             one\n\
             -two\n\
             +TWO\n"
        );
    }

    #[test]
    fn parse_diff_keeps_the_carriage_return_in_a_crlf_lines_text() {
        let raw =
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\r\n one\r\n-two\r\n+TWO\r\n";
        let diff = parse_diff("x", raw);
        assert_eq!(diff.hunks.len(), 1);
        let texts: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(
            texts,
            ["one\r", "two\r", "TWO\r"],
            "the trailing \\r must stay part of the line's text, not be stripped"
        );
    }
}
