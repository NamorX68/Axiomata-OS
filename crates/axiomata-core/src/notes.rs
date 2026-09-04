//! Creating a single new note whose folder, file name — and, if it doesn't
//! already have one, title — the agent decides, not the caller. A
//! scaled-down sibling of [`crate::importer`]'s "agent proposes, code
//! writes" split, for one note typed into the dashboard right now instead
//! of a batch of files already on disk. Like
//! [`crate::importer::assignment_prompt`], [`placement_prompt`] lets the
//! agent propose a brand new top-level area when none of the vault's
//! existing ones genuinely fit — that judgment call is the actual point of
//! having the agent choose at all, not a fallback path. It only nudges
//! toward reusing an existing area first, and toward broad/durable area
//! names over one-off ones, so notes don't scatter into a new folder each.
//!
//! There is deliberately no separate title field in the UI: a note either
//! already starts with its own `# Heading` (kept verbatim, the writer's
//! call), or it doesn't and the agent proposes one as part of the same JSON
//! turn — [`crate::memory::walker::first_heading`] (also used for the
//! Second Brain's file titles) decides which case applies.
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
use crate::importer;
use crate::memory::walker;

/// Characters of the note's content shown to the agent.
const EXCERPT_CHARS: usize = 400;

/// The agent's answer: where the note belongs, what to call the file, and —
/// only asked for when the note has no heading of its own — a title.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Placement {
    pub area: String,
    pub file_name: String,
    #[serde(default)]
    pub title: Option<String>,
}

fn excerpt(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(EXCERPT_CHARS).collect();
    if collapsed.chars().count() > EXCERPT_CHARS {
        out.push('…');
    }
    out
}

/// The placement request handed to the agent (`agents::chat`). When `content`
/// already starts with its own `#` heading, that heading is shown as the
/// title and the agent is only asked for an area and a file name; otherwise
/// it is also asked to propose a short title, since the note doesn't have
/// one yet.
pub fn placement_prompt(content: &str, existing_areas: &[String]) -> String {
    let known_title = walker::first_heading(content);
    let mut p = String::new();
    p.push_str(
        "A new note was just written in a personal Second-Brain workspace. Decide where it \
         belongs: reuse one of the EXISTING areas (folders) below if it genuinely fits; \
         otherwise propose a short, NEW top-level area name — a broad, durable category \
         (e.g. \"Fotografie\"), not a one-off topic (not \"Urlaub Italien 2026\"). Use \
         \"Inbox\" only if nothing existing fits and no new area makes sense either. Also \
         propose a short, folder-safe file name ending in \".md\" (letters, digits, spaces or \
         hyphens only, no path separators).\n\n",
    );
    p.push_str("Existing areas: ");
    p.push_str(&existing_areas.join(", "));
    p.push_str("\n\n");
    match &known_title {
        Some(title) => p.push_str(&format!("Title: {title}\n")),
        None => p.push_str("The note has no title yet — also propose a short, natural one.\n"),
    }
    p.push_str(&format!("Content: {}\n\n", excerpt(content)));
    p.push_str("Answer with ONLY a JSON object, no prose, no code fences, of this exact shape:\n");
    if known_title.is_some() {
        p.push_str("{\"area\":\"<an existing area, a new area name, or Inbox>\",\"file_name\":\"<name>.md\"}");
    } else {
        p.push_str(
            "{\"area\":\"<an existing area, a new area name, or Inbox>\",\"file_name\":\"<name>.md\",\
             \"title\":\"<a short title>\"}",
        );
    }
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

/// Writes `content` under `workspace_root/<area>`, creating that folder if it
/// does not exist yet — expected whenever the agent proposed a genuinely new
/// area, the normal case this function exists to support, not just for
/// `Inbox` — and never overwriting an existing file. `area` is sanitized via
/// [`crate::importer::sanitize_area`] first (folder-safe characters, falls
/// back to `Inbox` itself only for empty/garbage input).
///
/// If `content` does not already start with a `#` heading, `fallback_title`
/// (the agent's proposed title, from [`Placement::title`]) is prepended as
/// one; a missing or blank fallback becomes "Untitled" rather than failing
/// the save over a title.
///
/// Returns the workspace-relative path written.
pub fn write_placed_note(
    workspace_root: &Path,
    content: &str,
    area: &str,
    file_name: &str,
    fallback_title: Option<&str>,
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
        let heading = fallback_title
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Untitled");
        format!("# {heading}\n\n{}\n", content.trim())
    };
    fs::write(&target, markdown).map_err(importer::io(&target))?;
    Ok(target
        .strip_prefix(workspace_root)
        .unwrap_or(&target)
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn placement_prompt_lists_areas_and_asks_for_json_only() {
        let p = placement_prompt(
            "# Rust generics\n\nSome content here.",
            &["Entwicklung".into(), "Inbox".into()],
        );
        assert!(p.contains("Entwicklung, Inbox"));
        assert!(p.contains("Title: Rust generics"));
        assert!(p.contains("ONLY a JSON object"));
    }

    #[test]
    fn placement_prompt_explicitly_allows_a_new_area() {
        let p = placement_prompt("# T\n\nc", &["Arbeit".into()]);
        assert!(p.contains("propose a short, NEW top-level area name"));
        assert!(p.contains("a new area name, or Inbox"));
    }

    #[test]
    fn placement_prompt_asks_for_a_title_only_when_the_note_has_none() {
        let untitled = placement_prompt("Just some prose, no heading.", &[]);
        assert!(untitled.contains("no title yet"));
        assert!(untitled.contains("\"title\":"));

        let titled = placement_prompt("# Already Titled\n\nBody.", &[]);
        assert!(!titled.contains("no title yet"));
        assert!(!titled.contains("\"title\":"));
        assert!(titled.contains("Title: Already Titled"));
    }

    #[test]
    fn placement_prompt_excerpts_long_content() {
        let long = "word ".repeat(200);
        let p = placement_prompt(&long, &[]);
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
    fn write_placed_note_creates_a_brand_new_area_folder() {
        let dir = unique_temp_dir("notes-write-new-area");
        fs::create_dir_all(&dir).unwrap();
        let rel = write_placed_note(
            &dir,
            "Body.",
            "Fotografie",
            "first-shot.md",
            Some("First shot"),
        )
        .unwrap();
        assert_eq!(rel, "Fotografie/first-shot.md");
        assert!(dir.join("Fotografie").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_placed_note_creates_the_area_folder_and_dedups_on_collision() {
        let dir = unique_temp_dir("notes-write");
        fs::create_dir_all(&dir).unwrap();
        let rel1 = write_placed_note(&dir, "First one.", "Inbox", "idea.md", Some("Idea")).unwrap();
        assert_eq!(rel1, "Inbox/idea.md");
        assert_eq!(
            fs::read_to_string(dir.join(&rel1)).unwrap(),
            "# Idea\n\nFirst one.\n"
        );

        // A second note with the same title/file name never overwrites the first.
        let rel2 =
            write_placed_note(&dir, "Second one.", "Inbox", "idea.md", Some("Idea")).unwrap();
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
        // A fallback title is offered but must be ignored: the note's own heading wins.
        let rel = write_placed_note(
            &dir,
            "# My Own Title\n\nBody.",
            "Inbox",
            "titled.md",
            Some("Ignored"),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(&rel)).unwrap(),
            "# My Own Title\n\nBody.\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_placed_note_falls_back_to_untitled_without_a_heading_or_agent_title() {
        let dir = unique_temp_dir("notes-write-untitled");
        fs::create_dir_all(&dir).unwrap();
        let rel = write_placed_note(&dir, "Just some prose.", "Inbox", "prose.md", None).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(&rel)).unwrap(),
            "# Untitled\n\nJust some prose.\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
