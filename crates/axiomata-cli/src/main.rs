//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history,
//! sync the memory router, send an assistant turn.

use anyhow::{Context, Result};
use axiomata_core::agents::{self, ChatMode};
use axiomata_core::routines::{self, NewRoutine, RoutineTarget};
use axiomata_core::skills::{self, RunStatus};
use axiomata_core::{AxiomataCore, memory, paths};
use clap::{ArgGroup, Args, Parser, Subcommand};

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
    /// Scheduled routines: list, create, enable/disable, or run one poll pass.
    Routines {
        #[command(subcommand)]
        action: RoutineAction,
    },
    /// Send one turn to the dashboard assistant (Claude Code) and print the
    /// Markdown reply plus the session id to continue with `--resume`.
    Assistant {
        /// The message.
        message: String,
        /// Continue an earlier session (id printed by a previous turn).
        #[arg(long)]
        resume: Option<String>,
        /// Allow the agent to edit workspace files (one-shot instruction).
        #[arg(long)]
        instruct: bool,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryAction {
    /// Regenerate every router block from the current workspace contents.
    Sync,
    /// Report whether the router is stale (a tracked file changed since sync).
    Status,
}

#[derive(Debug, Subcommand)]
enum RoutineAction {
    /// List every routine, soonest next-fire first.
    List,
    /// Create a routine.
    Add(AddRoutine),
    /// Enable a routine by id (recomputes its next fire from now).
    Enable {
        /// Routine id, as shown by `routines list`.
        id: i64,
    },
    /// Disable a routine by id (keeps its schedule, stops it firing).
    Disable {
        /// Routine id, as shown by `routines list`.
        id: i64,
    },
    /// Show a routine's firing history, newest first.
    History {
        /// Routine id, as shown by `routines list`.
        id: i64,
        /// Maximum number of entries to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Run one scheduler poll pass now (fire whatever is due) and exit.
    ///
    /// This is what the background loop does every 30 seconds; exposed here so
    /// a routine can be exercised without waiting on the timer.
    Tick,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target").required(true).args(["skill", "prompt"])
))]
struct AddRoutine {
    /// Unique routine name.
    #[arg(long)]
    name: String,
    /// Cron expression: 6-7 fields, seconds first, e.g. "0 */2 * * * *".
    #[arg(long)]
    cron: String,
    /// Fire this skill (by name) when the routine runs.
    #[arg(long, group = "target")]
    skill: Option<String>,
    /// Send this raw prompt to an agent when the routine runs.
    #[arg(long, group = "target")]
    prompt: Option<String>,
    /// Backend override: "claude-code" or "ollama". Defaults to the skill's
    /// own backend, or "ollama" for a raw prompt.
    #[arg(long)]
    backend: Option<String>,
    /// Create the routine disabled.
    #[arg(long)]
    disabled: bool,
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
        Command::Routines { action } => return routines_cmd(&core, action).await,
        Command::Assistant {
            message,
            resume,
            instruct,
        } => return assistant(&core, message, resume, instruct).await,
    }
    Ok(())
}

/// One assistant turn; prints the reply and the session id.
async fn assistant(
    core: &AxiomataCore,
    message: String,
    resume: Option<String>,
    instruct: bool,
) -> Result<()> {
    let mode = if instruct {
        ChatMode::Instruct
    } else {
        ChatMode::Chat
    };
    let reply = agents::chat(&core.config, message, resume, mode)
        .await
        .context("assistant turn failed")?;
    println!("{}", reply.reply_markdown.trim_end());
    println!();
    println!(
        "session: {}  ({} ms{}{})",
        reply.session_id,
        reply.duration_ms,
        reply
            .cost_usd
            .map(|c| format!(", ${c:.4}"))
            .unwrap_or_default(),
        if reply.is_error { ", is_error" } else { "" },
    );
    if reply.is_error {
        std::process::exit(1);
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

/// Dispatches the `routines` subcommands.
async fn routines_cmd(core: &AxiomataCore, action: RoutineAction) -> Result<()> {
    match action {
        RoutineAction::List => routines_list(core),
        RoutineAction::Add(args) => routines_add(core, args),
        RoutineAction::Enable { id } => routines_set_enabled(core, id, true),
        RoutineAction::Disable { id } => routines_set_enabled(core, id, false),
        RoutineAction::History { id, limit } => routines_history(core, id, limit),
        RoutineAction::Tick => routines_tick(core).await,
    }
}

/// Prints one line per routine, soonest next-fire first.
fn routines_list(core: &AxiomataCore) -> Result<()> {
    let db = core.db.lock().expect("database mutex is poisoned");
    let routines = routines::store::list(&db).context("failed to read routines")?;
    if routines.is_empty() {
        println!("No routines defined.");
        return Ok(());
    }
    for routine in routines {
        let (kind, value) = routine.target.to_columns();
        println!(
            "#{id:<4} {enabled}  {next:<25}  {name}  [{kind}: {value}]  ({cron})",
            id = routine.id,
            enabled = if routine.enabled { "on " } else { "off" },
            next = routine
                .next_fire_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "—".to_owned()),
            name = routine.name,
            cron = routine.cron_expr,
        );
    }
    Ok(())
}

/// Creates a routine from the parsed `add` arguments.
fn routines_add(core: &AxiomataCore, args: AddRoutine) -> Result<()> {
    // The clap ArgGroup guarantees exactly one of skill / prompt is set.
    let target = match (args.skill, args.prompt) {
        (Some(name), None) => RoutineTarget::Skill(name),
        (None, Some(text)) => RoutineTarget::Prompt(text),
        _ => unreachable!("clap enforces exactly one target"),
    };

    let db = core.db.lock().expect("database mutex is poisoned");
    let routine = routines::store::add(
        &db,
        NewRoutine {
            name: args.name,
            cron_expr: args.cron,
            target,
            backend: args.backend,
            enabled: !args.disabled,
        },
    )
    .context("failed to create routine")?;

    println!(
        "created routine #{id} {name:?}  next fire: {next}",
        id = routine.id,
        name = routine.name,
        next = routine
            .next_fire_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "never (cron has no future occurrence)".to_owned()),
    );
    Ok(())
}

/// Enables or disables a routine by id.
fn routines_set_enabled(core: &AxiomataCore, id: i64, enabled: bool) -> Result<()> {
    let db = core.db.lock().expect("database mutex is poisoned");
    let found = routines::store::set_enabled(&db, id, enabled)
        .with_context(|| format!("failed to update routine #{id}"))?;
    if !found {
        anyhow::bail!("no routine with id {id}");
    }
    println!(
        "routine #{id} {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Prints a routine's firing history.
fn routines_history(core: &AxiomataCore, id: i64, limit: usize) -> Result<()> {
    let db = core.db.lock().expect("database mutex is poisoned");
    let runs = routines::store::list_runs(&db, id, limit)
        .with_context(|| format!("failed to read history for routine #{id}"))?;
    if runs.is_empty() {
        println!("Routine #{id} has not fired yet.");
        return Ok(());
    }
    for run in runs {
        println!(
            "{fired}  {status:<7}  scheduled {scheduled}  run {run_id}{detail}",
            fired = run.fired_at.to_rfc3339(),
            status = run.status.as_str(),
            scheduled = run.scheduled_for.to_rfc3339(),
            run_id = run
                .run_id
                .map(|r| format!("#{r}"))
                .unwrap_or_else(|| "—".to_owned()),
            detail = run.detail.map(|d| format!("  ({d})")).unwrap_or_default(),
        );
    }
    Ok(())
}

/// Runs one scheduler poll pass and reports what fired.
async fn routines_tick(core: &AxiomataCore) -> Result<()> {
    let report = routines::scheduler::tick(&core.config, &core.db)
        .await
        .context("routine tick failed")?;
    println!(
        "tick: {fired} fired ({succeeded} ok, {failed} failed)",
        fired = report.fired,
        succeeded = report.succeeded,
        failed = report.failed,
    );
    for err in &report.errors {
        eprintln!("  error: {err}");
    }
    if !report.errors.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
