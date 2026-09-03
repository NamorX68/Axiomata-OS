//! Importing notes from other tools into the Second-Brain workspace.
//!
//! First source: an **Obsidian** vault folder. Each note is normalised to
//! "one `#` title line + the content": YAML frontmatter and a leading
//! `#tag #tag` line are removed (their tags survive only as *hints* for the
//! sorting step), the title comes from the note's first `#` heading or its
//! file name. Sorting into top-level workspace areas is not done by rules
//! here — the agent proposes the areas and the assignment from a compact
//! listing (title, tags, excerpt) and answers in JSON; [`apply`] writes the
//! files under `<workspace_root>/<Area>/<name>.md`, never overwriting.
//! Notes that look like they hold secrets are flagged (and can be skipped).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;

/// Area used for notes the agent did not assign (or assigned to an unknown
/// area).
pub const FALLBACK_AREA: &str = "Inbox";
/// Notes larger than this are skipped (they are not notes).
pub const MAX_NOTE_BYTES: u64 = 1024 * 1024;
/// Characters of body text shown to the agent per note.
pub const EXCERPT_CHARS: usize = 240;

/// One source note after normalisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceNote {
    /// Original file name (with `.md`), the key the agent answers with.
    pub file_name: String,
    pub title: String,
    pub tags: Vec<String>,
    /// Content without frontmatter / tag line, without the title heading.
    pub body: String,
    pub secret_like: bool,
}

impl SourceNote {
    /// The note as written into the workspace.
    pub fn markdown(&self) -> String {
        if self.body.trim().is_empty() {
            format!("# {}\n", self.title)
        } else {
            format!("# {}\n\n{}\n", self.title, self.body.trim())
        }
    }

    /// First [`EXCERPT_CHARS`] characters of the body, whitespace collapsed.
    pub fn excerpt(&self) -> String {
        let collapsed = self.body.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut out: String = collapsed.chars().take(EXCERPT_CHARS).collect();
        if collapsed.chars().count() > EXCERPT_CHARS {
            out.push('…');
        }
        out
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> AxiomataError {
    let path = path.to_path_buf();
    move |source| AxiomataError::Io { path, source }
}

/// Reads every `*.md` under `src` (recursively, hidden folders skipped) and
/// normalises it. Sorted by file name for a stable listing.
pub fn scan_obsidian(src: &Path) -> Result<Vec<SourceNote>, AxiomataError> {
    let mut files = Vec::new();
    collect_markdown(src, &mut files)?;
    files.sort();
    let mut notes = Vec::with_capacity(files.len());
    for path in files {
        let meta = fs::metadata(&path).map_err(io(&path))?;
        if meta.len() > MAX_NOTE_BYTES {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(io(&path))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        notes.push(normalise(&file_name, &raw));
    }
    Ok(notes)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), AxiomataError> {
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let entry = entry.map_err(io(dir))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().map_err(io(&path))?;
        if file_type.is_dir() {
            collect_markdown(&path, out)?;
        } else if file_type.is_file() && name.to_lowercase().ends_with(".md") {
            out.push(path);
        }
    }
    Ok(())
}

/// Pure normalisation of one note's raw text.
pub fn normalise(file_name: &str, raw: &str) -> SourceNote {
    let mut tags: BTreeSet<String> = BTreeSet::new();
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let (front, rest) = split_frontmatter(text);
    if let Some(front) = front {
        tags.extend(frontmatter_tags(front));
    }
    let mut lines: Vec<&str> = rest.lines().collect();
    // Drop leading blank lines and a leading `#tag #tag` line.
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    if let Some(first) = lines.first()
        && let Some(line_tags) = tag_line(first)
    {
        tags.extend(line_tags);
        lines.remove(0);
        while lines.first().is_some_and(|l| l.trim().is_empty()) {
            lines.remove(0);
        }
    }
    // Title: a leading `# Heading`, else the file stem.
    let stem = file_name.strip_suffix(".md").unwrap_or(file_name).trim();
    let title = match lines.first().and_then(|l| l.strip_prefix("# ")) {
        Some(h) if !h.trim().is_empty() => {
            let t = h.trim().to_string();
            lines.remove(0);
            t
        }
        _ => stem.to_string(),
    };
    let body = lines.join("\n").trim().to_string();
    let secret_like = looks_secret(file_name, &body);
    SourceNote {
        file_name: file_name.to_string(),
        title,
        tags: tags.into_iter().collect(),
        body,
        secret_like,
    }
}

/// Splits `---\n…\n---` off the top. Returns (frontmatter, rest).
fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let Some(after) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut offset = 0;
    for line in after.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let front = &after[..offset];
            let rest = &after[offset + line.len()..];
            return (Some(front), rest);
        }
        offset += line.len();
    }
    (None, text)
}

/// `tags:` from YAML frontmatter — the `- x` list form or `[a, b]` inline.
fn frontmatter_tags(front: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut in_list = false;
    for line in front.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("tags:") {
            let value = value.trim();
            in_list = value.is_empty();
            if let Some(inline) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                tags.extend(inline.split(',').map(clean_tag).filter(|t| !t.is_empty()));
            }
            continue;
        }
        if in_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                let t = clean_tag(item);
                if !t.is_empty() {
                    tags.push(t);
                }
            } else if !trimmed.is_empty() {
                in_list = false;
            }
        }
    }
    tags
}

/// A line made only of `#tag` tokens (Obsidian inline tags).
fn tag_line(line: &str) -> Option<Vec<String>> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() || !tokens.iter().all(|t| t.starts_with('#') && t.len() > 1) {
        return None;
    }
    Some(tokens.iter().map(|t| clean_tag(t)).collect())
}

fn clean_tag(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('#')
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

/// Heuristic: file name mentions keys/secrets, or the body carries a
/// well-known credential shape.
pub fn looks_secret(file_name: &str, body: &str) -> bool {
    let name = file_name.to_lowercase();
    if [
        "api-key",
        "api key",
        "apikey",
        "secret",
        "passwor",
        "token",
        "credential",
    ]
    .iter()
    .any(|k| name.contains(k))
    {
        return true;
    }
    let patterns = [
        "sk-",
        "ghp_",
        "AKIA",
        "BEGIN RSA PRIVATE",
        "BEGIN OPENSSH PRIVATE",
        "xoxb-",
    ];
    body.split_whitespace().any(|w| {
        patterns
            .iter()
            .any(|p| w.contains(p) && w.len() >= p.len() + 12)
    })
}

// ------------------------------------------------------------- the agent

/// One proposed area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// The agent's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub areas: Vec<Area>,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub file: String,
    pub area: String,
}

/// The sorting request handed to the agent (`agents::chat`).
pub fn assignment_prompt(notes: &[SourceNote], existing_areas: &[String]) -> String {
    let mut p = String::new();
    p.push_str(
        "You are organising a personal Second-Brain workspace. Below is a list of notes \
         (file name, title, tags, excerpt). Group them into a small number of top-level \
         AREAS (folders): between 3 and 8, each a short folder-safe name in the language \
         the notes are written in (letters, digits, spaces, hyphens only; e.g. \"Entwicklung\", \
         \"Fotografie\", \"Arbeit\"). Prefer broad, durable areas over one-off topics. Every \
         note gets exactly one area.\n\n",
    );
    if !existing_areas.is_empty() {
        p.push_str("Areas that already exist in the workspace (reuse them when they fit): ");
        p.push_str(&existing_areas.join(", "));
        p.push_str("\n\n");
    }
    p.push_str(
        "Answer with ONLY a JSON object, no prose, no code fences, of this exact shape:\n\
         {\"areas\":[{\"name\":\"…\",\"description\":\"one line\"}],\
         \"assignments\":[{\"file\":\"<file name exactly as listed>\",\"area\":\"<area name>\"}]}\n\n\
         NOTES:\n",
    );
    for (i, n) in notes.iter().enumerate() {
        p.push_str(&format!(
            "{}. file: {}\n   title: {}\n   tags: {}\n   excerpt: {}\n",
            i + 1,
            n.file_name,
            n.title,
            if n.tags.is_empty() {
                "-".to_string()
            } else {
                n.tags.join(", ")
            },
            n.excerpt()
        ));
    }
    p
}

/// Parses the agent's reply, tolerating surrounding prose or code fences.
pub fn parse_plan(reply: &str) -> Result<Plan, AxiomataError> {
    let start = reply.find('{');
    let end = reply.rfind('}');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &reply[s..=e],
        _ => {
            return Err(AxiomataError::AgentApi {
                backend: "claude-code",
                message: "the sorting reply contained no JSON object".to_string(),
            });
        }
    };
    serde_json::from_str(json).map_err(|e| AxiomataError::AgentApi {
        backend: "claude-code",
        message: format!("could not parse the sorting reply: {e}"),
    })
}

// -------------------------------------------------------------- applying

/// What [`apply`] did.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportReport {
    /// `(area, file name)` pairs written.
    pub written: Vec<(String, String)>,
    pub skipped_existing: Vec<String>,
    pub skipped_secret: Vec<String>,
    /// Notes the agent left out or sent to an unknown area → [`FALLBACK_AREA`].
    pub fell_back: Vec<String>,
}

/// Folder-safe area name: letters, digits, spaces, hyphens; else fallback.
pub fn sanitize_area(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() || cleaned.starts_with('.') {
        FALLBACK_AREA.to_string()
    } else {
        cleaned.chars().take(60).collect()
    }
}

/// File-safe note name (keeps umlauts and spaces, strips separators).
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0'))
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "note.md".to_string()
    } else {
        cleaned
    }
}

/// Writes the notes into `workspace_root/<area>/<file>`. Existing files are
/// never overwritten; secret-like notes are skipped unless `include_secrets`.
/// With `dry_run` nothing is written but the report is complete.
pub fn apply(
    notes: &[SourceNote],
    plan: &Plan,
    workspace_root: &Path,
    include_secrets: bool,
    dry_run: bool,
) -> Result<ImportReport, AxiomataError> {
    let known: BTreeSet<String> = plan.areas.iter().map(|a| sanitize_area(&a.name)).collect();
    let mut report = ImportReport::default();
    for note in notes {
        if note.secret_like && !include_secrets {
            report.skipped_secret.push(note.file_name.clone());
            continue;
        }
        let assigned = plan
            .assignments
            .iter()
            .find(|a| a.file == note.file_name)
            .map(|a| sanitize_area(&a.area));
        // An assignment counts only if it names a proposed area (or the
        // agent proposed none at all); everything else lands in the fallback.
        let area = match assigned {
            Some(a) if known.contains(&a) || known.is_empty() => a,
            _ => {
                report.fell_back.push(note.file_name.clone());
                FALLBACK_AREA.to_string()
            }
        };
        let file_name = sanitize_file_name(&note.file_name);
        let target = workspace_root.join(&area).join(&file_name);
        if target.exists() {
            report.skipped_existing.push(note.file_name.clone());
            continue;
        }
        if !dry_run {
            let dir = workspace_root.join(&area);
            fs::create_dir_all(&dir).map_err(io(&dir))?;
            fs::write(&target, note.markdown()).map_err(io(&target))?;
        }
        report.written.push((area, file_name));
    }
    Ok(report)
}

/// Existing top-level folders of the workspace (areas), for the prompt.
pub fn existing_areas(workspace_root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(workspace_root) else {
        return Vec::new();
    };
    let mut areas: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    areas.sort();
    areas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn strips_frontmatter_and_tag_line_and_derives_title() {
        let raw = "---\nid: x\ntags:\n  - Rust\n  - CheatSheet\n---\n#Git #Development \n\nSome text\n\n## Sub\n- a\n";
        let n = normalise("Cargo Cheat Sheet.md", raw);
        assert_eq!(n.title, "Cargo Cheat Sheet");
        assert_eq!(n.tags, vec!["CheatSheet", "Development", "Git", "Rust"]);
        assert_eq!(n.body, "Some text\n\n## Sub\n- a");
        assert_eq!(
            n.markdown(),
            "# Cargo Cheat Sheet\n\nSome text\n\n## Sub\n- a\n"
        );
        assert!(!n.secret_like);
    }

    #[test]
    fn keeps_an_existing_h1_as_title_and_handles_inline_tag_lists() {
        let n = normalise("x.md", "---\ntags: [a, b]\n---\n# Real Title\n\nbody");
        assert_eq!(n.title, "Real Title");
        assert_eq!(n.tags, vec!["a", "b"]);
        assert_eq!(n.body, "body");
        let empty = normalise("Empty.md", "");
        assert_eq!(empty.markdown(), "# Empty\n");
        let hash_heading = normalise("h.md", "## not a tag line\ntext");
        assert!(hash_heading.tags.is_empty());
        assert_eq!(hash_heading.title, "h");
    }

    #[test]
    fn detects_secret_like_notes() {
        assert!(normalise("OpenAI-API-Keys.md", "sk-abcdefghijklmnopqrstuvwxyz").secret_like);
        assert!(normalise("notes.md", "token sk-abcdefghijklmnopqrstuvwxyz0123").secret_like);
        assert!(!normalise("notes.md", "the sk- prefix is documented").secret_like);
    }

    #[test]
    fn excerpt_is_collapsed_and_capped() {
        let n = normalise("x.md", &format!("{}\n", "word ".repeat(200)));
        let e = n.excerpt();
        assert!(e.ends_with('…'));
        assert_eq!(e.chars().count(), EXCERPT_CHARS + 1);
        assert!(!e.contains('\n'));
    }

    #[test]
    fn prompt_lists_notes_and_existing_areas() {
        let n = normalise("a.md", "#Tag\nhello");
        let p = assignment_prompt(&[n], &["Work".to_string()]);
        assert!(p.contains("1. file: a.md"));
        assert!(p.contains("tags: Tag"));
        assert!(p.contains("already exist in the workspace (reuse them when they fit): Work"));
        assert!(p.contains("ONLY a JSON object"));
    }

    #[test]
    fn parses_plans_with_or_without_fences() {
        let plain = r#"{"areas":[{"name":"Dev","description":"d"}],"assignments":[{"file":"a.md","area":"Dev"}]}"#;
        assert_eq!(parse_plan(plain).unwrap().assignments.len(), 1);
        let fenced = format!("Here you go:\n```json\n{plain}\n```\n");
        assert_eq!(parse_plan(&fenced).unwrap().areas[0].name, "Dev");
        assert!(parse_plan("no json here").is_err());
        assert!(parse_plan("{\"areas\": 1}").is_err());
    }

    #[test]
    fn apply_writes_never_overwrites_skips_secrets_and_falls_back() {
        let root = unique_temp_dir("axiomata-test-import");
        fs::create_dir_all(root.join("Dev")).unwrap();
        fs::write(root.join("Dev/old.md"), "keep me").unwrap();
        let notes = vec![
            normalise("a.md", "#Rust\nbody a"),
            normalise("old.md", "new content"),
            normalise("OpenAI-API-Keys.md", "sk-abcdefghijklmnopqrstuvwxyz"),
            normalise("lost.md", "nobody assigned me"),
            normalise("weird/name:.md", "x"),
        ];
        let plan = Plan {
            areas: vec![Area {
                name: "Dev / Code".into(),
                description: String::new(),
            }],
            assignments: vec![
                Assignment {
                    file: "a.md".into(),
                    area: "Dev / Code".into(),
                },
                Assignment {
                    file: "old.md".into(),
                    area: "Dev".into(),
                },
                Assignment {
                    file: "weird/name:.md".into(),
                    area: "Dev / Code".into(),
                },
                Assignment {
                    file: "lost.md".into(),
                    area: "Nope".into(),
                },
            ],
        };
        let dry = apply(&notes, &plan, &root, false, true).unwrap();
        assert!(
            !root.join("Dev Code/a.md").exists(),
            "dry run writes nothing"
        );
        assert_eq!(dry.written.len(), 4, "a, old→Inbox, weird, lost→Inbox");

        let report = apply(&notes, &plan, &root, false, false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("Dev Code/a.md")).unwrap(),
            "# a\n\nbody a\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("Dev/old.md")).unwrap(),
            "keep me"
        );
        assert!(root.join("Dev Code/weirdname.md").exists());
        assert!(root.join("Inbox/lost.md").exists());
        assert_eq!(report.skipped_secret, vec!["OpenAI-API-Keys.md"]);
        assert_eq!(report.fell_back, vec!["old.md", "lost.md"]);
        assert!(
            report.skipped_existing.is_empty(),
            "old.md is in area Dev which is unknown → Inbox"
        );
        assert!(root.join("Inbox/old.md").exists());

        let again = apply(&notes, &plan, &root, true, false).unwrap();
        assert!(
            again
                .written
                .iter()
                .any(|(a, f)| a == FALLBACK_AREA && f == "OpenAI-API-Keys.md")
        );
        assert_eq!(again.skipped_existing.len(), 4);
        assert_eq!(existing_areas(&root), vec!["Dev", "Dev Code", "Inbox"]);
        let _ = fs::remove_dir_all(root);
    }
}
