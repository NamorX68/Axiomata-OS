//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history,
//! sync the memory router.

use anyhow::{Context, Result};
use axiomata_core::skills::{self, RunStatus};
use axiomata_core::{AxiomataCore, memory, paths};
use clap::{Parser, Subcommand};

/// Axiomata-OS headless control CLI.
#[derive(Debug, Parser)]
#[command(name = "axiomata-cli", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize the core and print resolved paths (the default action).
    Status,
    /// List every discovered skill.
    ListSkills,
    /// Run a skill by name and print its outcome.
    RunSkill {
        /// Skill name, as in its `SKILL.md` frontmatter.
        name: String,
    },
    /// Show the most recent skill runs, newest first.
    ListRuns {
        /// Maximum number of runs to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Memory router: regenerate or inspect the workspace `CLAUDE.md` blocks.
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryAction {
    /// Regenerate every router block from the current workspace contents.
    Sync,
    /// Report whether the router is stale (a tracked file changed since sync).
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let core = AxiomataCore::init().context("failed to initialize the Axiomata-OS core engine")?;

    match cli.command.unwrap_or(Command::Status) {
        Command::Status => print_status(&core),
        Command::ListSkills => list_skills()?,
        Command::RunSkill { name } => return run_skill(&core, &name).await,
        Command::ListRuns { limit } => list_runs(&core, limit)?,
        Command::Memory { action } => match action {
            MemoryAction::Sync => memory_sync(&core)?,
            MemoryAction::Status => memory_status(&core)?,
        },
    }
    Ok(())
}

/// Prints the resolved runtime paths and workspace root.
fn print_status(core: &AxiomataCore) {
    println!("Axiomata-OS core initialized.");
    println!("  workspace root: {}", core.config.workspace_root.display());
    println!("  config file:    {}", paths::config_path().display());
    println!("  database:       {}", paths::db_path().display());
    println!("  logs directory: {}", paths::logs_dir().display());
    println!("  skills:         {}", paths::global_skills_dir().display());
}

/// Prints one line per discovered skill: `name  backend  — description`.
fn list_skills() -> Result<()> {
    let skills = skills::list_skills().context("failed to scan skills")?;
    if skills.is_empty() {
        println!("No skills found.");
        return Ok(());
    }
    for skill in skills {
        println!(
            "{name}  {backend}  — {description}",
            name = skill.name,
            backend = skill.backend,
            description = skill.description,
        );
    }
    Ok(())
}

/// Runs `name`, prints a summary, and exits non-zero if the run failed.
async fn run_skill(core: &AxiomataCore, name: &str) -> Result<()> {
    let record = skills::execute_and_record_skill(name, &core.config, &core.db)
        .await
        .with_context(|| format!("failed to run skill {name:?}"))?;

    println!(
        "run #{id}  {status}  ({backend}, {ms} ms)",
        id = record.id.unwrap_or_default(),
        status = record.status.as_str(),
        backend = record.backend,
        ms = record.duration_ms,
    );
    if let Some(code) = record.exit_code {
        println!("  exit code: {code}");
    }
    if let Some(err) = &record.error {
        println!("  error: {err}");
    }
    if !record.stdout.trim().is_empty() {
        println!("  --- stdout ---\n{}", record.stdout.trim_end());
    }
    if !record.stderr.trim().is_empty() {
        println!("  --- stderr ---\n{}", record.stderr.trim_end());
    }

    if record.status == RunStatus::Failed {
        std::process::exit(1);
    }
    Ok(())
}

/// Prints recent runs from the database, newest first.
fn list_runs(core: &AxiomataCore, limit: usize) -> Result<()> {
    let db = core.db.lock().expect("database mutex is poisoned");
    let runs = skills::list_runs(&db, limit).context("failed to read run history")?;
    if runs.is_empty() {
        println!("No runs recorded yet.");
        return Ok(());
    }
    for run in runs {
        println!(
            "#{id:<4} {started}  {status:<7} {skill} ({backend}, {ms} ms)",
            id = run.id,
            started = run.started_at.to_rfc3339(),
            status = run.status.as_str(),
            skill = run.skill_name,
            backend = run.backend,
            ms = run.duration_ms,
        );
    }
    Ok(())
}

/// Regenerates the workspace router `CLAUDE.md` blocks and reports what changed.
fn memory_sync(core: &AxiomataCore) -> Result<()> {
    let report = memory::sync(&core.config).context("memory sync failed")?;
    if report.written.is_empty() {
        println!(
            "Router already in sync — {} tracked files, {} CLAUDE.md file(s) unchanged.",
            report.tracked_files, report.unchanged,
        );
    } else {
        println!("Wrote {} CLAUDE.md file(s):", report.written.len());
        for path in &report.written {
            println!("  {}", path.display());
        }
        if report.unchanged > 0 {
            println!("  ({} already current)", report.unchanged);
        }
        println!("{} tracked files.", report.tracked_files);
    }
    if !report.failed.is_empty() {
        eprintln!("\n{} file(s) could not be written:", report.failed.len());
        for (path, why) in &report.failed {
            eprintln!("  {} — {why}", path.display());
        }
        std::process::exit(1);
    }
    Ok(())
}

/// Prints the memory-router freshness status.
fn memory_status(core: &AxiomataCore) -> Result<()> {
    let status = memory::status(&core.config).context("memory status failed")?;
    println!("workspace:     {}", status.workspace_root.display());
    println!("tracked files: {}", status.tracked_files);
    println!(
        "last sync:     {}",
        status
            .last_sync
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "never".to_owned()),
    );
    println!(
        "state:         {}",
        if status.stale {
            "STALE — run `axiomata-cli memory sync`"
        } else {
            "fresh"
        },
    );
    Ok(())
}
