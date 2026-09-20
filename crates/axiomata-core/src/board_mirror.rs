//! Mirrors a Kanban board into the user's workspace as Markdown.
//!
//! The database is the board. This is a picture of it, written so that
//! Obsidian, the Second-Brain search and the memory router can see what is on
//! a board without the app running — and **only** that. Nothing reads the file
//! back, and the file says so in its own header.
//!
//! One-way is not laziness. A board is edited concurrently (a human now, two
//! agents in M7.5), and card claiming rests on a compare-and-swap that a text
//! file cannot offer. Two writers with no way to arbitrate between them is a
//! worse problem than a file that is only ever a copy.
//!
//! This lives in `axiomata-core`, not in `axiomata-board`, because only the
//! embedder knows where a workspace is. The board crate renders the text
//! (`render_board_markdown`); everything here is about where it goes.

use std::fs;
use std::path::PathBuf;

use rusqlite::Connection;

use crate::config::Config;
use crate::error::AxiomataError;
use crate::workspace;

/// Workspace folder the mirrors live in, relative to `workspace_root`.
pub const MIRROR_DIR: &str = "Kanban";

/// Turns a board name into something safe to put in a file name.
///
/// Not a pretty-printer — a guard. A board may be called anything at all,
/// including `../../etc/passwd` or a name full of separators, and the result
/// is pasted into a path. `workspace::resolve` would reject an escape anyway,
/// but rejecting it here means a legitimately odd board name still gets a
/// mirror instead of an error the user cannot act on.
fn slug(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(['-', ' ']).to_string();
    if trimmed.is_empty() {
        "Brett".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

/// `Kanban/<id>-<name>.md`.
///
/// The id prefix is what makes a board's mirror findable regardless of what
/// it is currently called, and what keeps two boards of the same name apart.
/// The name is in there for the human reading the folder — which does mean a
/// rename produces a new file name, so [`sync`] sweeps the old one away.
fn rel_path(board_id: i64, name: &str) -> String {
    format!("{MIRROR_DIR}/{board_id}-{}.md", slug(name))
}

/// Writes (or rewrites) one board's mirror, and removes any earlier mirror of
/// the same board left behind by a rename.
///
/// Returns the path written, or `None` if there is no such board.
pub fn sync(
    db: &Connection,
    config: &Config,
    board_id: i64,
) -> Result<Option<PathBuf>, AxiomataError> {
    let Some(board) = crate::board::store::get_board(db, board_id)? else {
        return Ok(None);
    };
    let columns = crate::board::store::list_columns(db, board_id)?;
    // Archived cards are fetched and dropped by the renderer rather than
    // filtered here, so "what the mirror shows" stays one decision in one
    // place — the renderer's.
    let cards = crate::board::store::list_cards(db, board_id, true)?;

    let rel = rel_path(board.id, &board.name);
    let markdown = crate::board::render_board_markdown(&board, &columns, &cards);
    workspace::write_file(config, &rel, &markdown)?;
    sweep_stale(config, board_id, Some(&rel));
    Ok(Some(workspace::resolve(config, &rel)?))
}

/// Removes a board's mirror entirely — after the board itself is deleted.
pub fn remove(config: &Config, board_id: i64) {
    sweep_stale(config, board_id, None);
}

/// Refreshes the mirror after a change, without letting it spoil the change.
///
/// A mirror is a convenience. If the workspace is missing, read-only, or on a
/// volume that has gone away, the edit the user just made still happened and
/// must still be reported as having happened — so this logs and returns
/// rather than propagating. Every mutating call site uses this, not [`sync`].
pub fn after_change(db: &Connection, config: &Config, board_id: i64) {
    if let Err(err) = sync(db, config, board_id) {
        tracing::warn!(board_id, %err, "could not write the board mirror");
    }
}

/// Same, for a call site that holds a card rather than a board.
pub fn after_card_change(db: &Connection, config: &Config, card_id: i64) {
    match crate::board::store::get_card(db, card_id) {
        Ok(Some(card)) => after_change(db, config, card.board_id),
        Ok(None) => {}
        Err(err) => tracing::warn!(card_id, %err, "could not resolve the card's board"),
    }
}

/// Same, for a call site that holds a column rather than a board.
pub fn after_column_change(db: &Connection, config: &Config, column_id: i64) {
    match crate::board::store::get_column(db, column_id) {
        Ok(Some(column)) => after_change(db, config, column.board_id),
        Ok(None) => {}
        Err(err) => tracing::warn!(column_id, %err, "could not resolve the column's board"),
    }
}

/// Deletes every `Kanban/<board_id>-*.md` except `keep`.
///
/// Best-effort and deliberately silent: a mirror is a convenience, and failing
/// to tidy one up must never turn into an error on the edit that triggered it.
/// The directory not existing yet is the normal first-run case, not a problem.
fn sweep_stale(config: &Config, board_id: i64, keep: Option<&str>) {
    // Joined directly rather than through `workspace::resolve`, which refuses
    // a directory by design. Safe here because `MIRROR_DIR` is a constant and
    // never caller input; every *file* this then touches still goes through
    // `workspace::delete_file` and its full set of guards.
    let dir = config.workspace_root.join(MIRROR_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let prefix = format!("{board_id}-");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(&prefix) || !name.ends_with(".md") {
            continue;
        }
        let rel = format!("{MIRROR_DIR}/{name}");
        if keep == Some(rel.as_str()) {
            continue;
        }
        let _ = workspace::delete_file(config, &rel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_keeps_a_readable_name() {
        assert_eq!(slug("Axiomata OS"), "Axiomata OS");
        assert_eq!(slug("Sprint-7_final"), "Sprint-7_final");
    }

    #[test]
    fn a_slug_defuses_a_name_that_would_escape_the_folder() {
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert!(!slug("a/b\\c").contains('/'));
        assert!(!slug("a/b\\c").contains('\\'));
    }

    #[test]
    fn a_nameless_board_still_gets_a_file_name() {
        assert_eq!(slug(""), "Brett");
        assert_eq!(slug("---"), "Brett");
        assert_eq!(slug("   "), "Brett");
    }

    #[test]
    fn a_very_long_name_is_cut_rather_than_refused() {
        assert_eq!(slug(&"x".repeat(200)).chars().count(), 60);
    }

    #[test]
    fn the_path_carries_the_id_so_a_rename_can_be_swept_up() {
        assert_eq!(rel_path(7, "Mein Brett"), "Kanban/7-Mein Brett.md");
        assert_eq!(rel_path(7, "Umbenannt"), "Kanban/7-Umbenannt.md");
    }
}
