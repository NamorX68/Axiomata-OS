//! Moving a note's `#tags` out of its text into its frontmatter's `tags:` (owner, 2026-10-29, concept note Q7): the
//! news skills used to write `· #ki-news` into every item and a line `Tags: #a #b` here and there; the vault keeps its
//! tags in the frontmatter now, where the list, the search and Obsidian all read them. A one-time cleanup the owner
//! runs with `axiomata-cli notes tags lift-inline`.
//!
//! A run of `#tag`s at a line's start or end goes, with the space or list comma that belonged to it; a separator left
//! dangling at a line's end (`· #tag`) goes with it, and a line that held nothing but tags (or a `Tags:` label) is
//! removed — one blank line with it, so no double gap is left. A tag inside a sentence keeps its word, without `#`.
//! Code, links, URLs, headings and HTML comments are never touched: the tag scan skips code, links and URLs, and a
//! heading, an indented code line or a comment keeps its tags. Every note is read and written through the workspace's
//! guard, checked against the version read; an instruction file, the tag list and `Kanban/` are left out, as for
//! `notes tag`.

use std::ops::Range;

use serde::Serialize;

use super::detect::{LineIter, inline_tags, is_within, normalise};
use super::note::with_tags;
use super::rename::SkippedNote;
use super::{note_path, refused, workspace};
use crate::config::Config;
use crate::error::AxiomataError;

/// One note whose tags moved (or would move).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiftedNote {
    pub note: String,
    /// The tags moved into the frontmatter, normalised, in the order they first stood in the text.
    pub tags: Vec<String>,
    /// `#tag`s taken out of the text (one inside a sentence keeps its word, without the `#`).
    pub removed: usize,
}

/// What [`lift_inline`] did (or, in a dry run, would do).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Lifted {
    pub notes: Vec<LiftedNote>,
    /// Notes with tags to lift that were left as they were, and why.
    pub skipped: Vec<SkippedNote>,
}

impl Lifted {
    /// Notes rewritten.
    pub fn files(&self) -> usize {
        self.notes.len()
    }

    /// `#tag` tokens taken out of the texts.
    pub fn removed(&self) -> usize {
        self.notes.iter().map(|note| note.removed).sum()
    }
}

/// Moves the inline tags of every note into its frontmatter: those within one of `only` (a tag or a parent of it), or
/// all when `only` is empty.
///
/// # Errors
///
/// A name in `only` is no tag, or the vault cannot be walked. A single note that cannot be lifted is no error: it is
/// listed in [`Lifted::skipped`].
pub fn lift_inline(
    config: &Config,
    only: &[String],
    dry_run: bool,
) -> Result<Lifted, AxiomataError> {
    let mut wanted = Vec::with_capacity(only.len());
    for raw in only {
        let Some(tag) = normalise(raw) else {
            return Err(refused(format!("{raw:?} is no tag")));
        };
        wanted.push(tag);
    }
    let mut out = Lifted::default();
    for rel in crate::memory::walker::file_paths(config)? {
        if !rel.to_lowercase().ends_with(".md") || note_path(&rel).is_err() {
            continue;
        }
        let file = match workspace::read(config, &rel) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(err) => {
                out.skipped.push(SkippedNote {
                    note: rel,
                    reason: err.to_string(),
                });
                continue;
            }
        };
        let Some(lift) = lifted_text(&file.content, &wanted) else {
            continue;
        };
        let text = match lift {
            Ok(lift) => lift,
            Err(err) => {
                out.skipped.push(SkippedNote {
                    note: rel,
                    reason: err.to_string(),
                });
                continue;
            }
        };
        if !dry_run
            && let Err(err) = workspace::write(config, &rel, &text.text, Some(&file.version))
        {
            out.skipped.push(SkippedNote {
                note: rel,
                reason: err.to_string(),
            });
            continue;
        }
        out.notes.push(LiftedNote {
            note: rel,
            tags: text.tags,
            removed: text.removed,
        });
    }
    Ok(out)
}

/// A note's text with its tags lifted.
#[derive(Debug, PartialEq, Eq)]
struct Lift {
    text: String,
    tags: Vec<String>,
    removed: usize,
}

/// `text` with the inline tags within `wanted` (all, when empty) moved into its frontmatter. `None` when it has none
/// to move; an error when its `tags:` is written in a form the app does not rewrite.
fn lifted_text(text: &str, wanted: &[String]) -> Option<Result<Lift, AxiomataError>> {
    let mut tokens: Vec<(Range<usize>, String)> = Vec::new();
    for (range, raw) in inline_tags(text) {
        let Some(tag) = normalise(&raw) else {
            continue;
        };
        if !wanted.is_empty() && !wanted.iter().any(|parent| is_within(&tag, parent)) {
            continue;
        }
        // The token is `#` and its tag, with the `/` the scan leaves out at a tag's end (`#a/`).
        let end = range.end + text[range.end..].bytes().take_while(|b| *b == b'/').count();
        tokens.push((range.start - 1..end, tag));
    }
    let mut tags: Vec<String> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut removed = 0;
    // `text[copied..]` is not yet in `out`: unchanged lines are copied in one piece when the next change comes.
    let mut copied = 0;
    let mut drop_blank = false;
    let mut last_blank = false;
    let mut in_comment = false;
    for line in LineIter::new(text, 0) {
        let content = &text[line.content.clone()];
        if std::mem::take(&mut drop_blank) && content.trim().is_empty() {
            out.push_str(&text[copied..line.start]);
            copied = line.end;
            continue;
        }
        let here: Vec<&(Range<usize>, String)> = tokens
            .iter()
            .filter(|(range, _)| line.content.start <= range.start && range.end <= line.content.end)
            .collect();
        // What the tag scan does not mask but is no prose either: an HTML comment and an indented code block. Their
        // `#` stays as it is (security audit: `#include` in indented code must not lose its `#`).
        let commented = in_comment || content.contains("<!--");
        if commented {
            in_comment = comment_open(content, in_comment);
        }
        if here.is_empty() || commented || is_heading(content) || is_indented_code(content) {
            last_blank = content.trim().is_empty();
            continue;
        }
        for (_, tag) in &here {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        removed += here.len();
        let ranges: Vec<Range<usize>> = here
            .iter()
            .map(|(range, _)| range.start - line.start..range.end - line.start)
            .collect();
        out.push_str(&text[copied..line.start]);
        copied = line.end;
        let cleaned = cleaned_line(content, &ranges);
        // A table row stays a row, however empty: removing it would break the table (a tag in a cell keeps its word).
        if !content.trim_start().starts_with('|') && is_empty_line(&cleaned) {
            // A blank line on both sides would leave a double gap: the one after goes too.
            drop_blank = last_blank || line.start == 0;
        } else {
            out.push_str(&cleaned);
            out.push_str(&text[line.content.end..line.end]);
            last_blank = false;
        }
    }
    if removed == 0 {
        return None;
    }
    out.push_str(&text[copied..]);
    Some(with_tags(&out, &tags).map(|(text, _)| Lift {
        text,
        tags,
        removed,
    }))
}

/// Whether an HTML comment is still open after `line`, given whether one was open before it.
fn comment_open(line: &str, open_before: bool) -> bool {
    let mut open = open_before;
    let mut rest = line;
    loop {
        if open {
            match rest.find("-->") {
                Some(at) => {
                    open = false;
                    rest = &rest[at + 3..];
                }
                None => return true,
            }
        } else {
            match rest.find("<!--") {
                Some(at) => {
                    open = true;
                    rest = &rest[at + 4..];
                }
                None => return false,
            }
        }
    }
}

/// Whether `line` reads as indented code: a tab or four spaces before text that is no list item (a nested list item
/// is indented as much and is prose).
fn is_indented_code(line: &str) -> bool {
    let rest = line.trim_start();
    let indent = &line[..line.len() - rest.len()];
    let deep = indent.starts_with('\t') || indent.chars().take_while(|c| *c == ' ').count() >= 4;
    let list_item = rest.starts_with(['-', '*', '+']) && rest[1..].starts_with([' ', '\t'])
        || rest.split_once(['.', ')']).is_some_and(|(number, after)| {
            !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && after.starts_with(' ')
        });
    deep && !list_item
}

/// Whether `line` is an ATX heading (`# Title`): its tags stay where the owner wrote them.
fn is_heading(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    indent <= 3
        && (1..=6).contains(&hashes)
        && rest[hashes..]
            .chars()
            .next()
            .is_none_or(|c| c == ' ' || c == '\t')
}

/// Characters that only joined a tag to what came before it: at a line's end with the tag gone, they go too.
const TRAILING_SEPARATORS: [char; 8] = ['·', '•', '|', ',', ';', '–', '—', '-'];

/// Characters between two tags that only list them: with both tags gone, they go too.
fn is_glue(segment: &str) -> bool {
    segment
        .chars()
        .all(|c| c.is_whitespace() || c == ',' || c == ';')
}

/// `line` with its lifted `tokens` (sorted byte ranges within it) taken out: a run of tags at the line's start or end
/// goes, with the separators that only joined it; a tag inside a sentence keeps its word and loses only its `#`, so
/// the sentence still reads (`Built with #tauri and Rust` → `Built with tauri and Rust`).
fn cleaned_line(line: &str, tokens: &[Range<usize>]) -> String {
    let count = tokens.len();
    // The text before token `i`, and for `i == count` the text after the last one.
    let gap = |i: usize| {
        let from = if i == 0 { 0 } else { tokens[i - 1].end };
        let to = if i == count {
            line.len()
        } else {
            tokens[i].start
        };
        &line[from..to]
    };
    let starts_line = gap(0)
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '-' | '*' | '+' | '>'));
    let mut out = String::with_capacity(line.len());
    let mut removed: Vec<Range<usize>> = Vec::new();
    let mut from = 0;
    for (i, token) in tokens.iter().enumerate() {
        let leading = starts_line && (1..=i).all(|k| only_tags(gap(k)));
        let trailing = (i + 1..=count).all(|k| only_tags(gap(k)));
        out.push_str(&line[from..token.start]);
        if leading || trailing {
            let at = out.len();
            out.push_str(&line[token.clone()]);
            removed.push(at..out.len());
        } else {
            out.push_str(&line[token.start + 1..token.end]);
        }
        from = token.end;
    }
    out.push_str(&line[from..]);
    if removed.is_empty() {
        out
    } else {
        without_tokens(&out, &removed)
    }
}

/// Whether `segment` holds nothing but list glue and `#tag`s that stay (not lifted): a run of tags goes on through it.
fn only_tags(segment: &str) -> bool {
    segment
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .all(|word| {
            word.is_empty()
                || word
                    .strip_prefix('#')
                    .is_some_and(|tag| normalise(tag).is_some())
        })
}

/// `line` without the `tokens` (sorted byte ranges within it), the spacing around them closed up.
fn without_tokens(line: &str, tokens: &[Range<usize>]) -> String {
    let mut segments: Vec<&str> = Vec::with_capacity(tokens.len() + 1);
    let mut from = 0;
    for token in tokens {
        segments.push(&line[from..token.start]);
        from = token.end;
    }
    segments.push(&line[from..]);
    let last = segments.len() - 1;
    let mut out = segments[0].to_owned();
    for (index, &segment) in segments.iter().enumerate().skip(1) {
        if index == last && segment.trim().is_empty() {
            trim_trailing_separator(&mut out);
            break;
        }
        let segment = if index < last && is_glue(segment) {
            ""
        } else {
            segment
        };
        if out.trim().is_empty() {
            // Only indentation before the tag: the text after it moves up to it, without a separator that joined it.
            let next = segment.trim_start();
            let next = match next.strip_prefix(['·', '•', '|', ',', ';', '–', '—']) {
                Some(rest) if !rest.trim().is_empty() => rest.trim_start(),
                _ => next,
            };
            out.push_str(next);
        } else if out.ends_with(char::is_whitespace) {
            // The space before the tag stays as the one between the words; a comma or stop closes up to the word.
            let next = segment.trim_start();
            if next.starts_with([',', '.', ';', ':', '!', '?', ')']) {
                out.truncate(out.trim_end().len());
            }
            out.push_str(next);
        } else {
            out.push_str(segment);
        }
    }
    out
}

/// `out` without its trailing spaces and the one separator before them that joined a tag now gone (`a · ` → `a`).
fn trim_trailing_separator(out: &mut String) {
    out.truncate(out.trim_end().len());
    if let Some(last) = out.chars().next_back()
        && TRAILING_SEPARATORS.contains(&last)
    {
        let before = &out[..out.len() - last.len_utf8()];
        // `-` joins only with a space before it (`a -`); in `x-` it belongs to the word.
        if last != '-' || before.ends_with(char::is_whitespace) || before.is_empty() {
            out.truncate(before.trim_end().len());
        }
    }
}

/// Labels a line of tags may start with; left alone, the label is the whole line and goes too.
const TAG_LABELS: [&str; 6] = [
    "tags",
    "tag",
    "schlagwörter",
    "schlagworte",
    "stichwörter",
    "labels",
];

/// Whether what is left of a line after its tags went says nothing any more: blank, a bare list marker or quote sign,
/// a lone separator, or a `Tags:` label.
fn is_empty_line(line: &str) -> bool {
    let bare = line.trim().trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '-' | '*' | '+' | '>' | '_' | ':' | '·' | '•' | '|' | ',')
    });
    bare.is_empty() || TAG_LABELS.contains(&bare.to_lowercase().as_str())
}

#[cfg(test)]
// The helpers take a token list; a list of one token is what these cases mean, not a range of numbers.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn lift(text: &str, wanted: &[&str]) -> Lift {
        let wanted: Vec<String> = wanted.iter().map(|tag| (*tag).to_owned()).collect();
        lifted_text(text, &wanted)
            .expect("tags to lift")
            .expect("liftable")
    }

    #[test]
    fn a_news_item_loses_its_trailing_tag_and_separator() {
        let text = "---\ntitle: \"KI-News (RSS)\"\ntags: [ki-news]\n---\n\n# KI-News (RSS)\n\n### A title\n\n\
                    Heise · 2026-10-28 · [Original](https://example.org/a#b) · #ki-news\n\nZwei Sätze.\n";
        let out = lift(text, &[]);
        assert_eq!(
            out.text,
            "---\ntitle: \"KI-News (RSS)\"\ntags: [ki-news]\n---\n\n# KI-News (RSS)\n\n### A title\n\n\
             Heise · 2026-10-28 · [Original](https://example.org/a#b)\n\nZwei Sätze.\n"
        );
        assert_eq!(out.tags, vec!["ki-news"]);
        assert_eq!(out.removed, 1);
    }

    #[test]
    fn a_line_of_tags_goes_with_one_blank_line() {
        let text = "# Note\n\nText.\n\nTags: #ki-news #KI-Modelle\n\nMore.\n";
        let out = lift(text, &[]);
        assert_eq!(
            out.text,
            "---\ntags:\n  - ki-news\n  - ki-modelle\n---\n# Note\n\nText.\n\nMore.\n"
        );
        assert_eq!(out.removed, 2);
        let list = lift("a\n- #x, #z\nb\n", &[]);
        assert_eq!(list.text, "---\ntags:\n  - x\n  - z\n---\na\nb\n");
        let first = lift("#solo\n\nBody\n", &[]);
        assert_eq!(first.text, "---\ntags:\n  - solo\n---\nBody\n");
    }

    #[test]
    fn a_tag_inside_a_sentence_keeps_its_word() {
        let line = "Built with #Tauri and #rust. #a #b";
        assert_eq!(
            cleaned_line(line, &[11..17, 22..27, 29..31, 32..34]),
            "Built with Tauri and rust."
        );
        assert_eq!(
            cleaned_line("#ki-news · Heise · date", &[0..8]),
            "Heise · date"
        );
        assert_eq!(
            cleaned_line("> #a #b text #c", &[2..4, 5..7, 13..15]),
            "> text"
        );
        let out = lift("Built with #tauri and `#code`.\n", &[]);
        assert_eq!(
            out.text,
            "---\ntags:\n  - tauri\n---\nBuilt with tauri and `#code`.\n"
        );
    }

    #[test]
    fn comments_indented_code_and_table_rows_are_kept() {
        let text = "<!-- #a -->\n<!--\n#b\n-->\n\n    #include x\n\n    - nested #c\n| x | #d |\n";
        let out = lift(text, &[]);
        assert_eq!(
            out.text,
            "---\ntags:\n  - c\n  - d\n---\n<!-- #a -->\n<!--\n#b\n-->\n\n    #include x\n\n    - nested\n| x | d |\n"
        );
        assert!(comment_open("a <!-- b", false));
        assert!(!comment_open("<!-- b --> c", false));
        assert!(!comment_open("x --> <!-- y -->", true));
    }

    #[test]
    fn prose_closes_up_around_a_tag() {
        assert_eq!(
            without_tokens("I like #rust, and #go a lot", &[7..12, 18..21]),
            "I like, and a lot"
        );
        assert_eq!(without_tokens("  #a text", &[2..4]), "  text");
        assert_eq!(without_tokens("- #a text", &[2..4]), "- text");
        assert_eq!(without_tokens("well-#a", &[5..7]), "well-");
        assert_eq!(without_tokens("x - #a", &[4..6]), "x");
        assert_eq!(without_tokens("| a | #t |", &[6..8]), "| a | |");
    }

    #[test]
    fn code_links_urls_and_headings_are_left() {
        let text =
            "## Heading #keep\n`#code` [[Note#Part]] https://x.org/#frag #go\n```\n#fenced\n```\n";
        let out = lift(text, &[]);
        assert_eq!(
            out.text,
            "---\ntags:\n  - go\n---\n## Heading #keep\n`#code` [[Note#Part]] https://x.org/#frag\n```\n#fenced\n```\n"
        );
        assert_eq!(out.tags, vec!["go"]);
    }

    #[test]
    fn only_the_given_tags_and_their_children_move() {
        let out = lift(
            "A #ki-news #rust/async #rustacean #other\n",
            &["rust", "ki-news"],
        );
        assert_eq!(
            out.text,
            "---\ntags:\n  - ki-news\n  - rust/async\n---\nA #rustacean #other\n"
        );
        assert!(lifted_text("#other only\n", &["rust".to_owned()]).is_none());
    }

    #[test]
    fn a_tag_already_in_the_frontmatter_is_not_listed_twice_and_an_odd_key_refuses() {
        let out = lift("---\ntags:\n  - ki-news\n---\nx · #ki-news\n", &[]);
        assert_eq!(out.text, "---\ntags:\n  - ki-news\n---\nx\n");
        let odd = lifted_text("---\ntags: [a] # mine\n---\nx #b\n", &[]).unwrap();
        assert!(odd.is_err());
    }

    #[test]
    fn the_vault_run_lifts_skips_protected_files_and_a_dry_run_writes_nothing() {
        let root = unique_temp_dir("axiomata-tags-lift");
        fs::create_dir_all(root.join("News")).unwrap();
        fs::create_dir_all(root.join("Kanban")).unwrap();
        let news = "---\ntitle: \"KI-Modelle\"\ntags: [ki-modelle]\n---\n\n# KI-Modelle\n\n<!-- NEU-HIER -->\n\n\
                    ## 2026-10-28 09:00\n\n### Model X\n\nAcme · LLM · 2026-10-27 · [Modell](https://hf.co/x) · \
                    #ki-modelle\n\nText.\n";
        fs::write(root.join("News/KI-Modelle.md"), news).unwrap();
        fs::write(root.join("Kanban/1-Board.md"), "card #ki-news\n").unwrap();
        fs::write(root.join("AGENTS.md"), "router #ki-news\n").unwrap();
        fs::write(root.join("odd.md"), "---\ntags: [a] # mine\n---\nx #b\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        let dry = lift_inline(&config, &[], true).unwrap();
        assert_eq!(dry.files(), 1);
        assert_eq!(dry.removed(), 1);
        assert_eq!(dry.skipped.len(), 1, "{:?}", dry.skipped);
        assert_eq!(dry.skipped[0].note, "odd.md");
        assert_eq!(
            fs::read_to_string(root.join("News/KI-Modelle.md")).unwrap(),
            news
        );
        let done = lift_inline(&config, &[], false).unwrap();
        assert_eq!(done, dry);
        let lifted = fs::read_to_string(root.join("News/KI-Modelle.md")).unwrap();
        assert!(
            lifted.contains("[Modell](https://hf.co/x)\n\nText."),
            "{lifted}"
        );
        assert!(!lifted.contains("#ki-modelle"));
        assert_eq!(
            fs::read_to_string(root.join("Kanban/1-Board.md")).unwrap(),
            "card #ki-news\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "router #ki-news\n"
        );
        assert!(lift_inline(&config, &["not a tag".to_owned()], true).is_err());
        assert!(
            lift_inline(&config, &[], true).unwrap().notes.is_empty(),
            "nothing left"
        );
        let _ = fs::remove_dir_all(root);
    }
}
