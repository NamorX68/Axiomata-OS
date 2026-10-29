//! `axiomata-cli notes tag|tags` — the one narrow way an agent sets a note's tags and links
//! (`docs/plans/tags-links.md` V2): `tags:` in the frontmatter from the vault's tag list and a `## Related` section of
//! links to notes that exist, nothing else of the note. A tag that is not on the list becomes a proposal the owner
//! answers in the app. `inbox-sort` and `vault-tagging` declare it instead of any write right, as `inbox move` for the
//! move (glut-round W8): the command checks every path and tag itself.
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

/// `tag:description` of `--propose`, split at the first colon.
fn split_proposal(raw: &str) -> Result<(String, String)> {
    let Some((tag, description)) = raw.split_once(':') else {
        bail!("--propose takes TAG:DESCRIPTION, such as rust/async:\"Async Rust\"");
    };
    Ok((tag.trim().to_owned(), description.trim().to_owned()))
}

pub fn run(core: &AxiomataCore, action: NotesAction) -> Result<()> {
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
            let request = TagRequest {
                note: file,
                tags: wanted,
                related,
                proposals,
                dry_run,
            };
            // A studio card session works in its worktree; the vault is none of its business (security audit).
            if axiomata_core::session::session_actor().is_some() {
                bail!(
                    "an agent session does not tag the vault's notes; the owner's skills and chat do"
                );
            }
            let outcome = tags::note::tag_note(&config, &core.db, &proposer(), &request)?;
            print_outcome(&outcome, dry_run);
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
}
