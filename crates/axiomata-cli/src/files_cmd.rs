//! `axiomata-cli files …` — the file service without the app (editor plan
//! §ED0, E11): list the roots, read and write through the same guard the
//! editor uses, and manage the dialog grants.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use axiomata_core::AxiomataCore;
use axiomata_core::files::{self, Roots};
use axiomata_files::{self as service, MAX_READ_BYTES, MAX_WRITE_BYTES, RootResolver, Version};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum FilesAction {
    /// Every root the file app can reach right now, with its id.
    Roots,
    /// Print a file's content; its version goes to stderr.
    Read {
        /// Root id: `workspace`, `project:<id>`, `worktree:<agent id>`, `grant:<id>`.
        root: String,
        /// Path relative to the root.
        rel: String,
    },
    /// Write stdin to a file and print the new version.
    Write {
        root: String,
        rel: String,
        /// Refuse the write if the file is no longer at this version.
        #[arg(long)]
        expect: Option<String>,
    },
    /// Dialog grants: the files and folders picked in the open dialog.
    Grants {
        #[command(subcommand)]
        action: GrantAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum GrantAction {
    /// Every grant, oldest first.
    List,
    /// Grant a file or folder, as if picked in the open dialog.
    Add { path: PathBuf },
    /// Revoke a grant by id.
    Revoke { id: String },
}

pub fn run(core: &AxiomataCore, action: FilesAction) -> Result<()> {
    match action {
        FilesAction::Roots => roots(core),
        FilesAction::Read { root, rel } => read(core, &root, &rel),
        FilesAction::Write { root, rel, expect } => write(core, &root, &rel, expect),
        FilesAction::Grants { action } => grants(action),
    }
}

fn roots(core: &AxiomataCore) -> Result<()> {
    let config = core.config_read().clone();
    let db = core.db_lock();
    let list = Roots::new(&config, &db).list()?;
    if list.is_empty() {
        println!("No roots.");
    }
    for info in list {
        println!(
            "{:<14} {:<13} {}  ({})",
            info.id,
            info.kind,
            info.label,
            info.path.display()
        );
    }
    Ok(())
}

fn root(core: &AxiomataCore, id: &str) -> Result<service::Root> {
    let config = core.config_read().clone();
    let db = core.db_lock();
    Ok(Roots::new(&config, &db).root(id)?)
}

fn read(core: &AxiomataCore, root_id: &str, rel: &str) -> Result<()> {
    let file = service::read_text(&root(core, root_id)?, rel, MAX_READ_BYTES)?;
    print!("{}", file.content);
    eprintln!(
        "version {}{}",
        file.version,
        if file.large {
            " (large: the editor's light mode)"
        } else {
            ""
        }
    );
    Ok(())
}

fn write(core: &AxiomataCore, root_id: &str, rel: &str, expect: Option<String>) -> Result<()> {
    let mut content = String::new();
    std::io::stdin()
        .read_to_string(&mut content)
        .context("stdin is not valid UTF-8 text")?;
    let expected = expect.map(Version::from);
    let version = service::write_text(
        &root(core, root_id)?,
        rel,
        &content,
        expected.as_ref(),
        MAX_WRITE_BYTES,
    )?;
    println!("{version}");
    Ok(())
}

fn grants(action: GrantAction) -> Result<()> {
    let store = files::grant_store();
    match action {
        GrantAction::List => {
            let list = store.list()?;
            if list.is_empty() {
                println!("No grants.");
            }
            for grant in list {
                println!(
                    "grant:{:<5} {:<6} {}  (since {})",
                    grant.id,
                    format!("{:?}", grant.kind).to_lowercase(),
                    grant.path.display(),
                    grant.granted_at.format("%Y-%m-%d %H:%M")
                );
            }
        }
        GrantAction::Add { path } => {
            let grant = store.grant(&path)?;
            println!("grant:{}  {}", grant.id, grant.path.display());
        }
        GrantAction::Revoke { id } => {
            let id = id.strip_prefix("grant:").unwrap_or(&id);
            if store.revoke(id)? {
                println!("Revoked grant:{id}.");
            } else {
                println!("No grant:{id}.");
            }
        }
    }
    Ok(())
}
