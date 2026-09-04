//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history,
//! sync the memory router, send an assistant turn, call a dashboard module
//! action through the file queue.

use anyhow::{Context, Result};
use axiomata_core::agents::{self, ChatMode};
use axiomata_core::bridge::{self, ActionRequest};
use axiomata_core::importer;
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
        /// `--allowedTools` value to pass through — needed for the turn to
        /// reach an MCP tool at all (see `AgentRequest::allowed_tools`);
        /// mainly for testing a connector module's write instruction here
        /// before wiring it into the dashboard.
        #[arg(long = "allowed-tools")]
        allowed_tools: Option<String>,
    },
    /// Import notes into the workspace; the agent proposes the areas.
    Import {
        #[command(subcommand)]
        source: ImportSource,
    },
    /// Summarise the workspace graph (areas, files, links, skills, routines).
    Graph,
    /// Print the module manifest the dashboard wrote for the agent.
    Modules,
    /// Call an action on a mounted dashboard module (the running dashboard
    /// answers through `~/.axiomata/module-actions/`). Exits 2 on timeout.
    ModuleAction {
        /// Instance id, as listed by `modules`.
        instance: String,
        /// Action name.
        action: String,
        /// Parameters as a JSON object.
        #[arg(long, default_value = "{}")]
        json: String,
        /// How long to wait for the dashboard.
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
    },
}

#[derive(Debug, Subcommand)]
enum ImportSource {
    /// An Obsidian vault folder (every `*.md` below it).
    Obsidian {
        /// Folder to import from.
        path: std::path::PathBuf,
        /// Show the agent's plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Skip notes that look like they hold API keys / secrets.
        #[arg(long)]
        skip_secrets: bool,
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
            allowed_tools,
        } => return assistant(&core, message, resume, instruct, allowed_tools).await,
        Command::Import {
            source:
                ImportSource::Obsidian {
                    path,
                    dry_run,
                    skip_secrets,
                },
        } => return import_obsidian(&core, &path, dry_run, !skip_secrets).await,
        Command::Graph => graph_summary(&core)?,
        Command::Modules => modules()?,
        Command::ModuleAction {
            instance,
            action,
            json,
            timeout_secs,
        } => return module_action(instance, action, json, timeout_secs),
    }
    Ok(())
}

/// Scans an Obsidian folder, lets the agent sort the notes into areas, writes
/// them into the workspace and re-syncs the memory router.
async fn import_obsidian(
    core: &AxiomataCore,
    path: &std::path::Path,
    dry_run: bool,
    include_secrets: bool,
) -> Result<()> {
    let notes = importer::scan_obsidian(path).context("scanning the Obsidian folder")?;
    if notes.is_empty() {
        println!("no Markdown notes found under {}", path.display());
        return Ok(());
    }
    let secrets = notes.iter().filter(|n| n.secret_like).count();
    println!(
        "{} notes found ({} look like secrets — {})",
        notes.len(),
        secrets,
        if include_secrets {
            "included"
        } else {
            "skipped"
        }
    );
    let root = &core.config.workspace_root;
    let existing = importer::existing_areas(root);
    println!("asking the agent to propose areas…");
    let reply = agents::chat(
        &core.config,
        importer::assignment_prompt(&notes, &existing),
        None,
        ChatMode::Chat,
        None,
    )
    .await
    .context("the sorting turn failed")?;
    let plan = importer::parse_plan(&reply.reply_markdown)?;
    println!("areas proposed:");
    for area in &plan.areas {
        println!(
            "  {:<24} {}",
            importer::sanitize_area(&area.name),
            area.description
        );
    }
    let report = importer::apply(&notes, &plan, root, include_secrets, dry_run)?;
    println!();
    println!("{}:", if dry_run { "would write" } else { "written" });
    for (area, file) in &report.written {
        println!("  {area}/{file}");
    }
    if !report.skipped_existing.is_empty() {
        println!(
            "skipped (already exist): {}",
            report.skipped_existing.join(", ")
        );
    }
    if !report.skipped_secret.is_empty() {
        println!(
            "skipped (secret-like): {}",
            report.skipped_secret.join(", ")
        );
    }
    if !report.fell_back.is_empty() {
        println!(
            "sent to {}: {}",
            importer::FALLBACK_AREA,
            report.fell_back.join(", ")
        );
    }
    if !dry_run {
        let sync = memory::sync(&core.config).context("memory sync after import")?;
        println!(
            "memory router synced: {} CLAUDE.md written, {} tracked files (session {}, ${:.2})",
            sync.written.len(),
            sync.tracked_files,
            reply.session_id,
            reply.cost_usd.unwrap_or_default()
        );
    }
    Ok(())
}

/// Prints a summary of the workspace graph.
fn graph_summary(core: &AxiomataCore) -> Result<()> {
    let db = core
        .db
        .lock()
        .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
    let g = axiomata_core::graph::build(&core.config, &db).context("building the graph")?;
    println!(
        "workspace: {}  hub: {}",
        g.workspace_root,
        g.hub.as_deref().unwrap_or("-")
    );
    println!(
        "{} files ({} total{}), {} links, {} skills, {} routines",
        g.files.len(),
        g.total_files,
        if g.truncated { ", truncated" } else { "" },
        g.links.len(),
        g.skills.len(),
        g.routines.len()
    );
    for area in &g.areas {
        println!("  {:<28} {:>4} files", area.name, area.files);
    }
    for link in g.links.iter().take(10) {
        println!("  link: {} -> {}", link.from, link.to);
    }
    Ok(())
}

/// Prints `~/.axiomata/module-context.md`, or a hint if it doesn't exist.
fn modules() -> Result<()> {
    let path = paths::module_context_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => print!("{text}"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "no manifest at {} — start the dashboard first",
                path.display()
            );
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    }
    Ok(())
}

/// Enqueues one action request and waits for the dashboard's response.
fn module_action(instance: String, action: String, json: String, timeout_secs: u64) -> Result<()> {
    let params: serde_json::Value =
        serde_json::from_str(&json).context("--json must be a JSON value")?;
    let request = ActionRequest {
        id: bridge::new_action_id(),
        instance_id: instance,
        action,
        params,
        created_at: chrono::Utc::now(),
    };
    bridge::enqueue(&request).context("could not write the request")?;
    let response = match bridge::wait_for_response(
        &request.id,
        std::time::Duration::from_secs(timeout_secs),
    ) {
        Ok(response) => response,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(2);
        }
    };
    if response.ok {
        let result = response.result.unwrap_or(serde_json::Value::Null);
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(())
    } else {
        eprintln!(
            "error: {}",
            response
                .error
                .unwrap_or_else(|| "action failed".to_string())
        );
        std::process::exit(1);
    }
}

/// One assistant turn; prints the reply and the session id.
async fn assistant(
    core: &AxiomataCore,
    message: String,
    resume: Option<String>,
    instruct: bool,
    allowed_tools: Option<String>,
) -> Result<()> {
    let mode = if instruct {
        ChatMode::Instruct
    } else {
        ChatMode::Chat
    };
    let reply = agents::chat(&core.config, message, resume, mode, allowed_tools)
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

/// Prints one line per discovered skill: `name  backend  — description`,
/// followed by a warning line for each skill directory that was skipped
/// (broken `SKILL.md`, symlink, …) rather than letting it vanish silently.
fn list_skills() -> Result<()> {
    let skills = skills::list_skills().context("failed to scan skills")?;
    let skipped = skills::list_skipped_skills().context("failed to scan skills")?;

    if skills.is_empty() {
        println!("No skills found.");
    }
    for skill in skills {
        println!(
            "{name}  {backend}  — {description}",
            name = skill.name,
            backend = skill.backend,
            description = skill.description,
        );
    }
    for skill in skipped {
        println!(
            "⚠ skipped {name}: {reason}",
            name = skill.name,
            reason = skill.reason
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
