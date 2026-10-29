//! The vault's one tag list, `<workspace>/TAGS.md` (`docs/plans/tags-links.md` Q6, V1): a line per tag,
//! `- #tag — description`, readable without the app and read by the agents, but written only by the app (Settings →
//! Workspace → Tags, or `axiomata-cli notes tags rename|describe`) — "nothing by hand in files". A list that is not
//! there yet starts as the news skills' two tags (V4) and is written the first time it is asked for, so an agent that
//! reads the file finds it.

use serde::{Deserialize, Serialize};

use super::detect::{LineIter, normalise};
use super::{TAGS_FILE, refused, workspace};
use crate::config::Config;
use crate::error::AxiomataError;

/// The most tags the list holds: far above the 30–60 the vault aims at (Q7), low enough that a run gone wrong cannot
/// make a file no one can read.
pub const MAX_TAGS: usize = 500;

/// The longest description kept, in characters: one line of the settings' list.
pub const MAX_DESCRIPTION_CHARS: usize = 160;

/// One tag of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagEntry {
    /// Normalised: lower case, without `#`.
    pub tag: String,
    pub description: String,
}

/// The list a vault starts with (V4): the tags the news skills write into their notes' frontmatter. The names follow
/// the vault's nested scheme (owner, 2026-10-29: `#ki-news` became `ai/news`, `#ki-modelle` `ai/models`); the
/// descriptions are German like the rest of the owner's list.
pub fn seed() -> Vec<TagEntry> {
    vec![
        TagEntry {
            tag: "ai/models".into(),
            description:
                "Neue KI-Modelle und ihre Veröffentlichungen, gesammelt vom Skill model-news".into(),
        },
        TagEntry {
            tag: "ai/news".into(),
            description: "KI-Nachrichten, gesammelt von den News-Skills (RSS und Web)".into(),
        },
    ]
}

const HEADER: &str = "# Tags\n\n\
The vault's tag list, kept by Axiomata-OS (Settings → Workspace → Tags): change it there, not here.\n\
Agents tag notes from this list only; a tag that is not on it waits for the owner as a proposal.\n\n";

/// `TAGS.md` as the app writes it.
pub fn render(entries: &[TagEntry]) -> String {
    let mut out = String::from(HEADER);
    for entry in entries {
        if entry.description.is_empty() {
            out.push_str(&format!("- #{}\n", entry.tag));
        } else {
            out.push_str(&format!("- #{} — {}\n", entry.tag, entry.description));
        }
    }
    out
}

/// The entries of a `TAGS.md`: every line `- #tag` with an optional description after ` — ` (or ` – `, ` - `, `: `).
/// Lines that are none are skipped, and so is a tag listed twice (the first one stays).
pub fn parse(text: &str) -> Vec<TagEntry> {
    let mut out: Vec<TagEntry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in LineIter::new(text, 0) {
        if out.len() == MAX_TAGS {
            break;
        }
        let content = text[line.content].trim();
        let Some(rest) = content
            .strip_prefix("- #")
            .or_else(|| content.strip_prefix("* #"))
        else {
            continue;
        };
        let end = rest
            .char_indices()
            .find(|(_, c)| !super::detect::is_tag_char(*c))
            .map_or(rest.len(), |(at, _)| at);
        let Some(tag) = normalise(&rest[..end]) else {
            continue;
        };
        let tail = rest[end..].trim_start();
        let description = ["—", "–", "-", ":"]
            .iter()
            .find_map(|sep| tail.strip_prefix(sep))
            .unwrap_or(tail);
        if !seen.insert(tag.clone()) {
            continue;
        }
        out.push(TagEntry {
            tag,
            description: one_line(description),
        });
    }
    out
}

/// `text` on one line, at most [`MAX_DESCRIPTION_CHARS`]: every control character and line break a space, and only
/// letters, digits, marks of punctuation and a few signs kept — no invisible format characters (zero-width, bidi
/// overrides, Unicode tag characters), which an agent's proposal could hide instructions in for every later agent
/// that reads `TAGS.md` (security audit).
pub fn one_line(text: &str) -> String {
    let flat: String = text
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() || c.is_control() {
                Some(' ')
            } else {
                visible(c).then_some(c)
            }
        })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(MAX_DESCRIPTION_CHARS).collect()
}

/// Whether `c` is a character a description may hold: a letter or digit, ASCII punctuation, or one of the usual
/// typographic marks.
fn visible(c: char) -> bool {
    c.is_alphanumeric() || c.is_ascii_punctuation() || "—–…„“”‘’‚«»·•€£°§×→←↔✓".contains(c)
}

/// The vault's tag list. A vault without a `TAGS.md` gets the [`seed`], written there now.
///
/// # Errors
///
/// The workspace cannot be read or written, or `TAGS.md` is no regular file of its own (a link is refused).
pub fn load(config: &Config) -> Result<Vec<TagEntry>, AxiomataError> {
    match workspace::read(config, TAGS_FILE)? {
        Some(file) => Ok(parse(&file.content)),
        None => {
            let entries = seed();
            workspace::write(config, TAGS_FILE, &render(&entries), None)?;
            Ok(entries)
        }
    }
}

/// The vault's tag list without writing anything: a vault without a `TAGS.md` reads as the [`seed`] (a dry run must
/// not create the file — security audit).
///
/// # Errors
///
/// As [`load`], short of the write.
pub fn current(config: &Config) -> Result<Vec<TagEntry>, AxiomataError> {
    Ok(workspace::read(config, TAGS_FILE)?.map_or_else(seed, |file| parse(&file.content)))
}

/// Checks and orders a list the owner saved: every tag normalised, none twice, descriptions on one line, at most
/// [`MAX_TAGS`]. Sorted by tag, so the file reads like an index.
///
/// # Errors
///
/// A tag that is no tag, a tag listed twice, or more than [`MAX_TAGS`] of them.
pub fn validated(entries: Vec<TagEntry>) -> Result<Vec<TagEntry>, AxiomataError> {
    if entries.len() > MAX_TAGS {
        return Err(refused(format!(
            "the tag list holds at most {MAX_TAGS} tags"
        )));
    }
    let mut out: Vec<TagEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(tag) = normalise(&entry.tag) else {
            return Err(refused(format!(
                "{:?} is no tag: letters, digits, _ - and / only, not only digits",
                entry.tag
            )));
        };
        if out.iter().any(|known| known.tag == tag) {
            return Err(refused(format!("#{tag} is on the list twice")));
        }
        out.push(TagEntry {
            tag,
            description: one_line(&entry.description),
        });
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    Ok(out)
}

/// Writes the owner's list to `TAGS.md` and returns it as stored.
///
/// # Errors
///
/// As [`validated`], and when the file cannot be written through the workspace's guard.
pub fn save(config: &Config, entries: Vec<TagEntry>) -> Result<Vec<TagEntry>, AxiomataError> {
    let entries = validated(entries)?;
    workspace::write(config, TAGS_FILE, &render(&entries), None)?;
    Ok(entries)
}

/// Changes the list as `change` says, written only while `TAGS.md` is as read: an accepted proposal, a rename or a
/// description never drops a tag the owner saved meanwhile. A list that is not there yet is seeded first ([`load`]).
/// Returns the list as stored.
///
/// # Errors
///
/// As [`load`] and [`validated`], what `change` returns, and a conflict when the file changed in between.
pub fn update(
    config: &Config,
    change: impl FnOnce(&mut Vec<TagEntry>) -> Result<(), AxiomataError>,
) -> Result<Vec<TagEntry>, AxiomataError> {
    load(config)?;
    let Some(file) = workspace::read(config, TAGS_FILE)? else {
        return Err(refused("TAGS.md went away while the list was changed"));
    };
    let mut entries = parse(&file.content);
    change(&mut entries)?;
    let entries = validated(entries)?;
    let text = render(&entries);
    if text != file.content {
        workspace::write(config, TAGS_FILE, &text, Some(&file.version))?;
    }
    Ok(entries)
}

/// Adds `tag` to the list unless it is there; returns whether it was added.
///
/// # Errors
///
/// As [`update`].
pub fn add(config: &Config, tag: &str, description: &str) -> Result<bool, AxiomataError> {
    let Some(tag) = normalise(tag) else {
        return Err(refused(format!("{tag:?} is no tag")));
    };
    let mut added = false;
    update(config, |entries| {
        if !entries.iter().any(|entry| entry.tag == tag) {
            entries.push(TagEntry {
                tag: tag.clone(),
                description: description.to_owned(),
            });
            added = true;
        }
        Ok(())
    })?;
    Ok(added)
}

/// What [`describe`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Described {
    /// The tag, normalised.
    pub tag: String,
    /// The description as stored (one line, capped).
    pub description: String,
    /// Whether the tag was not on the list before and was put there with it.
    pub added: bool,
    /// Whether the stored description differs from the one before.
    pub changed: bool,
}

/// Sets `tag`'s description on the list — the owner's `notes tags describe`. A tag that is not on the list yet is
/// added with it: the owner naming a tag and what it is for is the decision a proposal waits for.
///
/// # Errors
///
/// `tag` is no tag, and as [`update`].
pub fn describe(config: &Config, tag: &str, description: &str) -> Result<Described, AxiomataError> {
    let Some(tag) = normalise(tag) else {
        return Err(refused(format!("{tag:?} is no tag")));
    };
    let description = one_line(description);
    let mut outcome = Described {
        tag: tag.clone(),
        description: description.clone(),
        added: false,
        changed: false,
    };
    update(config, |entries| {
        match entries.iter_mut().find(|entry| entry.tag == tag) {
            Some(entry) => {
                outcome.changed = entry.description != description;
                entry.description = description.clone();
            }
            None => {
                outcome.added = true;
                outcome.changed = true;
                entries.push(TagEntry {
                    tag: tag.clone(),
                    description: description.clone(),
                });
            }
        }
        Ok(())
    })?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_and_parse_round_trip() {
        let entries = vec![
            TagEntry {
                tag: "rust/async".into(),
                description: "Async Rust — tokio, futures".into(),
            },
            TagEntry {
                tag: "solo".into(),
                description: String::new(),
            },
        ];
        assert_eq!(parse(&render(&entries)), entries);
    }

    #[test]
    fn parse_takes_other_dashes_and_skips_the_rest() {
        let text = "# Tags\n\nSome #prose here.\n- #A – first\n* #b: second\n- #a — again\n- #1 — number\n- plain\n";
        let parsed = parse(text);
        assert_eq!(
            parsed,
            vec![
                TagEntry {
                    tag: "a".into(),
                    description: "first".into()
                },
                TagEntry {
                    tag: "b".into(),
                    description: "second".into()
                },
            ]
        );
    }

    #[test]
    fn validated_normalises_sorts_and_refuses_bad_or_double_tags() {
        let ok = validated(vec![
            TagEntry {
                tag: "#Zeta".into(),
                description: "line\nbreak".into(),
            },
            TagEntry {
                tag: "alpha".into(),
                description: String::new(),
            },
        ])
        .unwrap();
        assert_eq!(ok[0].tag, "alpha");
        assert_eq!(ok[1].tag, "zeta");
        assert_eq!(ok[1].description, "line break");
        for bad in [vec!["a b"], vec!["x", "X"], vec!["123"]] {
            let entries = bad
                .into_iter()
                .map(|tag| TagEntry {
                    tag: tag.into(),
                    description: String::new(),
                })
                .collect();
            assert!(validated(entries).is_err());
        }
    }

    #[test]
    fn a_description_is_one_capped_line() {
        assert_eq!(one_line("a\tb\r\nc"), "a b c");
        assert_eq!(
            one_line("Rust\u{200b}\u{202e} notes\u{e0041}\u{e0042} — ok"),
            "Rust notes — ok"
        );
        assert_eq!(
            one_line(&"x".repeat(500)).chars().count(),
            MAX_DESCRIPTION_CHARS
        );
    }
}
