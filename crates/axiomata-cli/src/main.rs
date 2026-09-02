//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history.

use anyhow::{Context, Result};
use axiomata_core::skills::{self, RunStatus};
use axiomata_core::{AxiomataCore, paths};
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
    /// List every discovered skill and where it came from.
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
    println!("  global skills:  {}", paths::global_skills_dir().display());
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
