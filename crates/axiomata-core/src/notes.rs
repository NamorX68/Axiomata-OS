//! Creating a single new note whose folder and file name the agent decides,
//! not the caller — a scaled-down sibling of [`crate::importer`]'s "agent
//! proposes, code writes" split, for one note typed into the dashboard right
//! now instead of a batch of files already on disk. Unlike
//! [`crate::importer::assignment_prompt`], which lets the agent *invent* new
//! top-level areas for a fresh import, [`placement_prompt`] only ever offers
//! the vault's existing areas (or [`crate::importer::FALLBACK_AREA`]) — an
//! established, organised vault should not sprout a new top-level folder
//! every time a quick note gets saved.
//!
//! This module is deliberately agent-free (like `importer`): building the
//! prompt, parsing the reply, and writing the file are pure and unit-tested
//! here; the actual `agents::chat` call is made by the caller (the
//! `create_note` Tauri command), the same split `axiomata-cli`'s `import`
//! command uses around `importer::assignment_prompt` / `parse_plan`.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::AxiomataError;
use crate::importer::{self, FALLBACK_AREA};

/// Characters of the note's content shown to the agent.
const EXCERPT_CHARS: usize = 400;

/// The agent's answer: where the note belongs and what to call the file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Placement {
    pub area: String,
    pub file_name: String,
}

fn excerpt(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(EXCERPT_CHARS).collect();
    if collapsed.chars().count() > EXCERPT_CHARS {
        out.push('…');
    }
    out
}

/// The placement request handed to the agent (`agents::chat`).
pub fn placement_prompt(title: &str, content: &str, existing_areas: &[String]) -> String {
    let mut p = String::new();
    p.push_str(
        "A new note was just written in a personal Second-Brain workspace. Decide where it \
         belongs: pick the single best-fitting EXISTING area (folder) from the list below, or \
         \"Inbox\" if truly none fit — never invent a new area. Also propose a short, \
         folder-safe file name ending in \".md\" (letters, digits, spaces or hyphens only, no \
         path separators).\n\n",
    );
    p.push_str("Existing areas: ");
    p.push_str(&existing_areas.join(", "));
    p.push_str("\n\n");
    p.push_str(&format!(
        "Title: {title}\nExcerpt: {}\n\n",
        excerpt(content)
    ));
    p.push_str(
        "Answer with ONLY a JSON object, no prose, no code fences, of this exact shape:\n\
         {\"area\":\"<one of the existing areas, or Inbox>\",\"file_name\":\"<name>.md\"}",
    );
    p
}

/// Parses the agent's reply, tolerating surrounding prose or code fences —
/// the same tolerant brace-slicing [`crate::importer::parse_plan`] uses.
pub fn parse_placement(reply: &str) -> Result<Placement, AxiomataError> {
    let start = reply.find('{');
    let end = reply.rfind('}');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &reply[s..=e],
        _ => {
            return Err(AxiomataError::AgentApi {
                backend: "claude-code",
                message: "the placement reply contained no JSON object".to_string(),
            });
        }
    };
    serde_json::from_str(json).map_err(|e| AxiomataError::AgentApi {
        backend: "claude-code",
        message: format!("could not parse the placement reply: {e}"),
    })
}

/// Finds a free `<dir>/<stem>[-N].md` path so a new note never overwrites an
/// existing file — tries the bare name first, then `-2`, `-3`, …
fn free_path(dir: &Path, file_name: &str) -> std::path::PathBuf {
    let stem = file_name.strip_suffix(".md").unwrap_or(file_name);
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            dir.join(format!("{stem}.md"))
        } else {
            dir.join(format!("{stem}-{n}.md"))
        };
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Writes `title`/`content` under `workspace_root/<area>`, creating that one
/// folder if it does not exist yet (only `Inbox` should ever need this —
/// every other area came from [`crate::importer::existing_areas`]), and
/// never overwriting an existing file. `area` should already be resolved via
/// [`resolved_area`] — this function sanitizes it again regardless, but does
/// not fall back to `Inbox` itself, so an unvalidated area from a raw agent
/// reply could still land somewhere unexpected.
///
/// Returns the workspace-relative path written.
pub fn write_placed_note(
    workspace_root: &Path,
    title: &str,
    content: &str,
    area: &str,
    file_name: &str,
) -> Result<String, AxiomataError> {
    let area = importer::sanitize_area(area);
    let dir = workspace_root.join(&area);
    if !dir.is_dir() {
        fs::create_dir_all(&dir).map_err(importer::io(&dir))?;
    }
    let file_name = importer::sanitize_file_name(file_name);
    let target = free_path(&dir, &file_name);
    let markdown = if content.trim_start().starts_with('#') {
        format!("{}\n", content.trim_end())
    } else {
        format!("# {title}\n\n{}\n", content.trim())
    };
    fs::write(&target, markdown).map_err(importer::io(&target))?;
    Ok(target
        .strip_prefix(workspace_root)
        .unwrap_or(&target)
        .to_string_lossy()
        .into_owned())
}

/// The area a `Placement` should actually use: `placement.area` if it names
/// one of `existing_areas` (or is already [`FALLBACK_AREA`]), else the
/// fallback — the same "unknown answer degrades to Inbox" rule
/// [`crate::importer::apply`] applies per note.
pub fn resolved_area(placement: &Placement, existing_areas: &[String]) -> String {
    let sanitized = importer::sanitize_area(&placement.area);
    if sanitized == FALLBACK_AREA || existing_areas.contains(&sanitized) {
        sanitized
    } else {
        FALLBACK_AREA.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn placement_prompt_lists_areas_and_asks_for_json_only() {
        let p = placement_prompt(
            "Rust generics",
            "Some content here.",
            &["Entwicklung".into(), "Inbox".into()],
        );
        assert!(p.contains("Entwicklung, Inbox"));
        assert!(p.contains("Title: Rust generics"));
        assert!(p.contains("ONLY a JSON object"));
    }

    #[test]
    fn placement_prompt_excerpts_long_content() {
        let long = "word ".repeat(200);
        let p = placement_prompt("T", &long, &[]);
        assert!(p.contains('…'));
    }

    #[test]
    fn parse_placement_tolerates_code_fences_and_prose() {
        let reply = "Sure!\n```json\n{\"area\": \"Entwicklung\", \"file_name\": \"rust-generics.md\"}\n```\n";
        let placement = parse_placement(reply).unwrap();
        assert_eq!(placement.area, "Entwicklung");
        assert_eq!(placement.file_name, "rust-generics.md");
    }

    #[test]
    fn parse_placement_rejects_a_reply_with_no_json() {
        assert!(parse_placement("I could not decide.").is_err());
    }

    #[test]
    fn resolved_area_falls_back_for_an_unknown_area() {
        let known = vec!["Entwicklung".to_string(), "Arbeit".to_string()];
        let ok = Placement {
            area: "Entwicklung".into(),
            file_name: "x.md".into(),
        };
        assert_eq!(resolved_area(&ok, &known), "Entwicklung");
        let unknown = Placement {
            area: "Made Up Area".into(),
            file_name: "x.md".into(),
        };
        assert_eq!(resolved_area(&unknown, &known), FALLBACK_AREA);
        let inbox = Placement {
            area: "Inbox".into(),
            file_name: "x.md".into(),
        };
        assert_eq!(resolved_area(&inbox, &known), FALLBACK_AREA);
    }

    #[test]
    fn write_placed_note_creates_the_area_folder_and_dedups_on_collision() {
        let dir = unique_temp_dir("notes-write");
        fs::create_dir_all(&dir).unwrap();
        let rel1 = write_placed_note(&dir, "Idea", "First one.", "Inbox", "idea.md").unwrap();
        assert_eq!(rel1, "Inbox/idea.md");
        assert_eq!(
            fs::read_to_string(dir.join(&rel1)).unwrap(),
            "# Idea\n\nFirst one.\n"
        );

        // A second note with the same title/file name never overwrites the first.
        let rel2 = write_placed_note(&dir, "Idea", "Second one.", "Inbox", "idea.md").unwrap();
        assert_eq!(rel2, "Inbox/idea-2.md");
        assert_eq!(
            fs::read_to_string(dir.join(&rel1)).unwrap(),
            "# Idea\n\nFirst one.\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_placed_note_keeps_content_that_already_starts_with_a_heading() {
        let dir = unique_temp_dir("notes-write-heading");
        fs::create_dir_all(&dir).unwrap();
        let rel = write_placed_note(
            &dir,
            "Ignored",
            "# My Own Title\n\nBody.",
            "Inbox",
            "titled.md",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(&rel)).unwrap(),
            "# My Own Title\n\nBody.\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
