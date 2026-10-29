//! Finding tags in a note the way Obsidian does (`docs/plans/tags-links.md` Q1): `#tag` in the text and `tags:` in the
//! frontmatter (a YAML list, a flow list or comma/space-separated text), nested as `#a/b`, compared without case.
//!
//! Not a tag: the `#` of a heading (`# Title` — the `#` is followed by a space), anything in code (a fenced block or an
//! inline span), a `#` inside a word, a URL or a link (`[[Note#Heading]]`, `site/#frag` — no space before it), and a
//! number alone (`#1`). The scan is by bytes and lines, without a Markdown parser: what it skips is exactly the list
//! above, so a reader of a note without the app (Obsidian) sees the same tags.

use std::ops::Range;

/// The longest tag kept; a longer run of tag characters is no tag anyone typed on purpose.
pub const MAX_TAG_CHARS: usize = 64;

/// A tag as the vault stores it: lower case, without `#`, nested parts kept as `a/b`. `None` when `raw` is no tag:
/// empty, longer than [`MAX_TAG_CHARS`], a character other than a letter, a digit, `_`, `-` or `/`, only digits, a
/// first character that is no letter or digit (`-x` would read as a YAML list in the frontmatter), or a `/` at the end
/// or twice in a row.
pub fn normalise(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let bare = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if bare.is_empty() || bare.chars().count() > MAX_TAG_CHARS {
        return None;
    }
    if !bare.chars().all(is_tag_char) {
        return None;
    }
    if bare.chars().all(|c| c.is_ascii_digit() || c == '/') {
        return None;
    }
    if !bare.chars().next().is_some_and(char::is_alphanumeric) {
        return None;
    }
    if bare.starts_with('/') || bare.ends_with('/') || bare.contains("//") {
        return None;
    }
    Some(bare.to_lowercase())
}

/// Whether `c` may stand in a tag (Obsidian's set: letters of any script, digits, `_`, `-`, `/`).
pub fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Whether `tag` is `parent` or lies below it (`rust/async` under `rust`). Both normalised.
pub fn is_within(tag: &str, parent: &str) -> bool {
    tag == parent
        || tag
            .strip_prefix(parent)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The tags of a note: its frontmatter's and its text's, normalised, without duplicates, sorted.
pub fn note_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = frontmatter_items(text)
        .into_iter()
        .filter_map(|item| normalise(&item.value))
        .chain(
            inline_tags(text)
                .into_iter()
                .filter_map(|(_, raw)| normalise(&raw)),
        )
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// The frontmatter block of `text`: the byte range of its lines between the opening `---` and the closing `---` (or
/// `...`), and the range of the whole block including both fences. `None` without one at the very top (a BOM aside).
pub(crate) fn frontmatter(text: &str) -> Option<Frontmatter> {
    let start = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let mut lines = LineIter::new(text, start);
    let first = lines.next()?;
    if text[first.content.clone()].trim_end() != "---" {
        return None;
    }
    let body_start = first.end;
    for line in lines {
        let content = text[line.content.clone()].trim_end();
        if content == "---" || content == "..." {
            return Some(Frontmatter {
                block: start..line.end,
                body: body_start..line.start,
                close_start: line.start,
            });
        }
    }
    None
}

/// Where a note's frontmatter lies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    /// From the opening fence to the end of the closing fence's line.
    pub block: Range<usize>,
    /// The lines between the fences.
    pub body: Range<usize>,
    /// Where the closing fence's line starts: a new key goes in before it.
    pub close_start: usize,
}

/// One value of the frontmatter's `tags:` (or `tag:`) key, as written (quotes and a leading `#` removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrontmatterItem {
    pub value: String,
}

/// Where the `tags:` key of a frontmatter lies: its own line and the list lines that belong to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TagsKey {
    /// From the start of the key's line to the end of its last item line.
    pub lines: Range<usize>,
    pub items: Vec<FrontmatterItem>,
    /// Whether the key is written in a form the app can write back without losing anything: a block list or a
    /// one-line list of plain or simply quoted items, no comment, no multi-line flow list. Only such a key is
    /// rewritten (security audit: the owner's frontmatter must never be damaged).
    pub simple: bool,
}

/// The frontmatter's `tags:` key, when it has one.
pub(crate) fn tags_key(text: &str) -> Option<TagsKey> {
    let fm = frontmatter(text)?;
    let mut lines = LineIter::new(text, fm.body.start).take_while(|l| l.start < fm.body.end);
    while let Some(line) = lines.next() {
        let content = &text[line.content.clone()];
        let Some((key, value)) = content.split_once(':') else {
            continue;
        };
        // A key starts in the first column; an indented one belongs to another key's value.
        if key.starts_with(char::is_whitespace) {
            continue;
        }
        let key = key.trim();
        if !(key.eq_ignore_ascii_case("tags") || key.eq_ignore_ascii_case("tag")) {
            continue;
        }
        let value = value.trim();
        let mut end = line.end;
        let mut items = Vec::new();
        let mut simple = true;
        if value.is_empty() {
            // A block list: the indented `- item` lines (and blank lines between them) that follow.
            for next in lines.by_ref() {
                let item_line = &text[next.content.clone()];
                let trimmed = item_line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Some(item) = trimmed.strip_prefix('-') else {
                    break;
                };
                if !(item_line.starts_with(char::is_whitespace) || item_line.starts_with('-')) {
                    break;
                }
                end = next.end;
                simple &= simple_item(item.trim(), true);
                push_item(&mut items, item);
            }
        } else {
            let flow = value.strip_prefix('[');
            let inner = match flow {
                Some(rest) => match rest.strip_suffix(']') {
                    Some(inner) => inner,
                    None => {
                        simple = false;
                        rest
                    }
                },
                None => value,
            };
            for part in inner.split(',') {
                let part = part.trim();
                // Plain text may separate by spaces too; a flow item with a space is one item the split would cut.
                simple &= part.is_empty() && flow.is_some()
                    || part.split(' ').all(|word| simple_item(word, false));
                if flow.is_some() && part.contains(' ') {
                    simple = false;
                }
            }
            for part in inner.split([',', ' ']) {
                push_item(&mut items, part);
            }
        }
        // A second `tags:`/`tag:` key would be left beside the rewritten one: never write such a frontmatter.
        if tag_keys(text, &fm) > 1 {
            simple = false;
        }
        return Some(TagsKey {
            lines: line.start..end,
            items,
            simple,
        });
    }
    None
}

/// How many `tags:` or `tag:` keys the frontmatter holds in its first column.
fn tag_keys(text: &str, fm: &Frontmatter) -> usize {
    LineIter::new(text, fm.body.start)
        .take_while(|l| l.start < fm.body.end)
        .filter(|line| {
            let content = &text[line.content.clone()];
            content.split_once(':').is_some_and(|(key, _)| {
                !key.starts_with(char::is_whitespace)
                    && (key.trim().eq_ignore_ascii_case("tags")
                        || key.trim().eq_ignore_ascii_case("tag"))
            })
        })
        .count()
}

/// Whether one written item reads back as itself: plain (no `:`, `#`, quote or YAML indicator) or, where `quoted_ok`,
/// in one pair of quotes with nothing special inside.
fn simple_item(raw: &str, quoted_ok: bool) -> bool {
    if raw.is_empty() {
        return false;
    }
    let plain = |text: &str| {
        !text.contains([':', '#', '"', '\'', '\\', '[', ']', '{', '}', ','])
            && !text.starts_with(['&', '*', '!', '|', '>', '%', '@', '`', '-', '?'])
    };
    for quote in ['"', '\''] {
        if let Some(inner) = raw.strip_prefix(quote).and_then(|r| r.strip_suffix(quote)) {
            return quoted_ok
                && !inner.is_empty()
                && !inner.contains(['"', '\'', '\\'])
                && !inner.contains(": ");
        }
    }
    plain(raw)
}

fn push_item(items: &mut Vec<FrontmatterItem>, raw: &str) {
    let value = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .trim_start_matches('#');
    if !value.is_empty() {
        items.push(FrontmatterItem {
            value: value.to_owned(),
        });
    }
}

fn frontmatter_items(text: &str) -> Vec<FrontmatterItem> {
    tags_key(text).map(|key| key.items).unwrap_or_default()
}

/// Every `#tag` in the text outside the frontmatter and code, as the byte range of the characters after `#` and the
/// raw text there (a trailing `/` left out). The raw text still has to pass [`normalise`].
pub fn inline_tags(text: &str) -> Vec<(Range<usize>, String)> {
    let skip_until = frontmatter(text).map_or(0, |fm| fm.block.end);
    let mut out = Vec::new();
    for (line, mut masked) in prose_lines(text, skip_until) {
        masked.extend(link_spans(text, line.clone()));
        let masked = merged(masked);
        let bytes = text.as_bytes();
        let mut i = line.start;
        while i < line.end {
            if bytes[i] != b'#' || inside(&masked, i) {
                i += 1;
                continue;
            }
            let before = text[..i].chars().next_back();
            if !(i == line.start || before.is_some_and(char::is_whitespace)) {
                i += 1;
                continue;
            }
            let start = i + 1;
            let len = text[start..line.end]
                .char_indices()
                .find(|(_, c)| !is_tag_char(*c))
                .map_or(line.end - start, |(at, _)| at);
            let raw = text[start..start + len].trim_end_matches('/');
            if !raw.is_empty() && normalise(raw).is_some() {
                out.push((start..start + raw.len(), raw.to_owned()));
            }
            i = start + len.max(1);
        }
    }
    out
}

/// Every `[[…]]` outside the frontmatter and code: the byte range of its inner text (between the brackets).
pub fn wikilinks(text: &str) -> Vec<Range<usize>> {
    let skip_until = frontmatter(text).map_or(0, |fm| fm.block.end);
    let mut out = Vec::new();
    for (line, masked) in prose_lines(text, skip_until) {
        let mut from = line.start;
        while let Some(at) = text[from..line.end].find("[[") {
            let open = from + at;
            let inner = open + 2;
            let Some(close) = text[inner..line.end].find("]]") else {
                break;
            };
            let end = inner + close;
            if !inside(&masked, open) && end > inner {
                out.push(inner..end);
            }
            from = end + 2;
        }
    }
    out
}

/// The lines of `text` from `from` on that are prose (no fenced code), each with its inline code spans.
fn prose_lines(text: &str, from: usize) -> Vec<(Range<usize>, Vec<Range<usize>>)> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for line in LineIter::new(text, from) {
        let content = &text[line.content.clone()];
        let indent = content.len() - content.trim_start_matches(' ').len();
        let rest = &content[indent..];
        if indent <= 3 {
            let marker = rest.bytes().next();
            if let Some(m @ (b'`' | b'~')) = marker {
                let run = rest.bytes().take_while(|b| *b == m).count();
                if run >= 3 {
                    match fence {
                        None => {
                            fence = Some((m, run));
                            continue;
                        }
                        Some((open, len))
                            if open == m && run >= len && rest[run..].trim().is_empty() =>
                        {
                            fence = None;
                            continue;
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        if fence.is_some() {
            continue;
        }
        let code = code_spans(text, line.content.clone());
        out.push((line.content, code));
    }
    out
}

/// `spans` sorted by start, overlapping ones joined: what [`inside`] searches.
fn merged(mut spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    spans.sort_by_key(|span| span.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(spans.len());
    for span in spans {
        match out.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => out.push(span),
        }
    }
    out
}

/// Whether `at` lies in one of the sorted, disjoint `spans` — a binary search, so a line of thousands of `#` and
/// spans stays linear-logarithmic rather than quadratic (security audit).
fn inside(spans: &[Range<usize>], at: usize) -> bool {
    let index = spans.partition_point(|span| span.end <= at);
    spans.get(index).is_some_and(|span| span.contains(&at))
}

/// How many bytes the search for closing backticks may look at per byte of a line: a line of ever longer backtick
/// runs would make it quadratic; past the budget the rest of the line has no code spans.
const CODE_SCAN_BUDGET: usize = 8;

/// Inline code spans on one line (a run of N backticks to the next run of exactly N), as absolute byte ranges.
fn code_spans(text: &str, line: Range<usize>) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut budget = (line.end - line.start).saturating_mul(CODE_SCAN_BUDGET);
    let mut i = line.start;
    while i < line.end {
        if bytes[i] == b'`' {
            let run = bytes[i..line.end]
                .iter()
                .take_while(|b| **b == b'`')
                .count();
            let mut j = i + run;
            let mut closed = None;
            while j < line.end {
                if budget == 0 {
                    return spans;
                }
                budget -= 1;
                if bytes[j] == b'`' {
                    let other = bytes[j..line.end]
                        .iter()
                        .take_while(|b| **b == b'`')
                        .count();
                    budget = budget.saturating_sub(other);
                    if other == run {
                        closed = Some(j + other);
                        break;
                    }
                    j += other;
                } else {
                    j += 1;
                }
            }
            match closed {
                Some(end) => {
                    spans.push(i..end);
                    i = end;
                }
                None => i += run,
            }
            continue;
        }
        i += 1;
    }
    spans
}

/// The insides of `[[…]]` and the destinations of `[text](…)` on one line, as absolute byte ranges: a `#` there is
/// part of a link (`[[Note #x]]`, `(page #frag)`), never a tag.
fn link_spans(text: &str, line: Range<usize>) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let slice = &text[line.clone()];
    let mut from = 0;
    while let Some(at) = slice[from..].find("[[") {
        let open = from + at;
        let Some(close) = slice[open..].find("]]") else {
            break;
        };
        spans.push(line.start + open..line.start + open + close + 2);
        from = open + close + 2;
    }
    let mut from = 0;
    while let Some(at) = slice[from..].find("](") {
        let open = from + at;
        let Some(close) = slice[open..].find(')') else {
            break;
        };
        spans.push(line.start + open..line.start + open + close + 1);
        from = open + close + 1;
    }
    spans
}

/// One line of a text: where it starts, its content without the line break, and where the next one starts.
#[derive(Debug, Clone)]
pub(crate) struct Line {
    pub start: usize,
    pub content: Range<usize>,
    pub end: usize,
}

/// The lines of a text from a byte offset on; `\n` and `\r\n` both end a line.
pub(crate) struct LineIter<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> LineIter<'a> {
    pub(crate) fn new(text: &'a str, at: usize) -> Self {
        Self { text, at }
    }
}

impl Iterator for LineIter<'_> {
    type Item = Line;

    fn next(&mut self) -> Option<Line> {
        if self.at >= self.text.len() {
            return None;
        }
        let start = self.at;
        let (content_end, end) = match self.text[start..].find('\n') {
            Some(at) => {
                let nl = start + at;
                let content_end = if nl > start && self.text.as_bytes()[nl - 1] == b'\r' {
                    nl - 1
                } else {
                    nl
                };
                (content_end, nl + 1)
            }
            None => (self.text.len(), self.text.len()),
        };
        self.at = end;
        Some(Line {
            start,
            content: start..content_end,
            end,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline(text: &str) -> Vec<String> {
        inline_tags(text).into_iter().map(|(_, raw)| raw).collect()
    }

    #[test]
    fn normalises_like_obsidian() {
        assert_eq!(normalise("#Rust"), Some("rust".into()));
        assert_eq!(normalise("rust/Async"), Some("rust/async".into()));
        assert_eq!(normalise("Übersicht_2-a"), Some("übersicht_2-a".into()));
        assert_eq!(normalise("y1984"), Some("y1984".into()));
        for bad in [
            "", "#", "#1", "2024", "a b", "a.b", "/a", "a/", "a//b", "a#b", "1/2",
        ] {
            assert_eq!(normalise(bad), None, "{bad:?}");
        }
        assert_eq!(normalise(&"a".repeat(MAX_TAG_CHARS + 1)), None);
    }

    #[test]
    fn finds_inline_tags_but_not_headings_numbers_or_words() {
        assert_eq!(
            inline(
                "# Heading\n## Sub #todo\n#Rust and #rust/async, #ki-news.\nissue #12 and a#b and C#\n#Über"
            ),
            vec!["todo", "Rust", "rust/async", "ki-news", "Über"]
        );
    }

    #[test]
    fn skips_code_links_urls_and_escapes() {
        let text = "`#not` and ``a #nope b`` but #yes\n\
                    ```rust\n#[derive(Debug)] #inside\n```\n\
                    ~~~\n#tilde\n~~~\n\
                    [[Note#Heading]] [[Note #x]] [a](https://x.y/ #frag) https://x.y/#frag \\#esc &#123;\n\
                    after #done";
        assert_eq!(inline(text), vec!["yes", "done"]);
    }

    #[test]
    fn an_unclosed_fence_hides_the_rest_and_a_trailing_slash_is_dropped() {
        assert_eq!(inline("#a/ text\n```\n#b"), vec!["a"]);
    }

    #[test]
    fn reads_the_frontmatter_forms() {
        let list =
            "---\ntitle: X\ntags:\n  - Rust\n  - \"#ki-news\"\n\n  - a/b\nother: 1\n---\n# X\n";
        assert_eq!(note_tags(list), vec!["a/b", "ki-news", "rust"]);
        assert_eq!(
            note_tags("---\ntags: [one, Two]\n---\n"),
            vec!["one", "two"]
        );
        assert_eq!(
            note_tags("---\nTags: one, two three\n---\n"),
            vec!["one", "three", "two"]
        );
        assert_eq!(note_tags("---\ntag: solo\n---\n"), vec!["solo"]);
        assert_eq!(
            note_tags("\u{feff}---\r\ntags: [x]\r\n---\r\nbody #y\r\n"),
            vec!["x", "y"]
        );
        // No frontmatter at the top: a `tags:` line further down is text.
        assert_eq!(
            note_tags("text\n---\ntags: [x]\n---\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn only_plain_forms_count_as_simple() {
        let simple = |fm: &str| tags_key(&format!("---\n{fm}\n---\n")).unwrap().simple;
        assert!(simple("tags:\n  - a\n  - \"#b\""));
        assert!(simple("tags: [a, b/c]"));
        assert!(simple("tags: a, b c"));
        assert!(simple("tags: []"));
        for odd in [
            "tags: [a, \"b c\"]",
            "tags: [a] # note",
            "tags: [a,",
            "tags:\n  - \"x: y\"",
            "tags:\n  - x: y",
            "tags: \"a\"",
            "tags: |",
            "tag: x\ntags: [y]",
        ] {
            assert!(!simple(odd), "{odd:?}");
        }
    }

    #[test]
    fn the_frontmatter_is_no_text_and_tags_are_united() {
        let text = "---\ntags: [rust]\nnote: \"#hidden\"\n---\n#Rust #extra\n";
        assert_eq!(note_tags(text), vec!["extra", "rust"]);
    }

    #[test]
    fn wikilinks_are_found_outside_code_only() {
        let text = "[[A]] `[[B]]` [[C|c]]\n```\n[[D]]\n```\n[[E#h]] [[]]";
        let found: Vec<&str> = wikilinks(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, vec!["A", "C|c", "E#h"]);
    }

    #[test]
    fn hostile_lines_stay_fast() {
        let many = "`a`# ".repeat(50_000);
        let started = std::time::Instant::now();
        assert!(inline_tags(&many).is_empty());
        let runs: String = (1..2000).map(|n| "`".repeat(n) + "x").collect();
        let _ = inline_tags(&runs);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn nesting_is_a_prefix_by_parts() {
        assert!(is_within("rust", "rust"));
        assert!(is_within("rust/async", "rust"));
        assert!(!is_within("rusty", "rust"));
    }
}
