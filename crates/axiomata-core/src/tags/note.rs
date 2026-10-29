//! Setting a note's tags and links the one narrow way an agent may (`docs/plans/tags-links.md` V2, Q8, Q10): `tags:`
//! in the frontmatter and a `## Related` section at its end — nothing else of the note changes, byte for byte. A tag
//! from the list is set; one that is not on it becomes a proposal for the owner and is not set; a related note must
//! exist, at most [`MAX_RELATED`] go in per call, and a note already linked is left alone.

use std::collections::HashSet;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

use super::detect::{LineIter, frontmatter, normalise, tags_key, wikilinks};
use super::{Vault, list, newline_of, note_path, proposals, refused, workspace};
use crate::config::Config;
use crate::error::AxiomataError;

/// Related notes one call may add (Q8: "höchstens 3").
pub const MAX_RELATED: usize = 3;

/// Tags one call may name: far above the 1–4 per note the prompts ask for (Q7), a bound on a run gone wrong.
pub const MAX_TAGS_PER_CALL: usize = 12;

/// Proposals one call may make.
pub const MAX_PROPOSALS_PER_CALL: usize = 10;

/// The heading of the links section an agent writes.
pub const RELATED_HEADING: &str = "## Related";

/// What a caller asks to be set on one note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagRequest {
    /// Workspace-relative path of the note.
    pub note: String,
    /// Tags to set; one not on the list becomes a proposal without a description.
    pub tags: Vec<String>,
    /// Notes to link under `## Related`, by name or path.
    pub related: Vec<String>,
    /// New tags with their description, for the owner to accept.
    pub proposals: Vec<(String, String)>,
    /// Say what would change and write nothing to the note.
    pub dry_run: bool,
}

/// What came of a [`TagRequest`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TagOutcome {
    pub note: String,
    /// Tags newly set in the frontmatter (or that would be, in a dry run).
    pub tagged: Vec<String>,
    /// Tags the note already had.
    pub already: Vec<String>,
    /// Link names newly added under `## Related`.
    pub related: Vec<String>,
    /// Tags now waiting for the owner.
    pub proposed: Vec<String>,
    /// What was not done, and why, one line each.
    pub refused: Vec<String>,
    /// Whether the note was written.
    pub written: bool,
}

/// `text` with `tags` (normalised) in its frontmatter's `tags:` key: the key's items as they were, then the new ones,
/// as a YAML block list. A text without a frontmatter gets one at its top. Returns the new text and the tags added;
/// with none to add the text comes back unchanged.
///
/// # Errors
///
/// The note's `tags:` key is written in a form that could not be written back unchanged (a comment, a multi-line flow
/// list, a quoted item with a space …): the owner's frontmatter is never rewritten then.
pub fn with_tags(text: &str, tags: &[String]) -> Result<(String, Vec<String>), AxiomataError> {
    let key = tags_key(text);
    let have: HashSet<String> = key
        .iter()
        .flat_map(|key| key.items.iter())
        .filter_map(|item| normalise(&item.value))
        .collect();
    let mut added: Vec<String> = Vec::new();
    for tag in tags {
        if !have.contains(tag) && !added.contains(tag) {
            added.push(tag.clone());
        }
    }
    if added.is_empty() {
        return Ok((text.to_owned(), added));
    }
    if key.as_ref().is_some_and(|key| !key.simple) {
        return Err(refused(
            "the note's `tags:` is written in a form the app does not rewrite (a comment, a quoted item with a \
             space, a \
             list over several lines); its tags are set by hand",
        ));
    }
    let items: Vec<String> = key
        .iter()
        .flat_map(|key| key.items.iter().map(|item| item.value.clone()))
        .chain(added.iter().cloned())
        .collect();
    let out = write_tags_key(text, &items);
    Ok((out, added))
}

/// `item` as a YAML list item that reads back as the same text: quoted when YAML would take it for something else (a
/// boolean, null, a number).
fn yaml_item(item: &str) -> String {
    const SPECIAL: [&str; 11] = [
        "y", "n", "yes", "no", "true", "false", "on", "off", "null", "~", "",
    ];
    let lower = item.to_lowercase();
    let numeric = item.parse::<f64>().is_ok() || lower.starts_with("0x") || lower.starts_with("0o");
    if SPECIAL.contains(&lower.as_str()) || numeric || item.starts_with(['"', '\'']) {
        if item.starts_with(['"', '\'']) {
            item.to_owned()
        } else {
            format!("\"{item}\"")
        }
    } else {
        item.to_owned()
    }
}

/// Whether a note's name may go into a `[[link]]` as it is: nothing that ends the link, points it elsewhere or starts
/// a new line (security audit — file names come from mail subjects and page titles).
pub(crate) fn link_safe(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || "[]|#^`<>\\".contains(c))
}

/// At most [`SHOWN_CHARS`] of what a caller passed, for a refusal line: the line goes back into an agent's context.
fn shown(raw: &str) -> String {
    let cut: String = raw.chars().take(SHOWN_CHARS).collect();
    if cut.len() < raw.len() {
        format!("{cut:?}…")
    } else {
        format!("{cut:?}")
    }
}

/// How much of a refused argument a refusal repeats.
const SHOWN_CHARS: usize = 80;

/// `text` with its frontmatter's `tags:` key holding exactly `items`, written as a YAML block list in place of the old
/// key (or at the frontmatter's end; a text without a frontmatter gets one at its top). The rest stays byte for byte.
pub(crate) fn write_tags_key(text: &str, items: &[String]) -> String {
    let nl = newline_of(text);
    let mut block = format!("tags:{nl}");
    for item in items {
        block.push_str(&format!("  - {}{nl}", yaml_item(item)));
    }
    match (frontmatter(text), tags_key(text)) {
        (Some(_), Some(key)) => format!(
            "{}{block}{}",
            &text[..key.lines.start],
            &text[key.lines.end..]
        ),
        (Some(fm), None) => format!(
            "{}{block}{}",
            &text[..fm.close_start],
            &text[fm.close_start..]
        ),
        (None, _) => {
            let (bom, rest) = match text.strip_prefix('\u{feff}') {
                Some(rest) => ("\u{feff}", rest),
                None => ("", text),
            };
            format!("{bom}---{nl}{block}---{nl}{rest}")
        }
    }
}

/// `text` with a `- [[name]]` line per name under its `## Related` heading — after the section's last line, or as a
/// new section at the end. A name the section already links is skipped. Returns the new text and the names added.
pub fn with_related(text: &str, names: &[String]) -> (String, Vec<String>) {
    let section = related_section(text);
    let present: HashSet<String> = section
        .as_ref()
        .map(|s| {
            wikilinks(&text[s.clone()])
                .into_iter()
                .map(|range| {
                    link_target(&text[s.start + range.start..s.start + range.end]).to_lowercase()
                })
                .collect()
        })
        .unwrap_or_default();
    let mut added: Vec<String> = Vec::new();
    for name in names.iter().filter(|name| link_safe(name)) {
        let lower = name.to_lowercase();
        if !present.contains(&lower) && !added.iter().any(|known| known.to_lowercase() == lower) {
            added.push(name.clone());
        }
    }
    if added.is_empty() {
        return (text.to_owned(), added);
    }
    let nl = newline_of(text);
    let lines: String = added
        .iter()
        .map(|name| format!("- [[{name}]]{nl}"))
        .collect();
    let out = match section {
        Some(range) => {
            let (insert_at, needs_break, after_heading) = last_line_end(text, range);
            let mut out = String::with_capacity(text.len() + lines.len() + 2);
            out.push_str(&text[..insert_at]);
            if needs_break {
                out.push_str(nl);
            }
            if after_heading {
                out.push_str(nl);
            }
            out.push_str(&lines);
            out.push_str(&text[insert_at..]);
            out
        }
        None => {
            let mut out = text.to_owned();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push_str(nl);
                }
                if !out.ends_with(&format!("{nl}{nl}")) {
                    out.push_str(nl);
                }
            }
            out.push_str(&format!("{RELATED_HEADING}{nl}{nl}{lines}"));
            out
        }
    };
    (out, added)
}

/// The part of a link's inner text that names its target: before `|` and `#`.
pub(crate) fn link_target(inner: &str) -> &str {
    inner.split(['|', '#']).next().unwrap_or("").trim()
}

/// The byte range of the `## Related` section: from its heading line to the next heading of level one or two (or the
/// end), fenced code skipped.
fn related_section(text: &str) -> Option<std::ops::Range<usize>> {
    let mut start = None;
    let mut fence = false;
    for line in LineIter::new(text, 0) {
        let content = text[line.content.clone()].trim();
        if content.starts_with("```") || content.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let level_one_or_two = content.starts_with("# ")
            || content.starts_with("## ")
            || content == "#"
            || content == "##";
        match start {
            None if content.eq_ignore_ascii_case(RELATED_HEADING) => start = Some(line.start),
            Some(begin) if level_one_or_two => return Some(begin..line.start),
            _ => {}
        }
    }
    start.map(|begin| begin..text.len())
}

/// Where new lines go in a section: after its last line that is not blank. Also whether that line lacks its line
/// break (the text's last line) and whether it is the heading itself (a blank line goes between).
fn last_line_end(text: &str, section: std::ops::Range<usize>) -> (usize, bool, bool) {
    let mut last = None;
    for line in LineIter::new(text, section.start).take_while(|l| l.start < section.end) {
        if !text[line.content.clone()].trim().is_empty() {
            last = Some(line);
        }
    }
    let Some(line) = last else {
        return (section.start, false, false);
    };
    let needs_break = line.end == line.content.end;
    (line.end, needs_break, line.start == section.start)
}

/// Sets what `request` asks on its note, as narrowly as V2 says. Proposals are stored in `db` (with the note, so an
/// accepted one is set there afterwards), also in a dry run: they change no file, and the owner decides them.
///
/// # Errors
///
/// The note may not be tagged ([`note_path`]), does not exist, is no text file of its own, or changed while it was
/// edited; the tag list or the vault cannot be read. Single tags or links that cannot be set are no error: they are
/// listed in [`TagOutcome::refused`].
pub fn tag_note(
    config: &Config,
    db: &Mutex<Connection>,
    by: &str,
    request: &TagRequest,
) -> Result<TagOutcome, AxiomataError> {
    let note = note_path(&request.note)?;
    let Some(file) = workspace::read(config, &note)? else {
        return Err(refused(format!("there is no note {note}")));
    };
    let known: HashSet<String> = list::load(config)?
        .into_iter()
        .map(|entry| entry.tag)
        .collect();
    let mut outcome = TagOutcome {
        note: note.clone(),
        ..TagOutcome::default()
    };
    let mut wanted: Vec<String> = Vec::new();
    let mut new_tags: Vec<(String, String)> = Vec::new();
    for raw in request.tags.iter().take(MAX_TAGS_PER_CALL) {
        match normalise(raw) {
            Some(tag) if known.contains(&tag) => wanted.push(tag),
            Some(tag) => new_tags.push((tag, String::new())),
            None => outcome.refused.push(format!("{} is no tag", shown(raw))),
        }
    }
    if request.tags.len() > MAX_TAGS_PER_CALL {
        outcome.refused.push(format!(
            "only {MAX_TAGS_PER_CALL} tags per call; the rest were left out"
        ));
    }
    for (raw, description) in request.proposals.iter().take(MAX_PROPOSALS_PER_CALL) {
        match normalise(raw) {
            Some(tag) if known.contains(&tag) => wanted.push(tag),
            Some(tag) => {
                new_tags.retain(|(known, _)| *known != tag);
                new_tags.push((tag, description.clone()));
            }
            None => outcome.refused.push(format!("{} is no tag", shown(raw))),
        }
    }
    if request.proposals.len() > MAX_PROPOSALS_PER_CALL {
        outcome.refused.push(format!(
            "only {MAX_PROPOSALS_PER_CALL} proposals per call; the rest were left out"
        ));
    }
    let names = related_names(config, &note, &file.content, &request.related, &mut outcome)?;
    let (tagged_text, tagged) = match with_tags(&file.content, &wanted) {
        Ok(done) => done,
        Err(err) => {
            outcome.refused.push(err.to_string());
            (file.content.clone(), Vec::new())
        }
    };
    let (text, related) = with_related(&tagged_text, &names);
    outcome.already = wanted
        .into_iter()
        .filter(|tag| !tagged.contains(tag))
        .collect();
    outcome.already.dedup();
    outcome.tagged = tagged;
    outcome.related = related;
    for (tag, description) in new_tags {
        // A dry run changes nothing anywhere, proposals included (security audit): it says what it would propose.
        if request.dry_run {
            outcome.proposed.push(tag);
            continue;
        }
        let proposed = {
            let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
            proposals::propose(&db, &tag, &description, Some(&note), by)?
        };
        match proposed {
            proposals::Proposed::Waiting => outcome.proposed.push(tag),
            proposals::Proposed::RejectedBefore => outcome.refused.push(format!(
                "#{tag} was turned down by the owner lately; pick one from the list"
            )),
            proposals::Proposed::Full => outcome.refused.push(format!(
                "#{tag} not proposed: too many proposals wait for the owner already"
            )),
        }
    }
    if !request.dry_run && text != file.content {
        workspace::write(config, &note, &text, Some(&file.version))?;
        outcome.written = true;
    }
    Ok(outcome)
}

/// The link names for `wanted` related notes: each must resolve to one note of the vault, not the note itself and
/// not one it links already; at most [`MAX_RELATED`].
fn related_names(
    config: &Config,
    note: &str,
    text: &str,
    wanted: &[String],
    outcome: &mut TagOutcome,
) -> Result<Vec<String>, AxiomataError> {
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let vault = Vault::load(config)?;
    let linked: HashSet<String> = wikilinks(text)
        .into_iter()
        .filter_map(|range| vault.resolve(note, link_target(&text[range])))
        .map(|path| path.to_lowercase())
        .collect();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for target in wanted {
        let Some(path) = vault.resolve(
            note,
            super::strip_md(target.trim()).trim_matches(['[', ']']),
        ) else {
            outcome.refused.push(format!(
                "{} is no note of the vault (or more than one has that name)",
                shown(target)
            ));
            continue;
        };
        if super::is_protected(&path) {
            outcome.refused.push(format!(
                "{} is kept by the app, not a related note",
                shown(target)
            ));
            continue;
        }
        let lower = path.to_lowercase();
        if lower == note.to_lowercase() || linked.contains(&lower) || !seen.insert(lower) {
            continue;
        }
        if names.len() == MAX_RELATED {
            outcome.refused.push(format!(
                "at most {MAX_RELATED} related notes; {} was left out",
                shown(target)
            ));
            continue;
        }
        let name = vault.link_name(&path);
        if !link_safe(&name) {
            outcome.refused.push(format!(
                "{} has a name no link can carry; link it by hand",
                shown(&path)
            ));
            continue;
        }
        names.push(name);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn tags_go_into_a_new_frontmatter_and_nothing_else_moves() {
        let (out, added) = with_tags("# Title\n\nBody #x\n", &tags(&["rust", "ki-news"])).unwrap();
        assert_eq!(
            out,
            "---\ntags:\n  - rust\n  - ki-news\n---\n# Title\n\nBody #x\n"
        );
        assert_eq!(added, tags(&["rust", "ki-news"]));
    }

    #[test]
    fn tags_join_an_existing_key_in_any_form_and_keep_the_other_keys() {
        let text = "---\ntitle: A\ntags: [Rust, old]\ncreated: 2024-01-01\n---\nBody\n";
        let (out, added) = with_tags(text, &tags(&["rust", "new"])).unwrap();
        assert_eq!(
            out,
            "---\ntitle: A\ntags:\n  - Rust\n  - old\n  - new\ncreated: 2024-01-01\n---\nBody\n"
        );
        assert_eq!(added, tags(&["new"]));
        let block = "---\ntags:\n  - a\nnext: 1\n---\n";
        assert_eq!(
            with_tags(block, &tags(&["b"])).unwrap().0,
            "---\ntags:\n  - a\n  - b\nnext: 1\n---\n"
        );
        let none = "---\ntitle: T\n---\nx";
        assert_eq!(
            with_tags(none, &tags(&["b"])).unwrap().0,
            "---\ntitle: T\ntags:\n  - b\n---\nx"
        );
    }

    #[test]
    fn known_tags_leave_the_text_untouched_and_crlf_stays() {
        let text = "---\r\ntags: a\r\n---\r\nBody\r\n";
        assert_eq!(with_tags(text, &tags(&["a"])).unwrap().0, text);
        assert_eq!(
            with_tags(text, &tags(&["b"])).unwrap().0,
            "---\r\ntags:\r\n  - a\r\n  - b\r\n---\r\nBody\r\n"
        );
        let bom = "\u{feff}# T\n";
        assert_eq!(
            with_tags(bom, &tags(&["b"])).unwrap().0,
            "\u{feff}---\ntags:\n  - b\n---\n# T\n"
        );
    }

    #[test]
    fn an_odd_tags_key_is_never_rewritten_and_ambiguous_items_are_quoted() {
        assert!(with_tags("---\ntags: [a] # mine\n---\n", &tags(&["b"])).is_err());
        assert_eq!(
            with_tags("---\ntags:\n  - \"no\"\n---\n", &tags(&["1e3", "yes"]))
                .unwrap()
                .0,
            "---\ntags:\n  - \"no\"\n  - \"1e3\"\n  - \"yes\"\n---\n"
        );
    }

    #[test]
    fn a_name_that_could_break_a_link_is_never_written() {
        for bad in ["a]]b", "x\n## Evil", "a|b", "a#b", "", "a^b"] {
            assert!(!link_safe(bad), "{bad:?}");
        }
        assert!(link_safe("Rust lernen (2024)"));
        assert_eq!(with_related("x\n", &tags(&["a]]\n# Evil"])).0, "x\n");
    }

    #[test]
    fn related_links_go_into_a_new_or_existing_section() {
        let (out, added) = with_related("# T\n\nBody", &tags(&["A", "B"]));
        assert_eq!(out, "# T\n\nBody\n\n## Related\n\n- [[A]]\n- [[B]]\n");
        assert_eq!(added, tags(&["A", "B"]));
        let existing = "# T\n\n## Related\n\n- [[A]]\n\n## Later\ntext\n";
        let (out, added) = with_related(existing, &tags(&["a", "C"]));
        assert_eq!(
            out,
            "# T\n\n## Related\n\n- [[A]]\n- [[C]]\n\n## Later\ntext\n"
        );
        assert_eq!(added, tags(&["C"]));
        let bare = "# T\n## related";
        assert_eq!(
            with_related(bare, &tags(&["X"])).0,
            "# T\n## related\n\n- [[X]]\n"
        );
        assert_eq!(with_related("", &tags(&["X"])).0, "## Related\n\n- [[X]]\n");
    }

    #[test]
    fn a_related_heading_in_code_is_no_section() {
        let text = "```\n## Related\n```\n";
        assert_eq!(
            with_related(text, &tags(&["X"])).0,
            "```\n## Related\n```\n\n## Related\n\n- [[X]]\n"
        );
    }

    /// A scratch vault with a list, a few notes and a database; removed when dropped.
    struct Fixture {
        root: std::path::PathBuf,
        config: Config,
        db: Mutex<Connection>,
    }

    impl Fixture {
        fn new() -> Self {
            let root = unique_temp_dir("axiomata-tags-note");
            fs::create_dir_all(root.join("Dev")).unwrap();
            fs::create_dir_all(root.join("KI")).unwrap();
            fs::write(root.join("TAGS.md"), "- #rust — Rust\n- #ki-news — News\n").unwrap();
            fs::write(
                root.join("Dev/Rust lernen.md"),
                "# Rust lernen\n\nSee [[Cargo]].\n",
            )
            .unwrap();
            fs::write(root.join("Dev/Cargo.md"), "# Cargo\n").unwrap();
            fs::write(root.join("KI/Agenten.md"), "# Agenten\n").unwrap();
            fs::write(root.join("KI/Other.md"), "# Other\n").unwrap();
            fs::write(root.join("KI/Fourth.md"), "# Fourth\n").unwrap();
            let config = Config {
                workspace_root: root.clone(),
                ..Config::default()
            };
            let db = crate::db::open_and_migrate_at(&root.join(".test.db")).unwrap();
            Fixture {
                root,
                config,
                db: Mutex::new(db),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn tag_note_sets_known_tags_proposes_new_ones_and_checks_links() {
        let f = Fixture::new();
        let outcome = tag_note(
            &f.config,
            &f.db,
            "test",
            &TagRequest {
                note: "Dev/Rust lernen.md".into(),
                tags: tags(&["#Rust", "brand-new", "no tag"]),
                related: tags(&[
                    "Agenten",
                    "Cargo",
                    "Nope",
                    "Dev/Rust lernen",
                    "KI/Other.md",
                    "Fourth",
                ]),
                proposals: vec![("async".into(), "Async code".into())],
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(outcome.tagged, tags(&["rust"]));
        assert_eq!(outcome.related, tags(&["Agenten", "Other", "Fourth"]));
        assert_eq!(outcome.proposed, tags(&["brand-new", "async"]));
        assert!(outcome.written);
        assert_eq!(outcome.refused.len(), 2, "{:?}", outcome.refused);
        let text = fs::read_to_string(f.root.join("Dev/Rust lernen.md")).unwrap();
        assert_eq!(
            text,
            "---\ntags:\n  - rust\n---\n# Rust lernen\n\nSee [[Cargo]].\n\n\
             ## Related\n\n- [[Agenten]]\n- [[Other]]\n- [[Fourth]]\n"
        );
        let open = proposals::open(&f.db.lock().unwrap()).unwrap();
        assert_eq!(open.len(), 2);
        assert_eq!(open[1].notes, vec!["Dev/Rust lernen.md".to_string()]);
    }

    #[test]
    fn a_dry_run_writes_nothing_and_a_repeat_changes_nothing() {
        let f = Fixture::new();
        let request = TagRequest {
            note: "KI/Agenten.md".into(),
            tags: tags(&["ki-news"]),
            related: tags(&["Other"]),
            dry_run: true,
            ..TagRequest::default()
        };
        let outcome = tag_note(&f.config, &f.db, "test", &request).unwrap();
        assert_eq!(outcome.tagged, tags(&["ki-news"]));
        assert!(!outcome.written);
        assert_eq!(
            fs::read_to_string(f.root.join("KI/Agenten.md")).unwrap(),
            "# Agenten\n"
        );
        let real = TagRequest {
            dry_run: false,
            ..request
        };
        assert!(tag_note(&f.config, &f.db, "test", &real).unwrap().written);
        let again = tag_note(&f.config, &f.db, "test", &real).unwrap();
        assert!(!again.written && again.tagged.is_empty() && again.related.is_empty());
        assert_eq!(again.already, tags(&["ki-news"]));
    }

    #[test]
    fn tag_note_refuses_protected_missing_and_linked_notes() {
        let f = Fixture::new();
        let mut request = TagRequest {
            note: "AGENTS.md".into(),
            tags: tags(&["rust"]),
            ..TagRequest::default()
        };
        assert!(tag_note(&f.config, &f.db, "t", &request).is_err());
        request.note = "Dev/none.md".into();
        assert!(tag_note(&f.config, &f.db, "t", &request).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(f.root.join("Dev/Cargo.md"), f.root.join("Dev/link.md"))
                .unwrap();
            request.note = "Dev/link.md".into();
            assert!(
                tag_note(&f.config, &f.db, "t", &request).is_err(),
                "a link is never followed"
            );
            assert_eq!(
                fs::read_to_string(f.root.join("Dev/Cargo.md")).unwrap(),
                "# Cargo\n"
            );
        }
    }
}
