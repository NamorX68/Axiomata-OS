//! `axiomata-cli maintenance status|clean`: Axiomata's own storage (`docs/plans/maintenance.md`, MT3) — what it takes,
//! what a clean-up frees, and the clean-up itself, the same one the app runs once a week at start.

use anyhow::{Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::maintenance::{self, Report};
use clap::Subcommand;

use crate::board_flow;

#[derive(Debug, Subcommand)]
pub enum MaintenanceAction {
    /// Sizes, rules, the last clean-up and what a clean-up would free; deletes nothing.
    Status {
        /// The report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Clean up by the rules of `[maintenance]` in config.toml. Worktrees with changes are only listed.
    Clean {
        /// Only say what would go.
        #[arg(long)]
        dry_run: bool,
        /// The report as JSON.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(core: &AxiomataCore, action: MaintenanceAction) -> Result<()> {
    let (report, json) = match action {
        MaintenanceAction::Status { json } => (maintenance::report(core), json),
        MaintenanceAction::Clean { dry_run, json } => {
            if !dry_run {
                // Deleting the owner's data by the owner's rules is the owner's step, never an agent's or a skill's.
                board_flow::owner_only("cleaning up Axiomata's storage")?;
            }
            (maintenance::clean(core, dry_run), json)
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    if let Some(error) = &report.error {
        bail!("{error}");
    }
    Ok(())
}

/// "340 B", "1.2 KB", "3.4 MB", "2.1 GB" — the Settings page's way of writing a size.
fn human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let n = bytes as f64;
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", n / KB),
        1_048_576..1_073_741_824 => format!("{:.1} MB", n / (KB * KB)),
        _ => format!("{:.1} GB", n / (KB * KB * KB)),
    }
}

fn print_report(report: &Report) {
    let verb = if report.dry_run {
        "would free"
    } else {
        "freed"
    };
    println!(
        "{}: {}  ({verb} {})",
        report.home.display(),
        human(report.total_bytes),
        human(report.freeable_bytes)
    );
    match report.last_run {
        Some(when) => println!("last clean-up: {}", when.format("%Y-%m-%d %H:%M UTC")),
        None => println!("last clean-up: never"),
    }
    for item in &report.items {
        let size = item.bytes.map(human).unwrap_or_default();
        let count = if item.unit.is_empty() {
            String::new()
        } else {
            format!("{} {}", item.count, item.unit)
        };
        let freed = match (item.freeable_bytes, item.freeable_count) {
            (Some(bytes), _) if bytes > 0 => human(bytes),
            (_, 0) => "—".to_owned(),
            (_, n) => format!("{n} {}", item.unit),
        };
        println!(
            "  {:<24} {:>10} {:>14}  {verb} {:<10} keep: {}",
            item.label, size, count, freed, item.keep
        );
        if let Some(error) = &item.error {
            println!("    ! {error}");
        }
    }
    let waiting: Vec<_> = report
        .orphan_worktrees
        .iter()
        .filter(|orphan| !orphan.removed && orphan.dirty != Some(false))
        .collect();
    if !waiting.is_empty() {
        println!(
            "orphaned worktrees waiting for you (their agent is gone, they hold unsaved work):"
        );
        for orphan in waiting {
            let state = if orphan.dirty.is_some() {
                "changes"
            } else {
                "unknown state"
            };
            println!(
                "  {}  {}  ({state})",
                orphan.path.display(),
                human(orphan.bytes)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_settings_page() {
        assert_eq!(human(340), "340 B");
        assert_eq!(human(1300), "1.3 KB");
        assert_eq!(human(3 * 1024 * 1024 + 400 * 1024), "3.4 MB");
        assert_eq!(human(2 * 1024 * 1024 * 1024), "2.0 GB");
    }
}
