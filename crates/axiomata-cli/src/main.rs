//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history,
//! sync the memory router, send an assistant turn, call a dashboard module
//! action through the file queue.

use anyhow::{Context, Result, bail};
use axiomata_core::agents::{self, ChatMode};
use axiomata_core::board;
use axiomata_core::bridge::{self, ActionRequest};
use axiomata_core::config::Config;
use axiomata_core::importer;
use axiomata_core::routines::{self, NewRoutine, RoutineTarget};
use axiomata_core::skills::{self, RunStatus};
use axiomata_core::{AxiomataCore, memory, paths, spend};
use clap::{ArgGroup, Args, Parser, Subcommand};

/// Clones `Config` out from under `core.config`'s `RwLock`. The CLI is a
/// one-shot process — this just keeps every call site short and consistent
/// with the dashboard's own `commands::read_config`, rather than holding a
/// guard across an `.await`.
fn read_config(core: &AxiomataCore) -> Config {
    core.config_read().clone()
}

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
    /// Show one full run record (including the captured stdout).
    GetRun {
        /// Run id, as shown by `list-runs`.
        id: i64,
    },
    /// Memory router: regenerate or inspect the workspace `CLAUDE.md` blocks.
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Bundled skills: re-copy the default four from `resources/` into
    /// `~/.axiomata/skills/`.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Scheduled routines: list, create, enable/disable, or run one poll pass.
    Routines {
        #[command(subcommand)]
        action: RoutineAction,
    },
    /// Kanban boards: inspect and drive a board without the dashboard.
    Board {
        #[command(subcommand)]
        action: BoardAction,
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
enum SkillsAction {
    /// Re-copy the bundled skills from `resources/` into `~/.axiomata/skills/`.
    ///
    /// Without `--force` this only seeds any *missing* bundled skill (the
    /// app's normal every-start behaviour). With `--force` the bundled four
    /// (`calendar-digest`, `mail-digest`, `reminders-digest`, `cleanup`) are
    /// overwritten from `resources/` — the escape hatch for the seed's
    /// seed-if-absent gotcha, where a bundled `SKILL.md` edit never reaches an
    /// install whose copy already exists. User-created skills are never
    /// touched.
    Reseed {
        /// Overwrite the bundled skills even where a copy already exists.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RoutineAction {
    /// List every routine, soonest next-fire first.
    List,
    /// Create a routine.
    Add(AddRoutine),
    /// Replace a routine's name/cron/target/backend by id — a full replace,
    /// like `add`, not a partial patch: pass every field again, not just the
    /// one that changed.
    Edit {
        /// Routine id, as shown by `routines list`.
        id: i64,
        #[command(flatten)]
        fields: AddRoutine,
    },
    /// Permanently delete a routine and its firing history by id.
    Delete {
        /// Routine id, as shown by `routines list`.
        id: i64,
    },
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

/// The board verbs exist so the whole store can be exercised without the
/// dashboard — including the two that only matter once several actors share a
/// board: `claim` (compare-and-swap) and `verify` (which refuses to let anyone
/// sign off their own work).
#[derive(Debug, Subcommand)]
enum BoardAction {
    /// List boards, or one board's columns and cards with `--board`.
    List {
        #[arg(long)]
        board: Option<i64>,
        /// Include archived cards, which are hidden by default.
        #[arg(long)]
        archived: bool,
    },
    /// Create a board (with its three default columns).
    New { name: String },
    /// Add a card to a column.
    Add {
        #[arg(long)]
        column: i64,
        title: String,
        #[arg(long)]
        body: Option<String>,
        /// Repeatable: `--label design --label rust`.
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    /// Move a card to a column, at an optional index within it (default: end).
    Move {
        id: i64,
        #[arg(long)]
        column: i64,
        #[arg(long, default_value_t = usize::MAX)]
        index: usize,
    },
    /// Take a card, if nobody else holds it.
    Claim {
        id: i64,
        #[arg(long, default_value = "human:owner")]
        actor: String,
    },
    /// Move a card into this board's first done column.
    Done { id: i64 },
    /// Sign a finished card off. Refused for the actor who claimed it.
    Verify {
        id: i64,
        #[arg(long, default_value = "human:owner")]
        actor: String,
    },
    /// Archive or restore a card.
    Archive {
        id: i64,
        /// Restore instead of archiving.
        #[arg(long)]
        undo: bool,
    },
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
    /// Backend override: "opencode" or "ollama". Defaults to the skill's
    /// own backend, or "ollama" for a raw prompt.
    #[arg(long)]
    backend: Option<String>,
    /// Create the routine disabled.
    #[arg(long)]
    disabled: bool,
}

/// Initializes `tracing`'s output so `axiomata_core`'s `tracing::info!`/
/// `warn!` calls (the routine scheduler's tick/reconcile summaries, in
/// particular) actually go somewhere — `tracing-subscriber` was a declared
/// workspace dependency that nothing ever called `.init()` on. Defaults to
/// `info`; override with `RUST_LOG` (e.g. `RUST_LOG=debug`).
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let core = AxiomataCore::init().context("failed to initialize the Axiomata-OS core engine")?;

    match cli.command.unwrap_or(Command::Status) {
        Command::Status => print_status(&core),
        Command::ListSkills => list_skills()?,
        Command::RunSkill { name } => return run_skill(&core, &name).await,
        Command::ListRuns { limit } => list_runs(&core, limit)?,
        Command::GetRun { id } => get_run(&core, id)?,
        Command::Memory { action } => match action {
            MemoryAction::Sync => memory_sync(&core)?,
            MemoryAction::Status => memory_status(&core)?,
        },
        Command::Skills { action } => match action {
            SkillsAction::Reseed { force } => skills_reseed(force)?,
        },
        Command::Routines { action } => return routines_cmd(&core, action).await,
        Command::Board { action } => return board_cmd(&core, action),
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
    let config = read_config(core);
    let root = &config.workspace_root;
    let existing = importer::existing_areas(root);
    println!("asking the agent to propose areas…");
    let reply = agents::chat_and_record(
        &config,
        &core.db,
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
        let sync = memory::sync(&config).context("memory sync after import")?;
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
    let db = core.db_lock();
    let g = axiomata_core::graph::build(&read_config(core), &db).context("building the graph")?;
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
    let reply = agents::chat_and_record(
        &read_config(core),
        &core.db,
        message,
        resume,
        mode,
        allowed_tools,
    )
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
    println!(
        "  workspace root: {}",
        read_config(core).workspace_root.display()
    );
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

/// Re-copies the bundled skills from `resources/` into `~/.axiomata/skills/`.
/// Without `--force` this is the every-start seed (missing-only); with
/// `--force` existing bundled copies are overwritten. User skills are always
/// left alone.
fn skills_reseed(force: bool) -> Result<()> {
    let dir = paths::global_skills_dir();
    skills::reseed_default_skills(force).context("reseed failed")?;
    if force {
        println!(
            "overwrote the bundled skills in {} — user skills untouched",
            dir.display()
        );
    } else {
        println!(
            "seeded any missing bundled skills in {} (no overwrite)",
            dir.display()
        );
    }
    Ok(())
}

/// Runs `name`, prints a summary, and exits non-zero if the run failed.
async fn run_skill(core: &AxiomataCore, name: &str) -> Result<()> {
    let record = skills::execute_and_record_skill(name, &read_config(core), &core.db)
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

/// Prints recent runs from the database, newest first, then a one-line spend
/// summary for the active provider (checkpoint 4).
fn list_runs(core: &AxiomataCore, limit: usize) -> Result<()> {
    let config = read_config(core);
    let db = core.db_lock();
    let runs = skills::list_runs(&db, limit).context("failed to read run history")?;
    if runs.is_empty() {
        println!("No runs recorded yet.");
    } else {
        for run in runs {
            let cost = run
                .cost_usd
                .map(|c| format!(", ${c:.4}"))
                .unwrap_or_default();
            let provider = run
                .provider
                .as_deref()
                .filter(|p| *p != "anthropic")
                .map(|p| format!(", {p}"))
                .unwrap_or_default();
            println!(
                "#{id:<4} {started}  {status:<7} {skill} ({backend}, {ms} ms{cost}{provider})",
                id = run.id,
                started = run.started_at.to_rfc3339(),
                status = run.status.as_str(),
                skill = run.skill_name,
                backend = run.backend,
                ms = run.duration_ms,
            );
        }
    }

    let summaries = spend::role_spend_summaries(&db, &config, chrono::Utc::now())
        .context("failed to compute the spend summary")?;
    for summary in &summaries {
        if summary.metered {
            let cap = summary
                .daily_cap_usd
                .map(|c| format!(" / ${c:.2} cap"))
                .unwrap_or_else(|| " (no cap)".to_string());
            println!(
                "\nspend ({role} → {provider}): ${today:.4} today{cap} · ${month:.2} this month",
                role = summary.role,
                provider = summary.provider,
                today = summary.today_usd,
                month = summary.month_usd,
            );
        } else {
            println!(
                "\nspend ({role} → {provider}): subscription-billed — not metered",
                role = summary.role,
                provider = summary.provider,
            );
        }
    }
    Ok(())
}

/// Prints one full run record by id: metadata, then the captured stdout /
/// stderr so a validator can pipe it into `jq`.
fn get_run(core: &AxiomataCore, id: i64) -> Result<()> {
    let db = core.db_lock();
    let run = skills::get_run(&db, id)
        .with_context(|| format!("failed to read run #{id}"))?
        .ok_or_else(|| anyhow::anyhow!("no run #{id}"))?;
    let cost = run
        .cost_usd
        .map(|c| format!(", ${c:.4}"))
        .unwrap_or_default();
    println!(
        "#{id} {started}→{finished}  {status}  ({backend}, {ms} ms{cost})",
        id = run.id.unwrap_or_default(),
        started = run.started_at.to_rfc3339(),
        finished = run.finished_at.to_rfc3339(),
        status = run.status.as_str(),
        backend = run.backend,
        ms = run.duration_ms,
    );
    if let Some(code) = run.exit_code {
        println!("exit code: {code}");
    }
    if let Some(err) = &run.error {
        println!("error: {err}");
    }
    println!(
        "num_turns: {}",
        run.num_turns.map_or("-".to_string(), |n| n.to_string())
    );
    if !run.stdout.trim().is_empty() {
        println!("--- stdout ---\n{}", run.stdout.trim_end());
    }
    if !run.stderr.trim().is_empty() {
        println!("--- stderr ---\n{}", run.stderr.trim_end());
    }
    Ok(())
}

/// Regenerates the workspace router `CLAUDE.md` blocks and reports what changed.
fn memory_sync(core: &AxiomataCore) -> Result<()> {
    let report = memory::sync(&read_config(core)).context("memory sync failed")?;
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
    let status = memory::status(&read_config(core)).context("memory status failed")?;
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
        RoutineAction::Edit { id, fields } => routines_edit(core, id, fields),
        RoutineAction::Delete { id } => routines_delete(core, id),
        RoutineAction::Enable { id } => routines_set_enabled(core, id, true),
        RoutineAction::Disable { id } => routines_set_enabled(core, id, false),
        RoutineAction::History { id, limit } => routines_history(core, id, limit),
        RoutineAction::Tick => routines_tick(core).await,
    }
}

/// Prints one line per routine, soonest next-fire first, followed by a
/// warning line for each row that's too corrupted to list normally (see
/// `routines::store::list_corrupted`) rather than letting it vanish silently.
fn routines_list(core: &AxiomataCore) -> Result<()> {
    let db = core.db_lock();
    let routines = routines::store::list(&db).context("failed to read routines")?;
    let corrupted = routines::store::list_corrupted(&db).context("failed to read routines")?;

    if routines.is_empty() {
        println!("No routines defined.");
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
    for routine in corrupted {
        println!(
            "⚠ #{id} is corrupted: {reason}",
            id = routine.id,
            reason = routine.reason
        );
    }
    Ok(())
}

/// Builds a [`RoutineTarget`] from `AddRoutine`'s `skill`/`prompt` pair,
/// shared by `add` and `edit` — the clap `ArgGroup` on [`AddRoutine`]
/// guarantees exactly one of the two is set.
fn routine_target_from_args(skill: Option<String>, prompt: Option<String>) -> RoutineTarget {
    match (skill, prompt) {
        (Some(name), None) => RoutineTarget::Skill(name),
        (None, Some(text)) => RoutineTarget::Prompt(text),
        _ => unreachable!("clap enforces exactly one target"),
    }
}

/// Creates a routine from the parsed `add` arguments.
fn routines_add(core: &AxiomataCore, args: AddRoutine) -> Result<()> {
    let target = routine_target_from_args(args.skill, args.prompt);

    let db = core.db_lock();
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

/// Replaces a routine's fields from the parsed `edit` arguments — a full
/// replace like `routines_add`, so `--disabled` behaves the same way here
/// too: omit it to leave the routine enabled, pass it to keep/force it
/// disabled. Editing a currently-disabled routine without `--disabled` makes
/// it enabled again, exactly like resubmitting `add` would.
fn routines_edit(core: &AxiomataCore, id: i64, args: AddRoutine) -> Result<()> {
    let target = routine_target_from_args(args.skill, args.prompt);

    let db = core.db_lock();
    let updated = routines::store::update(
        &db,
        id,
        NewRoutine {
            name: args.name,
            cron_expr: args.cron,
            target,
            backend: args.backend,
            enabled: !args.disabled,
        },
    )
    .with_context(|| format!("failed to update routine #{id}"))?;
    let Some(routine) = updated else {
        anyhow::bail!("no routine with id {id}");
    };

    println!(
        "updated routine #{id} {name:?}  next fire: {next}",
        id = routine.id,
        name = routine.name,
        next = routine
            .next_fire_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "never (cron has no future occurrence)".to_owned()),
    );
    Ok(())
}

/// Permanently deletes a routine by id.
fn routines_delete(core: &AxiomataCore, id: i64) -> Result<()> {
    let db = core.db_lock();
    let found = routines::store::delete(&db, id)
        .with_context(|| format!("failed to delete routine #{id}"))?;
    if !found {
        anyhow::bail!("no routine with id {id}");
    }
    println!("routine #{id} deleted");
    Ok(())
}

/// Enables or disables a routine by id.
fn routines_set_enabled(core: &AxiomataCore, id: i64, enabled: bool) -> Result<()> {
    let db = core.db_lock();
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
    let db = core.db_lock();
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
    let report = routines::scheduler::tick(&read_config(core), &core.db)
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

// ----------------------------------------------------------------- board ---

fn board_cmd(core: &AxiomataCore, action: BoardAction) -> Result<()> {
    match action {
        BoardAction::List { board, archived } => board_list(core, board, archived),
        BoardAction::New { name } => board_new(core, &name),
        BoardAction::Add {
            column,
            title,
            body,
            labels,
        } => board_add(core, column, &title, body, labels),
        BoardAction::Move { id, column, index } => board_move(core, id, column, index),
        BoardAction::Claim { id, actor } => board_claim(core, id, &actor),
        BoardAction::Done { id } => board_done(core, id),
        BoardAction::Verify { id, actor } => board_verify(core, id, &actor),
        BoardAction::Archive { id, undo } => board_archive(core, id, !undo),
    }
}

/// A card's one-line summary. The two signatures are the interesting part on
/// the command line — they are what a second actor needs to see before
/// deciding whether to touch the card at all.
fn card_line(card: &board::Card) -> String {
    let mut marks = String::new();
    if let Some(holder) = &card.claimed_by {
        marks.push_str(&format!("  claimed:{holder}"));
    }
    if let Some(signer) = &card.verified_by {
        marks.push_str(&format!("  verified:{signer}"));
    }
    if card.archived_at.is_some() {
        marks.push_str("  archived");
    }
    if !card.labels.is_empty() {
        marks.push_str(&format!("  [{}]", card.labels.join(", ")));
    }
    format!("#{:<4} {}{marks}", card.id, card.title)
}

fn board_list(core: &AxiomataCore, board_id: Option<i64>, archived: bool) -> Result<()> {
    let db = core.db_lock();
    let Some(board_id) = board_id else {
        let boards = board::store::list_boards(&db)?;
        if boards.is_empty() {
            println!("No boards yet. Create one with: axiomata-cli board new <name>");
            return Ok(());
        }
        for entry in boards {
            let cards = board::store::count_cards(&db, entry.id)?;
            println!("#{:<4} {}  ({cards} cards)", entry.id, entry.name);
        }
        return Ok(());
    };

    let Some(entry) = board::store::get_board(&db, board_id)? else {
        bail!("no board with id {board_id}");
    };
    println!("{} (#{})", entry.name, entry.id);
    let columns = board::store::list_columns(&db, board_id)?;
    let cards = board::store::list_cards(&db, board_id, archived)?;
    for column in columns {
        let held: Vec<&board::Card> = cards
            .iter()
            .filter(|card| card.column_id == column.id)
            .collect();
        println!(
            "\n  {} [#{} · {}]  {} cards",
            column.name,
            column.id,
            column.maps_to_status.as_str(),
            held.len()
        );
        if held.is_empty() {
            println!("    —");
        }
        for card in held {
            println!("    {}", card_line(card));
        }
    }
    Ok(())
}

fn board_new(core: &AxiomataCore, name: &str) -> Result<()> {
    let mut db = core.db_lock();
    let created = board::store::create_board(&mut db, name)
        .with_context(|| format!("failed to create board {name:?}"))?;
    let columns = board::store::list_columns(&db, created.id)?;
    println!("created board #{} {:?}", created.id, created.name);
    for column in columns {
        println!(
            "  column #{:<4} {}  ({})",
            column.id,
            column.name,
            column.maps_to_status.as_str()
        );
    }
    Ok(())
}

fn board_add(
    core: &AxiomataCore,
    column: i64,
    title: &str,
    body: Option<String>,
    labels: Vec<String>,
) -> Result<()> {
    let db = core.db_lock();
    let card = board::store::create_card(
        &db,
        &board::NewCard {
            column_id: column,
            fields: board::CardFields {
                title: title.to_string(),
                body: body.unwrap_or_default(),
                labels,
                assignee: None,
                due_at: None,
            },
        },
    )
    .with_context(|| format!("failed to add a card to column #{column}"))?;
    println!("created card #{} {:?}", card.id, card.title);
    Ok(())
}

fn board_move(core: &AxiomataCore, id: i64, column: i64, index: usize) -> Result<()> {
    let mut db = core.db_lock();
    // `usize::MAX` is the "no --index given" sentinel; the store clamps an
    // out-of-range index to the end of the column on its own.
    if !board::store::move_card(&mut db, id, column, index)? {
        bail!("no card with id {id}");
    }
    println!("card #{id} moved to column #{column}");
    Ok(())
}

fn board_claim(core: &AxiomataCore, id: i64, actor: &str) -> Result<()> {
    let db = core.db_lock();
    if board::store::claim_card(&db, id, actor)? {
        println!("card #{id} claimed by {actor}");
        return Ok(());
    }
    // Losing the race is an ordinary outcome, so say who holds it rather than
    // failing with a bare "no".
    match board::store::get_card(&db, id)? {
        None => bail!("no card with id {id}"),
        Some(card) => {
            let holder = card.claimed_by.unwrap_or_else(|| "somebody".to_string());
            bail!("card #{id} is already claimed by {holder}");
        }
    }
}

fn board_done(core: &AxiomataCore, id: i64) -> Result<()> {
    let mut db = core.db_lock();
    // "Which column means done" is answered once, in the store, so the
    // dashboard cannot later answer it differently.
    match board::store::move_to_status(&mut db, id, board::CardStatus::Done)? {
        Some(column) => {
            println!("card #{id} moved to {:?}", column.name);
            Ok(())
        }
        None if board::store::get_card(&db, id)?.is_none() => bail!("no card with id {id}"),
        None => bail!("card #{id} is on a board with no done column"),
    }
}

fn board_verify(core: &AxiomataCore, id: i64, actor: &str) -> Result<()> {
    let db = core.db_lock();
    if board::store::verify_card(&db, id, actor)? {
        println!("card #{id} verified by {actor}");
        return Ok(());
    }
    // Read the reason only *after* the attempt: checking first would reopen
    // the very race the single-statement rule exists to close.
    let reason = match board::store::explain_verify_refusal(&db, id, actor)? {
        Some(board::store::VerifyRefusal::NoSuchCard) => format!("no card with id {id}"),
        Some(board::store::VerifyRefusal::AlreadyVerified) => {
            format!("card #{id} is already verified")
        }
        Some(board::store::VerifyRefusal::NotClaimed) => {
            format!("card #{id} has not been claimed by anyone yet")
        }
        Some(board::store::VerifyRefusal::SelfVerify) => {
            // The stored claimant, not the spelling that was typed: actor
            // strings are canonicalised, so echoing the input back would show
            // a form that is not what the card actually holds.
            let holder = board::store::get_card(&db, id)?
                .and_then(|card| card.claimed_by)
                .unwrap_or_else(|| actor.to_string());
            format!("card #{id} is claimed by {holder} — verification needs a second party")
        }
        Some(board::store::VerifyRefusal::NotDone) => {
            format!("card #{id} is not in a done column")
        }
        Some(board::store::VerifyRefusal::Archived) => {
            format!("card #{id} is archived — restore it first with: board archive {id} --undo")
        }
        None => format!("card #{id} could not be verified"),
    };
    bail!("{reason}");
}

fn board_archive(core: &AxiomataCore, id: i64, archived: bool) -> Result<()> {
    let db = core.db_lock();
    if !board::store::set_card_archived(&db, id, archived)? {
        bail!("no card with id {id}");
    }
    println!(
        "card #{id} {}",
        if archived { "archived" } else { "restored" }
    );
    Ok(())
}
