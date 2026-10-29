//! Renames that pull their uses along, as Obsidian does: a note renamed in the app gets every `[[link]]` to it
//! rewritten in the other notes — alias and `#heading` kept (`docs/plans/tags-links.md` V3) — and a tag renamed in the
//! settings (or with `axiomata-cli notes tags rename`) is rewritten in every note's text and frontmatter, its nested
//! tags with it, merging into a tag that is listed already (TL4).
//!
//! Both walk the vault's Markdown notes once, read and write each through the workspace's guard (a note the owner
//! changed in between is skipped, never overwritten), and can count first without writing (`dry_run`). Code (fenced
//! or inline) is left alone, as the link and tag scans do.

use std::collections::HashSet;

use serde::Serialize;

use super::detect::{inline_tags, is_within, normalise, tags_key, wikilinks};
use super::list::{self, TagEntry};
use super::note::{link_safe, write_tags_key};
use super::{note_path, refused, strip_md, workspace};
use crate::config::Config;
use crate::error::AxiomataError;
use crate::graph::{file_stem, normalise_rel};

/// What a rename changed (or, in a dry run, would change).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RenameCount {
    /// Notes rewritten.
    pub files: usize,
    /// Links (for a note) or uses of the tag (for a tag) rewritten.
    pub links: usize,
}

/// A workspace-relative Markdown path as a rename names it: forward slashes, no leading `./` or `/`, no `..`.
fn markdown_rel(rel: &str) -> Result<String, AxiomataError> {
    let cleaned = rel.trim().replace('\\', "/");
    let cleaned = cleaned.strip_prefix("./").unwrap_or(&cleaned).to_owned();
    // The new name goes into `[[…]]` in every note: nothing that could end the link or start a line (security audit).
    let ok = !cleaned.is_empty()
        && !cleaned.starts_with('/')
        && cleaned.to_lowercase().ends_with(".md")
        && link_safe(&cleaned)
        && cleaned
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'));
    if ok {
        Ok(cleaned)
    } else {
        Err(refused(format!(
            "{rel:?} is no Markdown path inside the vault"
        )))
    }
}

/// Rewrites every `[[link]]` that pointed to the note `old_rel` so it points to `new_rel`, in all notes of the vault.
/// Works before and after the file itself was renamed. A link by name keeps the name form (the new file name, or the
/// path when another note has that name too); a link by path gets the new path, with `.md` when it had one. A bare
/// name that another note shares was never a link to `old_rel` and stays.
///
/// # Errors
///
/// `old_rel` or `new_rel` is no Markdown path inside the vault, both are the same, or the vault cannot be walked.
pub fn rename_note_links(
    config: &Config,
    old_rel: &str,
    new_rel: &str,
    dry_run: bool,
) -> Result<RenameCount, AxiomataError> {
    let old = markdown_rel(old_rel)?;
    let new = markdown_rel(new_rel)?;
    if old == new {
        return Err(refused("the old and the new name are the same"));
    }
    let paths = crate::memory::walker::file_paths(config)?;
    let notes: Vec<String> = paths
        .into_iter()
        .filter(|path| path.to_lowercase().ends_with(".md"))
        .collect();
    // A rename has one side on disk: before it the old note, after it the new one. Anything else is no rename.
    let exists = |rel: &str| notes.iter().any(|path| path.eq_ignore_ascii_case(rel));
    if exists(&old) == exists(&new) && !old.eq_ignore_ascii_case(&new) {
        return Err(refused(format!(
            "exactly one of {old} and {new} has to be a note of the vault"
        )));
    }
    let others: Vec<&String> = notes
        .iter()
        .filter(|path| !path.eq_ignore_ascii_case(&old) && !path.eq_ignore_ascii_case(&new))
        .collect();
    let stem_taken = |stem: &str| {
        others
            .iter()
            .any(|path| file_stem(path).eq_ignore_ascii_case(stem))
    };
    let target = LinkTarget {
        old_noext: strip_md(&old).to_lowercase(),
        old_stem: file_stem(&old).to_lowercase(),
        old_stem_shared: stem_taken(&file_stem(&old)),
        new_noext: strip_md(&new).to_owned(),
        new_name: if stem_taken(&file_stem(&new)) {
            strip_md(&new).to_owned()
        } else {
            file_stem(&new)
        },
    };
    let mut count = RenameCount::default();
    for rel in notes.iter().filter(|rel| rewritable(rel)) {
        let Ok(Some(file)) = workspace::read(config, rel) else {
            continue;
        };
        let (text, links) = rewrite_links(&file.content, rel, &target);
        if links == 0 {
            continue;
        }
        if !dry_run && workspace::write(config, rel, &text, Some(&file.version)).is_err() {
            continue;
        }
        count.files += 1;
        count.links += links;
    }
    Ok(count)
}

/// Whether a rename may rewrite `rel`: a note an agent could tag too ([`note_path`]) — no instruction file, not the
/// tag list, nothing the app rewrites itself.
fn rewritable(rel: &str) -> bool {
    note_path(rel).is_ok()
}

/// What a link to the renamed note looks like, and what it becomes.
struct LinkTarget {
    old_noext: String,
    old_stem: String,
    /// Another note has the old file name: a bare name never meant the renamed one.
    old_stem_shared: bool,
    new_noext: String,
    new_name: String,
}

/// `text` (the note `from`) with its links to the renamed note rewritten, and how many there were.
fn rewrite_links(text: &str, from: &str, target: &LinkTarget) -> (String, usize) {
    let folder = from.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for inner in wikilinks(text) {
        let segment_end = text[inner.clone()]
            .find(['|', '#'])
            .map_or(inner.end, |at| inner.start + at);
        let raw = &text[inner.start..segment_end];
        let lead = raw.len() - raw.trim_start().len();
        let written = raw.trim();
        if written.is_empty() {
            continue;
        }
        let range = inner.start + lead..inner.start + lead + written.len();
        let noext = strip_md(written).to_lowercase();
        let by_path =
            noext == target.old_noext || normalise_rel(folder, &noext) == target.old_noext;
        let replacement = if !written.contains('/') {
            let by_name = noext == target.old_stem && !target.old_stem_shared;
            (by_path || by_name).then(|| target.new_name.clone())
        } else {
            let had_md = written.to_lowercase().ends_with(".md");
            by_path.then(|| {
                if had_md {
                    format!("{}.md", target.new_noext)
                } else {
                    target.new_noext.clone()
                }
            })
        };
        if let Some(replacement) = replacement {
            edits.push((range, replacement));
        }
    }
    let count = edits.len();
    (apply_edits(text, edits), count)
}

/// `text` with each range replaced; the ranges must not overlap.
fn apply_edits(text: &str, mut edits: Vec<(std::ops::Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = text.to_owned();
    for (range, replacement) in edits {
        out.replace_range(range, &replacement);
    }
    out
}

/// A note a rename or a lift left alone, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedNote {
    pub note: String,
    pub reason: String,
}

/// What [`rename_tag`] changed (or, in a dry run, would change).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TagRename {
    /// The two names, normalised.
    pub old: String,
    pub new: String,
    /// Notes rewritten.
    pub files: usize,
    /// Uses of the tag rewritten: each `#tag` in a text and each frontmatter item, nested tags included.
    pub uses: usize,
    /// Notes that use the tag but were not (fully) rewritten: a `tags:` the app does not rewrite (its text's `#tag`s
    /// are renamed all the same), a note that changed while it was edited, one that could not be read.
    pub skipped: Vec<SkippedNote>,
    /// `new` was on the list already: `old` merges into it and the entry's description stays (unless one was given).
    pub merged: bool,
    /// The list's entry for `new` after the rename, as stored (or as it would be, in a dry run).
    pub description: String,
}

impl TagRename {
    /// The counts the settings' rename shows.
    pub fn count(&self) -> RenameCount {
        RenameCount {
            files: self.files,
            links: self.uses,
        }
    }
}

/// Renames the tag `old` to `new` in every note — `#old` and `#old/child` in the text, `old` and `old/child` in the
/// frontmatter — and on the tag list. Nested tags follow their parent, as in Obsidian. When `new` is on the list
/// already, `old` merges into it: a note that had both keeps one, and the entry keeps its description. After the
/// rename `new` is on the list — with `description` when given, else with the merged entry's or `old`'s.
///
/// # Errors
///
/// `old` or `new` is no tag, both are the same, `new` lies below `old` (its own uses would be renamed again), `old` is
/// on no note and not on the list (a typo), the vault cannot be walked, or the list cannot be written.
pub fn rename_tag(
    config: &Config,
    old: &str,
    new: &str,
    description: Option<&str>,
    dry_run: bool,
) -> Result<TagRename, AxiomataError> {
    let (Some(old), Some(new)) = (normalise(old), normalise(new)) else {
        return Err(refused("both names must be tags"));
    };
    if old == new {
        return Err(refused("the old and the new tag are the same"));
    }
    if is_within(&new, &old) {
        return Err(refused(format!(
            "#{new} lies below #{old}: its notes would be renamed twice; rename to a tag outside #{old}"
        )));
    }
    let listed = list::current(config)?;
    let given = description.map(list::one_line);
    let renamed = |entries: Vec<TagEntry>| {
        with_new(renamed_list(entries, &old, &new), &new, given.as_deref())
    };
    // The list as it will be is checked before any note is written: a list that cannot be stored stops the rename
    // while the notes are still as they were.
    let planned = list::validated(renamed(listed.clone()))?;
    let mut outcome = TagRename {
        old: old.clone(),
        new: new.clone(),
        merged: listed.iter().any(|entry| entry.tag == new),
        ..TagRename::default()
    };
    for rel in crate::memory::walker::file_paths(config)? {
        if !rel.to_lowercase().ends_with(".md") || !rewritable(&rel) {
            continue;
        }
        let file = match workspace::read(config, &rel) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(err) => {
                outcome.skipped.push(SkippedNote {
                    note: rel,
                    reason: err.to_string(),
                });
                continue;
            }
        };
        let rewrite = rewrite_tag(&file.content, &old, &new);
        if rewrite.kept_key {
            outcome.skipped.push(SkippedNote {
                note: rel.clone(),
                reason: "its `tags:` is written in a form the app does not rewrite; change it there by hand"
                    .to_owned(),
            });
        }
        if rewrite.uses == 0 {
            continue;
        }
        if !dry_run
            && let Err(err) = workspace::write(config, &rel, &rewrite.text, Some(&file.version))
        {
            outcome.skipped.push(SkippedNote {
                note: rel,
                reason: err.to_string(),
            });
            continue;
        }
        outcome.files += 1;
        outcome.uses += rewrite.uses;
    }
    let on_list = listed.iter().any(|entry| is_within(&entry.tag, &old));
    if outcome.uses == 0 && !on_list && outcome.skipped.is_empty() {
        return Err(refused(format!(
            "#{old} is on no note and not on the list; nothing to rename"
        )));
    }
    let stored = if dry_run {
        planned
    } else {
        list::update(config, |entries| {
            *entries = renamed(std::mem::take(entries));
            Ok(())
        })?
    };
    outcome.description = stored
        .iter()
        .find(|entry| entry.tag == new)
        .map(|entry| entry.description.clone())
        .unwrap_or_default();
    Ok(outcome)
}

/// `entries` with `new` on it: an entry of that name gets `description` when one is given; a missing one is added
/// with it (or without a description).
fn with_new(mut entries: Vec<TagEntry>, new: &str, description: Option<&str>) -> Vec<TagEntry> {
    match entries.iter_mut().find(|entry| entry.tag == new) {
        Some(entry) => {
            if let Some(description) = description {
                entry.description = description.to_owned();
            }
        }
        None => entries.push(TagEntry {
            tag: new.to_owned(),
            description: description.unwrap_or_default().to_owned(),
        }),
    }
    entries
}

/// `tag` with its `old` part (the tag itself or a parent of it) replaced by `new`.
fn renamed(tag: &str, old: &str, new: &str) -> String {
    format!("{new}{}", &tag[old.len()..])
}

/// A note's text after a tag rename.
struct TagRewrite {
    text: String,
    /// Uses of the tag rewritten.
    uses: usize,
    /// The frontmatter's `tags:` holds the tag but is written in a form the app does not rewrite, so it kept it.
    kept_key: bool,
}

/// `text` with the tag renamed, and how many uses there were.
fn rewrite_tag(text: &str, old: &str, new: &str) -> TagRewrite {
    let mut edits = Vec::new();
    for (range, raw) in inline_tags(text) {
        if let Some(tag) = normalise(&raw)
            && is_within(&tag, old)
        {
            edits.push((range, renamed(&tag, old, new)));
        }
    }
    let inline = edits.len();
    // The inline tags lie after the frontmatter, so the frontmatter's offsets hold after their edits.
    let text = apply_edits(text, edits);
    let holds_old = |key: &super::detect::TagsKey| {
        key.items
            .iter()
            .any(|item| normalise(&item.value).is_some_and(|tag| is_within(&tag, old)))
    };
    let Some(key) = tags_key(&text) else {
        return TagRewrite {
            text,
            uses: inline,
            kept_key: false,
        };
    };
    // A key the app cannot write back unchanged keeps its old tag; only the text's uses are renamed then.
    if !key.simple {
        let kept_key = holds_old(&key);
        return TagRewrite {
            text,
            uses: inline,
            kept_key,
        };
    }
    let mut uses = 0;
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for item in &key.items {
        let value = match normalise(&item.value) {
            Some(tag) if is_within(&tag, old) => {
                uses += 1;
                renamed(&tag, old, new)
            }
            _ => item.value.clone(),
        };
        if seen.insert(normalise(&value).unwrap_or_else(|| value.clone())) {
            items.push(value);
        }
    }
    if uses == 0 {
        return TagRewrite {
            text,
            uses: inline,
            kept_key: false,
        };
    }
    TagRewrite {
        text: write_tags_key(&text, &items),
        uses: inline + uses,
        kept_key: false,
    }
}

/// The list with `old` (and its nested entries) renamed; a renamed entry whose new name is listed already goes.
fn renamed_list(entries: Vec<TagEntry>, old: &str, new: &str) -> Vec<TagEntry> {
    let keep: HashSet<String> = entries
        .iter()
        .filter(|entry| !is_within(&entry.tag, old))
        .map(|entry| entry.tag.clone())
        .collect();
    let mut out: Vec<TagEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if !is_within(&entry.tag, old) {
            out.push(entry);
            continue;
        }
        let tag = renamed(&entry.tag, old, new);
        if keep.contains(&tag) || out.iter().any(|known| known.tag == tag) {
            continue;
        }
        out.push(TagEntry {
            tag,
            description: entry.description,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn target(old: &str, new: &str, shared: bool, new_shared: bool) -> LinkTarget {
        LinkTarget {
            old_noext: strip_md(old).to_lowercase(),
            old_stem: file_stem(old).to_lowercase(),
            old_stem_shared: shared,
            new_noext: strip_md(new).to_owned(),
            new_name: if new_shared {
                strip_md(new).to_owned()
            } else {
                file_stem(new)
            },
        }
    }

    #[test]
    fn links_keep_their_alias_heading_and_form() {
        let t = target("Dev/Old Note.md", "Dev/New Note.md", false, false);
        let text = "[[Old Note]] [[old note|alias]] [[Old Note#Part|a]] ![[Old Note]] [[Dev/Old Note.md]] \
                    [[Dev/Old Note#h]] [[Other]] `[[Old Note]]` [[ Old Note ]]";
        let (out, count) = rewrite_links(text, "Dev/x.md", &t);
        assert_eq!(
            out,
            "[[New Note]] [[New Note|alias]] [[New Note#Part|a]] ![[New Note]] [[Dev/New Note.md]] \
             [[Dev/New Note#h]] [[Other]] `[[Old Note]]` [[ New Note ]]"
        );
        assert_eq!(count, 7);
    }

    #[test]
    fn a_shared_name_is_left_and_relative_paths_resolve() {
        let shared = target("A/Dup.md", "A/Moved.md", true, false);
        let (out, count) = rewrite_links("[[Dup]] [[A/Dup]] [[../A/Dup]]", "B/x.md", &shared);
        assert_eq!(out, "[[Dup]] [[A/Moved]] [[A/Moved]]");
        assert_eq!(count, 2);
        let new_shared = target("A/Old.md", "A/Common.md", false, true);
        assert_eq!(
            rewrite_links("[[Old]]", "x.md", &new_shared).0,
            "[[A/Common]]"
        );
    }

    #[test]
    fn a_tag_renames_in_text_and_frontmatter_with_its_children() {
        let text =
            "---\ntags: [Rust, rust/async, other, neu]\n---\n#rust #Rust/Async #rusty `#rust`\n";
        let TagRewrite {
            text: out, uses, ..
        } = rewrite_tag(text, "rust", "neu");
        assert_eq!(
            out,
            "---\ntags:\n  - neu\n  - neu/async\n  - other\n---\n#neu #neu/async #rusty `#rust`\n"
        );
        assert_eq!(uses, 4);
        assert_eq!(rewrite_tag("nothing #here", "rust", "neu").uses, 0);
        // A key the app does not rewrite keeps the old tag (and says so); the text's use is renamed all the same.
        let odd = rewrite_tag(
            "---\ntags: [\"rust\", \"my tag\"]\n---\n#rust\n",
            "rust",
            "neu",
        );
        assert_eq!(odd.text, "---\ntags: [\"rust\", \"my tag\"]\n---\n#neu\n");
        assert!(odd.kept_key);
        assert_eq!(odd.uses, 1);
    }

    #[test]
    fn the_list_renames_and_merges() {
        let entry = |tag: &str, d: &str| TagEntry {
            tag: tag.into(),
            description: d.into(),
        };
        let out = renamed_list(
            vec![
                entry("rust", "R"),
                entry("rust/async", "A"),
                entry("neu", "N"),
            ],
            "rust",
            "neu",
        );
        assert_eq!(out, vec![entry("neu/async", "A"), entry("neu", "N")]);
    }

    #[test]
    fn renames_run_over_the_vault_and_a_dry_run_only_counts() {
        let root = unique_temp_dir("axiomata-tags-rename");
        fs::create_dir_all(root.join("Dev")).unwrap();
        fs::write(root.join("Dev/Old.md"), "# Old #rust\n").unwrap();
        fs::write(root.join("a.md"), "[[Old]] and [[Old|o]] #rust\n").unwrap();
        fs::write(root.join("b.md"), "---\ntags: rust\n---\n[[Dev/Old]]\n").unwrap();
        fs::write(root.join("TAGS.md"), "- #rust — Rust\n").unwrap();
        // An instruction file is no note: a rename never writes into it.
        fs::write(root.join("GEMINI.md"), "[[Old]] #rust\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        let dry = rename_note_links(&config, "Dev/Old.md", "Dev/New.md", true).unwrap();
        assert_eq!(dry, RenameCount { files: 2, links: 3 });
        assert!(
            fs::read_to_string(root.join("a.md"))
                .unwrap()
                .contains("[[Old]]")
        );
        fs::rename(root.join("Dev/Old.md"), root.join("Dev/New.md")).unwrap();
        let done = rename_note_links(&config, "Dev/Old.md", "Dev/New.md", false).unwrap();
        assert_eq!(done, dry);
        assert_eq!(
            fs::read_to_string(root.join("a.md")).unwrap(),
            "[[New]] and [[New|o]] #rust\n"
        );

        let tags = rename_tag(&config, "#Rust", "lang/rust", None, false).unwrap();
        assert_eq!(tags.count(), RenameCount { files: 3, links: 3 });
        assert!(!tags.merged);
        assert_eq!(tags.description, "Rust");
        assert_eq!(
            fs::read_to_string(root.join("b.md")).unwrap(),
            "---\ntags:\n  - lang/rust\n---\n[[Dev/New]]\n"
        );
        assert_eq!(list::load(&config).unwrap()[0].tag, "lang/rust");
        assert!(rename_tag(&config, "a", "A", None, true).is_err());
        assert!(rename_note_links(&config, "../x.md", "y.md", true).is_err());
        assert!(
            rename_note_links(&config, "a.md", "b.md", true).is_err(),
            "both exist"
        );
        assert!(
            rename_note_links(&config, "x.md", "y.md", true).is_err(),
            "neither exists"
        );
        assert!(rename_note_links(&config, "Dev/New.md", "Dev/x]]\n# y.md", true).is_err());
        assert_eq!(
            fs::read_to_string(root.join("GEMINI.md")).unwrap(),
            "[[Old]] #rust\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// The owner's cleanup of 2026-10-29, on a fixture: nested targets not yet listed, a merge into a listed tag, a
    /// tag only the notes know, descriptions given or kept.
    #[test]
    fn the_owners_cleanup_renames_nests_and_merges() {
        let root = unique_temp_dir("axiomata-tags-cleanup");
        fs::create_dir_all(root.join("News")).unwrap();
        fs::write(
            root.join("TAGS.md"),
            "- #ki-news — News\n- #ki-modelle — Modelle\n- #tauri — Tauri\n- #rust/gui — GUIs in Rust\n",
        )
        .unwrap();
        fs::write(
            root.join("News/KI-News RSS.md"),
            "---\ntitle: \"KI-News (RSS)\"\ntags: [ki-news]\n---\n\nHeise · [Original](https://x.org) · #ki-news\n",
        )
        .unwrap();
        fs::write(
            root.join("app.md"),
            "---\ntags:\n  - tauri\n  - rust/gui\n---\n#tauri and #tauri/v2\n",
        )
        .unwrap();
        fs::write(root.join("tools.md"), "#rust-tools here\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };

        let dry = rename_tag(&config, "ki-news", "ai/news", None, true).unwrap();
        assert_eq!((dry.files, dry.uses, dry.merged), (1, 2, false));
        assert_eq!(dry.description, "News");
        assert!(
            fs::read_to_string(root.join("TAGS.md"))
                .unwrap()
                .contains("#ki-news")
        );

        let news = rename_tag(&config, "ki-news", "ai/news", None, false).unwrap();
        assert_eq!(news.count(), dry.count());
        assert_eq!(
            fs::read_to_string(root.join("News/KI-News RSS.md")).unwrap(),
            "---\ntitle: \"KI-News (RSS)\"\ntags:\n  - ai/news\n---\n\nHeise · [Original](https://x.org) · #ai/news\n"
        );
        let models = rename_tag(
            &config,
            "ki-modelle",
            "ai/models",
            Some("KI-Modelle"),
            false,
        )
        .unwrap();
        assert_eq!(
            (models.files, models.description.as_str()),
            (0, "KI-Modelle")
        );

        // A merge: the note that had both keeps one, the child follows, the listed entry keeps its description.
        let gui = rename_tag(&config, "#Tauri", "rust/gui", None, false).unwrap();
        assert!(gui.merged);
        assert_eq!((gui.files, gui.uses), (1, 3));
        assert_eq!(gui.description, "GUIs in Rust");
        assert_eq!(
            fs::read_to_string(root.join("app.md")).unwrap(),
            "---\ntags:\n  - rust/gui\n---\n#rust/gui and #rust/gui/v2\n"
        );

        // A tag only the notes know goes onto the list under its new name.
        let tools = rename_tag(
            &config,
            "rust-tools",
            "rust/tools",
            Some("Werkzeuge"),
            false,
        )
        .unwrap();
        assert_eq!((tools.files, tools.merged), (1, false));
        assert_eq!(
            list::load(&config).unwrap(),
            vec![
                TagEntry {
                    tag: "ai/models".into(),
                    description: "KI-Modelle".into()
                },
                TagEntry {
                    tag: "ai/news".into(),
                    description: "News".into()
                },
                TagEntry {
                    tag: "rust/gui".into(),
                    description: "GUIs in Rust".into()
                },
                TagEntry {
                    tag: "rust/tools".into(),
                    description: "Werkzeuge".into()
                },
            ]
        );
        assert!(
            rename_tag(&config, "nowhere", "ai/x", None, true).is_err(),
            "a typo"
        );
        assert!(
            rename_tag(&config, "ai", "ai/old", None, true).is_err(),
            "into itself"
        );
        let described = list::describe(&config, "#AI/News", "KI-Nachrichten").unwrap();
        assert!(described.changed && !described.added);
        assert!(
            list::describe(&config, "system/cli", "Kommandozeile")
                .unwrap()
                .added
        );
        assert!(list::describe(&config, "a b", "x").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
