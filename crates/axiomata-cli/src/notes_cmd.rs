//! `axiomata-cli notes tag|tags` — the one narrow way an agent sets a note's tags and links
//! (`docs/plans/tags-links.md` V2): `tags:` in the frontmatter from the vault's tag list and a `## Related` section of
//! links to notes that exist, nothing else of the note. A tag that is not on the list becomes a proposal the owner
//! answers in the app. `inbox-sort` and `vault-tagging` declare it instead of any write right, as `inbox move` for the
//! move (glut-round W8): the command checks every path and tag itself. `notes tags rename|describe|lift-inline` are the
//! owner's cleanup of the list from a terminal, through the same core as Settings → Tags, closed to every agent
//! session, skill run and chat turn.
//!
//! It acts on the configured workspace — never on the shell's folder — through the workspace's strict guard, and
//! needs the database for the proposals, so a Glut run starts it directly (`agent_entry::cli_direct_program`).

use anyhow::{Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::tags::{self, note::TagRequest};
use clap::Subcommand;

use crate::{board_flow, read_config};

#[derive(Debug, Subcommand)]
pub enum NotesAction {
    /// Set tags from the vault's tag list (TAGS.md) and `## Related` links on one note; nothing else of it changes.
    Tag {
        /// The note, relative to the vault (`Arbeit/Plan.md`).
        file: String,
        /// A tag from the list (repeatable); one not on it waits for the owner as a proposal and is not set.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// A note that exists to link under `## Related`, by name or path (repeatable, at most 3).
        #[arg(long = "related", value_name = "NOTE")]
        related: Vec<String>,
        /// A new tag with a one-line description, for the owner to accept: `--propose rust/async:"Async Rust"`.
        #[arg(long = "propose", value_name = "TAG:DESCRIPTION")]
        propose: Vec<String>,
        /// Say what would change; write nothing to the note.
        #[arg(long)]
        dry_run: bool,
    },
    /// The vault's tag list.
    Tags {
        #[command(subcommand)]
        action: TagsAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum TagsAction {
    /// Print the tag list, one `#tag — description` per line (written to TAGS.md first if the vault has none).
    List {
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// The vault run's work (`vault-tagging`, tags-links.md TL6): which step it is in and the next notes to do.
    Todo,
    /// Leave a note out of the vault run's notes step, with the reason (a name not safe on a command line, …).
    Skip {
        /// The note, relative to the vault.
        file: String,
        /// Why, in a few words.
        reason: String,
    },
    /// Propose a new tag for the list, with a line of description; the owner accepts or turns it down in the app.
    Propose {
        /// The tag, lower case (`rust/async`).
        tag: String,
        /// One line on what the tag is for.
        description: String,
    },
    /// The owner's: rename a tag in every note (frontmatter and text, nested tags with it) and on the list. A new
    /// name already on the list is a merge: the old tag goes into it and its description stays.
    Rename {
        /// The tag to rename (`tauri`).
        old: String,
        /// Its new name; nested is fine (`rust/gui`).
        new: String,
        /// The new tag's description on the list (else the merged entry's, or the old tag's).
        #[arg(long)]
        description: Option<String>,
        /// Count what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// The counts as JSON.
        #[arg(long)]
        json: bool,
    },
    /// The owner's: set a tag's one-line description on the list (a tag not on it is added).
    Describe {
        /// The tag (`ai/news`).
        tag: String,
        /// One line on what the tag is for.
        description: String,
        /// The outcome as JSON.
        #[arg(long)]
        json: bool,
    },
    /// The owner's, once: move `#tags` from the notes' text into their frontmatter's `tags:` (code, links, URLs and
    /// headings untouched; a line left empty goes).
    LiftInline {
        /// Only this tag and the ones below it (repeatable); without it, every inline tag.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// Say which notes would change; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// The counts and notes as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Who the proposals are from: the agent session, the app's harness run, or the owner's own shell.
fn proposer() -> String {
    if let Some(actor) = axiomata_core::session::session_actor() {
        return actor;
    }
    match board_flow::harness_run_kind() {
        Some(board_flow::HarnessRun::Skill) => "a skill run".to_owned(),
        Some(board_flow::HarnessRun::Chat) => "the chat".to_owned(),
        None => "the command line".to_owned(),
    }
}

/// How recently a note must have come to its place for an unattended skill run to tag it: a sort run's moves and tags
/// lie well within it.
const RECENT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Whether the file at `path` (never followed through a link) changed its place or content within `window`: its
/// status-change time, which a move by `inbox move` (a new link) and every write set.
fn just_placed(path: &std::path::Path, window: std::time::Duration) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let changed =
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(meta.ctime().max(0) as u64);
    std::time::SystemTime::now()
        .duration_since(changed)
        .is_ok_and(|age| age <= window)
}

/// `tag:description` of `--propose`, split at the first colon.
fn split_proposal(raw: &str) -> Result<(String, String)> {
    let Some((tag, description)) = raw.split_once(':') else {
        bail!("--propose takes TAG:DESCRIPTION, such as rust/async:\"Async Rust\"");
    };
    Ok((tag.trim().to_owned(), description.trim().to_owned()))
}

/// The owner's commands of the list: they rewrite many notes at once, so no agent session, skill run or chat turn
/// starts them — not even as a dry run, which reads every note.
fn owner_command(action: &NotesAction) -> Option<&'static str> {
    match action {
        NotesAction::Tags {
            action: TagsAction::Rename { .. },
        } => Some("renaming a tag"),
        NotesAction::Tags {
            action: TagsAction::Describe { .. },
        } => Some("describing a tag"),
        NotesAction::Tags {
            action: TagsAction::LiftInline { .. },
        } => Some("lifting inline tags"),
        _ => None,
    }
}

pub fn run(core: &AxiomataCore, action: NotesAction) -> Result<()> {
    if let Some(what) = owner_command(&action) {
        board_flow::owner_only(what)?;
    }
    let writes = matches!(
        action,
        NotesAction::Tag { .. }
            | NotesAction::Tags {
                action: TagsAction::Propose { .. } | TagsAction::Skip { .. }
            }
    );
    // A studio card session works in its worktree; the vault is none of its business (security audit).
    if writes && axiomata_core::session::session_actor().is_some() {
        bail!(
            "an agent session does not tag the vault's notes or propose tags; the owner's skills and chat do"
        );
    }
    let config = read_config(core);
    match action {
        NotesAction::Tag {
            file,
            tags: wanted,
            related,
            propose,
            dry_run,
        } => {
            let proposals = propose
                .iter()
                .map(|raw| split_proposal(raw))
                .collect::<Result<Vec<_>>>()?;
            // An unattended skill run writes only into a note that just came to its place — moved by `inbox move` or
            // newly written — not into any note an injected text names (security audit, TL5). Its dry run (the vault
            // run's plan) writes nothing and may look at every note.
            if !dry_run
                && board_flow::harness_run_kind() == Some(board_flow::HarnessRun::Skill)
                && !just_placed(&config.workspace_root.join(&file), RECENT)
            {
                bail!(
                    "a skill run tags only a note it just moved or wrote; {file} has been where it is for a while"
                );
            }
            let request = TagRequest {
                note: file,
                tags: wanted,
                related,
                proposals,
                dry_run,
            };
            let outcome = match tags::note::tag_note(&config, &core.db, &proposer(), &request) {
                Ok(outcome) => outcome,
                Err(err) => {
                    // A note the vault run cannot plan is left out of it, so its notes step can still finish.
                    if dry_run
                        && tags::vault_run::record_skip(
                            &core.db_lock(),
                            &request.note,
                            &err.to_string(),
                        )?
                    {
                        println!("left out of the vault run: {err}");
                        return Ok(());
                    }
                    return Err(err.into());
                }
            };
            print_outcome(&outcome, dry_run);
            // In the vault run's notes step a dry run is its plan: kept for the owner's preview, written only by them.
            if dry_run {
                let db = core.db_lock();
                if tags::vault_run::record(&db, &outcome, &request.proposals)? {
                    println!("planned for the owner's preview; nothing was written");
                }
            }
        }
        NotesAction::Tags {
            action: TagsAction::Skip { file, reason },
        } => {
            if tags::vault_run::record_skip(&core.db_lock(), &file, &reason)? {
                println!("left {file} out of the vault run");
            } else {
                println!("no vault run is in its notes step; nothing to skip");
            }
        }
        NotesAction::Tags {
            action: TagsAction::Todo,
        } => print_todo(&tags::vault_run::todo(&config, &core.db)?),
        NotesAction::Tags {
            action: TagsAction::Propose { tag, description },
        } => {
            let proposed = {
                let db = core.db_lock();
                tags::proposals::propose(&db, &tag, &description, None, &proposer())?
            };
            match proposed {
                tags::proposals::Proposed::Waiting => {
                    println!("proposed #{} to the owner", tag.trim_start_matches('#'))
                }
                tags::proposals::Proposed::RejectedBefore => {
                    println!("#{tag} was turned down by the owner lately; not proposed again")
                }
                tags::proposals::Proposed::Full => {
                    println!(
                        "not proposed: too many proposals wait for the owner already; stop proposing"
                    )
                }
            }
        }
        NotesAction::Tags {
            action:
                TagsAction::Rename {
                    old,
                    new,
                    description,
                    dry_run,
                    json,
                },
        } => {
            let renamed =
                tags::rename::rename_tag(&config, &old, &new, description.as_deref(), dry_run)?;
            if !dry_run {
                settle(core, &config);
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&renamed)?);
            } else {
                print_rename(&renamed, dry_run);
            }
        }
        NotesAction::Tags {
            action:
                TagsAction::Describe {
                    tag,
                    description,
                    json,
                },
        } => {
            let described = tags::list::describe(&config, &tag, &description)?;
            settle(core, &config);
            if json {
                println!("{}", serde_json::to_string_pretty(&described)?);
            } else if described.added {
                println!("added #{} — {}", described.tag, described.description);
            } else if described.changed {
                println!("#{} — {}", described.tag, described.description);
            } else {
                println!("#{} is unchanged", described.tag);
            }
        }
        NotesAction::Tags {
            action:
                TagsAction::LiftInline {
                    tags: only,
                    dry_run,
                    json,
                },
        } => {
            let lifted = tags::lift::lift_inline(&config, &only, dry_run)?;
            if json {
                let report = serde_json::json!({
                    "dry_run": dry_run,
                    "files": lifted.files(),
                    "removed": lifted.removed(),
                    "notes": lifted.notes,
                    "skipped": lifted.skipped,
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_lifted(&lifted, dry_run);
            }
        }
        NotesAction::Tags {
            action: TagsAction::List { json },
        } => {
            let entries = tags::list::load(&config)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else if entries.is_empty() {
                println!("the tag list is empty");
            } else {
                for entry in entries {
                    if entry.description.is_empty() {
                        println!("#{}", entry.tag);
                    } else {
                        println!("#{} — {}", entry.tag, entry.description);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Closes the proposals of tags now on the list, as the settings do after a change; a failure only leaves the badge
/// lit.
fn settle(core: &AxiomataCore, config: &axiomata_core::config::Config) {
    let listed = match tags::list::load(config) {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| entry.tag)
            .collect::<Vec<_>>(),
        Err(err) => {
            eprintln!("tag proposals not settled: {err}");
            return;
        }
    };
    if let Err(err) = tags::proposals::settle_listed(&core.db_lock(), &listed) {
        eprintln!("tag proposals not settled: {err}");
    }
}

fn print_rename(renamed: &tags::rename::TagRename, dry_run: bool) {
    let verb = if dry_run { "would rename" } else { "renamed" };
    println!(
        "{verb} #{} to #{}: {} uses in {} notes",
        renamed.old, renamed.new, renamed.uses, renamed.files
    );
    if renamed.merged {
        println!(
            "#{} is on the list already: #{} merges into it",
            renamed.new, renamed.old
        );
    }
    if renamed.description.is_empty() {
        println!(
            "#{} has no description; set one with `notes tags describe`",
            renamed.new
        );
    } else {
        println!("#{} — {}", renamed.new, renamed.description);
    }
    for skipped in &renamed.skipped {
        println!("skipped {}: {}", skipped.note, skipped.reason);
    }
}

fn print_lifted(lifted: &tags::lift::Lifted, dry_run: bool) {
    let verb = if dry_run { "would move" } else { "moved" };
    for note in &lifted.notes {
        let hashed: Vec<String> = note.tags.iter().map(|tag| format!("#{tag}")).collect();
        println!(
            "{}: {} ({} in the text)",
            note.note,
            hashed.join(" "),
            note.removed
        );
    }
    for skipped in &lifted.skipped {
        println!("skipped {}: {}", skipped.note, skipped.reason);
    }
    println!(
        "{verb} {} inline tags into the frontmatter of {} notes",
        lifted.removed(),
        lifted.files()
    );
}

fn print_todo(todo: &tags::vault_run::Todo) {
    match todo.phase {
        None => println!("step: none — no vault run is under way; stop here"),
        Some(tags::vault_run::Phase::List) => println!("step: propose-list"),
        Some(tags::vault_run::Phase::Notes) => println!("step: tag-notes"),
    }
    if todo.phase.is_none() {
        return;
    }
    if todo.notes.is_empty() {
        println!("notes to do: none — the step is done");
        return;
    }
    println!(
        "notes to do ({} here, {} more after these):",
        todo.notes.len(),
        todo.more
    );
    for note in &todo.notes {
        println!("{note}");
    }
}

fn print_outcome(outcome: &tags::note::TagOutcome, dry_run: bool) {
    let verb = if dry_run { "would set" } else { "set" };
    let hashed = |list: &[String]| {
        list.iter()
            .map(|tag| format!("#{tag}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if !outcome.tagged.is_empty() {
        println!(
            "{verb} tags on {}: {}",
            outcome.note,
            hashed(&outcome.tagged)
        );
    }
    if !outcome.already.is_empty() {
        println!("already tagged: {}", hashed(&outcome.already));
    }
    if !outcome.related.is_empty() {
        let links: Vec<String> = outcome
            .related
            .iter()
            .map(|name| format!("[[{name}]]"))
            .collect();
        println!("{verb} under ## Related: {}", links.join(" "));
    }
    if !outcome.proposed.is_empty() {
        println!(
            "proposed to the owner (not set): {}",
            hashed(&outcome.proposed)
        );
    }
    for line in &outcome.refused {
        println!("refused: {line}");
    }
    if outcome.tagged.is_empty() && outcome.related.is_empty() {
        println!("{} is unchanged", outcome.note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_splits_at_its_first_colon() {
        assert_eq!(
            split_proposal("rust/async: Async: Rust").unwrap(),
            ("rust/async".to_owned(), "Async: Rust".to_owned())
        );
        assert!(split_proposal("nocolon").is_err());
    }

    #[test]
    fn a_note_just_written_counts_as_just_placed() {
        let path =
            std::env::temp_dir().join(format!("axiomata-notes-placed-{}.md", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        assert!(just_placed(&path, RECENT));
        assert!(!just_placed(&path.with_extension("none"), RECENT));
        let _ = std::fs::remove_file(&path);
    }
}
