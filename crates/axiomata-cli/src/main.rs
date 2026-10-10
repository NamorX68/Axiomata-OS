//! Minimal CLI for exercising the Axiomata-OS core engine end-to-end without
//! the Tauri GUI: initialize the core, list and run skills, inspect run history,
//! sync the memory router, send an assistant turn, call a dashboard module
//! action through the file queue.

mod files_cmd;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use axiomata_core::agents::{self, ChatMode};
use axiomata_core::board;
use axiomata_core::board_mirror;
use axiomata_core::bridge::{self, ActionRequest};
use axiomata_core::config::Config;
use axiomata_core::ide;
use axiomata_core::importer;
use axiomata_core::routines::{self, NewRoutine, RoutineTarget};
use axiomata_core::skills::{self, RunStatus};
use axiomata_core::{AxiomataCore, memory, paths, spend};
use clap::{ArgGroup, Args, Parser, Subcommand};

mod board_flow;

/// Clones `Config` out from under `core.config`'s `RwLock`. The CLI is a
/// one-shot process — this just keeps every call site short and consistent
/// with the dashboard's own `commands::read_config`, rather than holding a
/// guard across an `.await`.
pub(crate) fn read_config(core: &AxiomataCore) -> Config {
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
    /// Memory router: regenerate or inspect the workspace `AGENTS.md` blocks.
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
    /// File service (editor plan ED0): roots, guarded reads/writes, grants.
    Files {
        #[command(subcommand)]
        action: files_cmd::FilesAction,
    },
    /// Agentic IDE (M7.1): manage the projects the IDE works in.
    Ide {
        #[command(subcommand)]
        action: IdeAction,
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
    /// Serve the MCP tools (mailbox, board steps) for one agent session on stdin/stdout. Started by the studio for a
    /// session, with `AXIOMATA_AGENT_ID` in the environment; not meant to be run by hand.
    McpServe,
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
    /// app's normal every-start behaviour). With `--force` the bundled skills
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

/// The IDE's project verbs, so CP0 is fully exercisable before a single line
/// of the IDE view exists — the same order the board was built in.
#[derive(Debug, Subcommand)]
enum IdeAction {
    /// Projects: the folders the IDE works in.
    Projects {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Agents: the profiles a project can run in its panes.
    Agents {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Engines: the owner's catalog of harness + model + environment (a2a.md, CP-A1).
    Engines {
        #[command(subcommand)]
        action: EngineAction,
    },
    /// Roles: what an agent is for — one `AGENT.md` each under `~/.axiomata/agents/` (a2a.md, CP-A1).
    Roles {
        #[command(subcommand)]
        action: RoleAction,
    },
}

#[derive(Debug, Subcommand)]
enum EngineAction {
    /// List the engines and how many agent sessions run on each.
    List,
    /// Add an engine.
    Add {
        /// Slug sessions and roles refer to (lower-case letters, digits, - and _).
        id: String,
        /// What the UI shows, e.g. "Claude Code · Opus".
        #[arg(long)]
        label: String,
        /// claude_code | opencode | mini.
        #[arg(long, default_value = "opencode")]
        harness: String,
        /// Command line to run. Empty uses the harness's own default.
        #[arg(long, default_value = "")]
        command: String,
        /// Model to pass on (for Opencode the full `provider/model`). Omitted lets the harness pick.
        #[arg(long)]
        model: Option<String>,
        /// Extra environment, `KEY=value` per line.
        #[arg(long, default_value = "")]
        env: String,
        /// metered | subscription.
        #[arg(long, default_value = "metered")]
        billing: String,
    },
    /// Change an engine. A flag left out **keeps its current value**.
    Edit {
        id: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        billing: Option<String>,
    },
    /// Remove an engine. Refused while a session or a role still uses it.
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
enum RoleAction {
    /// List the owner's roles, and any that could not be read.
    List,
    /// Print a role's `AGENT.md`.
    Show { name: String },
    /// Save a role from an `AGENT.md` file (replacing one of the same name).
    Save { file: PathBuf },
    /// Delete a role. Refused while an agent session plays it.
    Delete { name: String },
    /// What a project brings in roles (`.axiomata/agents/`), whether you confirmed it, and what applies.
    Project { project: i64 },
    /// Confirm the project's role files exactly as `roles project` showed them (pass its hash).
    Confirm { project: i64, hash: String },
}

/// Agent profiles (M7.2 CP4), on the CLI for the same reason projects are:
/// the whole store is exercisable before a pane exists to show one in.
///
/// Note what is *not* here — starting or stopping an agent. A running agent is
/// a PTY session owned by the pane showing it, and it does not outlive the
/// app; the CLI has no window to put one in.
#[derive(Debug, Subcommand)]
enum AgentAction {
    /// List a project's agents, by name.
    List { project: i64 },
    /// Add an agent profile to a project.
    New {
        project: i64,
        name: String,
        /// claude_code | opencode | mini.
        #[arg(long, default_value = "opencode")]
        harness: String,
        /// Command line to run. Empty uses the harness's own default.
        #[arg(long, default_value = "")]
        command: String,
        /// Model to pass on. Omitted lets the harness pick.
        #[arg(long)]
        model: Option<String>,
        /// Extra environment, `KEY=value` per line.
        #[arg(long, default_value = "")]
        env: String,
    },
    /// Change an agent's fields. A flag left out **keeps its current value**.
    ///
    /// Patch semantics here, unlike `AgentFields` itself, which is a full
    /// replace: on a command line "I did not type --env" means "leave it
    /// alone", and clearing every field somebody forgot to repeat would be a
    /// trap. The store still receives every field — this merges first.
    Edit {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        env: Option<String>,
    },
    /// Remove an agent profile.
    Delete { id: i64 },
    /// Give an agent its worktree, port and status channel — and an Opencode
    /// agent its session on the Opencode service — and print where and how it
    /// would run. Idempotent — this is what the app does on every start.
    Prepare { id: i64 },
    /// Forget an Opencode agent's session, so its next start opens a fresh one.
    NewSession { id: i64 },
    /// What a project's agents are doing and planning, as their harnesses
    /// last reported it.
    Status { project: i64 },
    /// Remove an agent's worktree. Refuses to throw away uncommitted work
    /// unless `--force`.
    DiscardWorktree {
        id: i64,
        #[arg(long)]
        force: bool,
    },
    /// What the agent has changed since its base branch (M7.3); with
    /// `--file`, that file's diff.
    Diff {
        id: i64,
        #[arg(long)]
        file: Option<String>,
    },
    /// Print a file as the agent's base branch has it (the diff's left side),
    /// byte for byte.
    Base { id: i64, path: String },
    /// Commit everything the agent left uncommitted in its worktree.
    Commit {
        id: i64,
        #[arg(long, short)]
        message: String,
    },
    /// Put files back to how they are on the agent's base branch — committed
    /// changes included.
    Discard { id: i64, paths: Vec<String> },
    /// Put one hunk of a file back to the base — the `index`-th (from 0) of
    /// `ide agents diff <id> --file <path>`.
    DiscardHunk { id: i64, path: String, index: usize },
    /// Take the agent's committed work over into the project folder: one
    /// squash commit, or a merge commit with `--no-ff`. Never pushes.
    TakeOver {
        id: i64,
        #[arg(long, short)]
        message: String,
        #[arg(long)]
        no_ff: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectAction {
    /// List projects, most recently opened first.
    List,
    /// Add a project for an existing folder.
    New {
        name: String,
        /// The project's folder. Stored absolute and canonicalised.
        path: PathBuf,
    },
    /// Rename a project. Its folder is untouched.
    Rename { id: i64, name: String },
    /// Repoint a project at a different folder, keeping its id and layout —
    /// what the UI offers as "Pfad ändern…" when a folder has moved.
    SetRoot { id: i64, path: PathBuf },
    /// Remove a project from the list. **Never** deletes the folder.
    Delete { id: i64 },
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
    /// Rename a board. Its workspace mirror follows, old file swept up.
    Rename { id: i64, name: String },
    /// Delete a board with every column and card on it, and its mirror.
    Delete {
        id: i64,
        /// Required once the board holds cards — they go with it.
        #[arg(long)]
        force: bool,
    },
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
        #[command(flatten)]
        flow: board_flow::FlowFlags,
    },
    /// Change a card's title, body or labels; whatever is not passed keeps its value.
    Edit {
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// Repeatable, and replaces all labels: `--label error --label mail`.
        #[arg(long = "label", conflicts_with = "clear_labels")]
        labels: Vec<String>,
        /// Remove every label.
        #[arg(long)]
        clear_labels: bool,
        #[command(flatten)]
        flow: board_flow::FlowFlags,
        /// Take the card out of its plan (its dependencies must be removed first).
        #[arg(long, conflicts_with = "plan")]
        no_plan: bool,
    },
    /// Move a card to a column, at an optional index within it (default: end). Moving an unclaimed card into the
    /// review column claims it for `--actor`, so somebody else can sign it off.
    Move {
        id: i64,
        #[arg(long)]
        column: i64,
        #[arg(long, default_value_t = usize::MAX)]
        index: usize,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Take a card, if nobody else holds it.
    Claim {
        id: i64,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Start a card: make a session of its role for it in a project, take the card for it, and print how the session
    /// starts (the same as `ide agents prepare`). The owner's step.
    Start {
        id: i64,
        /// The project (repository) the session works in.
        #[arg(long)]
        project: i64,
        /// An engine of the catalog; without it the role's own engine is used.
        #[arg(long)]
        engine: Option<String>,
    },
    /// Start the review of a reported card by hand: a reviewer session on an engine other than the worker's, in a
    /// detached checkout of the work. The studio does this on its own when the app runs. The owner's step.
    Review {
        id: i64,
        /// An engine of the catalog; without it the reviewer role's own (or a fallback) is used.
        #[arg(long)]
        engine: Option<String>,
        /// Review even when the work changes files agents read their configuration from (`.claude/`, `.mcp.json`,
        /// `opencode.json`, `AGENTS.md` …): the reviewer would obey what the worker wrote there.
        #[arg(long)]
        allow_agent_config: bool,
    },
    /// Take a card the reviewer signed off over into the project's main line as one commit, close the card and clean
    /// up the sessions made for it. The owner's step.
    TakeOver {
        id: i64,
        /// The commit message; without it `#<card> <title>`.
        #[arg(long)]
        message: Option<String>,
    },
    /// Take a finished plan over into the project's branch: the line of its cards (their commits as they are while the
    /// branch has not moved, else one merge commit), the cards and the plan are closed, the line is removed. The owner's
    /// step; never pushes.
    TakeOverPlan { id: i64 },
    /// What the sessions of a card have used and what they may use (steps, tokens, money). Read from what the
    /// harnesses left behind; a session that is stopped at a limit says so.
    Usage { id: i64 },
    /// Merge a card the reviewer signed off into the line of its plan (a plan that runs by itself does this on its own).
    /// The owner's step; for a card the studio has not got to yet, or after a conflict was looked at.
    Integrate { id: i64 },
    /// Have a card the studio gave up integrating (it did not fit the plan's line twice) done again: it goes back to the
    /// open column, its sessions are cleaned up, and the plan starts it again on the line as it is now. The owner's step.
    Redo { id: i64 },
    /// Give a started card back: the claim is dropped and the card waits in its open column again. The owner's step.
    Release { id: i64 },
    /// Move a card into this board's first done column.
    Done { id: i64 },
    /// Sign a finished card off. Refused for the actor who claimed it.
    Verify {
        id: i64,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Archive or restore a card.
    Archive {
        id: i64,
        /// Restore instead of archiving.
        #[arg(long)]
        undo: bool,
    },
    /// Plans: the unit of approval, automation and limits.
    Plan {
        #[command(subcommand)]
        action: board_flow::PlanAction,
    },
    /// Dependencies between the cards of one plan.
    Dep {
        #[command(subcommand)]
        action: board_flow::DepAction,
    },
    /// The worker reports a card done: it moves to the review column.
    Report {
        id: i64,
        #[arg(long)]
        actor: Option<String>,
    },
    /// A reviewer's verdict on a card in the review column: `--approve`, or `--return --note "why"`.
    Verdict {
        id: i64,
        #[arg(long)]
        actor: Option<String>,
        #[arg(
            long,
            conflicts_with = "send_back",
            required_unless_present = "send_back"
        )]
        approve: bool,
        /// Send the card back to work (needs --note).
        #[arg(long = "return", id = "send_back")]
        send_back: bool,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// A card's history, latest last.
    Events {
        id: i64,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Write a line into a card's history.
    Note {
        id: i64,
        text: String,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Say what a working card waits for an answer to (`--ask "…"`), or clear it (`--clear`).
    Input {
        id: i64,
        #[arg(long, conflicts_with = "clear", required_unless_present = "clear")]
        ask: Option<String>,
        #[arg(long)]
        clear: bool,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Mark a card failed. Cards waiting for it stay blocked.
    Fail {
        id: i64,
        reason: String,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Call a card off. Cards waiting for it stay blocked.
    Cancel {
        id: i64,
        reason: String,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Undo a failure or cancellation.
    Reopen {
        id: i64,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Record that the finished work was taken over into the main line; the card is archived.
    TakenOver {
        id: i64,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Say yes to a proposal: it moves out of the proposal column to Offen.
    Approve {
        id: i64,
        /// Who acts. Defaults to the owner in your own terminal; in an agent session it is always that agent.
        #[arg(long)]
        actor: Option<String>,
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
    // stderr, not the default stdout: `mcp-serve` speaks the MCP protocol on stdout, and one log line there would be a
    // message the client cannot parse.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
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
            SkillsAction::Reseed { force } => {
                board_flow::owner_only("reseeding the bundled skills")?;
                skills_reseed(force)?
            }
        },
        Command::Routines { action } => return routines_cmd(&core, action).await,
        Command::Board { action } => return board_cmd(&core, action).await,
        Command::Files { action } => files_cmd::run(&core, action)?,
        Command::Ide { action } => return ide_cmd(&core, action).await,
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
        Command::McpServe => return mcp_serve(&core),
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

/// `mcp-serve`: the MCP server of one agent session (`docs/plans/a2a.md` CP-A4, `axiomata_core::agent_mcp`). Everything
/// but the protocol goes to stderr.
fn mcp_serve(core: &AxiomataCore) -> Result<()> {
    let roots = axiomata_core::paths::ide_locations().channels;
    let ctx = axiomata_core::agent_mcp::Context::from_env(core, roots)?;
    axiomata_core::agent_mcp::serve_stdio(&ctx).context("serving MCP on stdio")
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
            "memory router synced: {} AGENTS.md written, {} tracked files (session {}, ${:.2})",
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

/// Regenerates the workspace router `AGENTS.md` blocks and reports what changed.
fn memory_sync(core: &AxiomataCore) -> Result<()> {
    let report = memory::sync(&read_config(core)).context("memory sync failed")?;
    if report.written.is_empty() {
        println!(
            "Router already in sync — {} tracked files, {} AGENTS.md file(s) unchanged.",
            report.tracked_files, report.unchanged,
        );
    } else {
        println!("Wrote {} AGENTS.md file(s):", report.written.len());
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
    // A routine runs an agent on a schedule: an agent session does not schedule itself more work.
    if !matches!(action, RoutineAction::List | RoutineAction::History { .. }) {
        board_flow::owner_only("changing or running routines")?;
    }
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

async fn board_cmd(core: &AxiomataCore, action: BoardAction) -> Result<()> {
    use board_flow::{mcp_only, owner_only, resolve_actor};
    match action {
        BoardAction::List { board, archived } => board_list(core, board, archived),
        BoardAction::New { name } => {
            owner_only("creating a board")?;
            board_new(core, &name)
        }
        BoardAction::Rename { id, name } => {
            owner_only("renaming a board")?;
            board_rename(core, id, &name)
        }
        BoardAction::Delete { id, force } => {
            owner_only("deleting a board")?;
            board_delete(core, id, force).await
        }
        BoardAction::Add {
            column,
            title,
            body,
            labels,
            flow,
        } => {
            mcp_only("proposing a card")?;
            board_add(core, column, &title, body, labels, &flow)
        }
        BoardAction::Edit {
            id,
            title,
            body,
            labels,
            clear_labels,
            flow,
            no_plan,
        } => {
            if flow.plan.is_some() || no_plan {
                owner_only("putting a card into a plan or taking it out")?;
            }
            board_edit(
                core,
                id,
                CardEdit {
                    title,
                    body,
                    labels,
                    clear_labels,
                    no_plan,
                },
                &flow,
            )
        }
        BoardAction::Move {
            id,
            column,
            index,
            actor,
        } => board_move(core, id, column, index, &resolve_actor(actor)?),
        BoardAction::Claim { id, actor } => {
            mcp_only("taking a card")?;
            board_claim(core, id, &resolve_actor(actor)?)
        }
        BoardAction::Start {
            id,
            project,
            engine,
        } => {
            owner_only("starting a card")?;
            board_start(core, id, project, engine).await
        }
        BoardAction::Review {
            id,
            engine,
            allow_agent_config,
        } => {
            owner_only("starting a review")?;
            board_review(core, id, engine, allow_agent_config).await
        }
        BoardAction::TakeOver { id, message } => {
            owner_only("taking a card over")?;
            board_take_over(core, id, message).await
        }
        BoardAction::Integrate { id } => {
            owner_only("integrating a card")?;
            board_integrate(core, id).await
        }
        BoardAction::Redo { id } => {
            owner_only("having a card done again")?;
            let ended = axiomata_core::card_session::redo_card(core, id).await?;
            println!(
                "card #{id} goes back to the open column; {} session(s) ended",
                ended.len()
            );
            Ok(())
        }
        BoardAction::TakeOverPlan { id } => {
            owner_only("taking a plan over")?;
            board_take_over_plan(core, id).await
        }
        BoardAction::Usage { id } => board_usage(core, id).await,
        BoardAction::Release { id } => {
            owner_only("giving a card back")?;
            board_release(core, id)
        }
        BoardAction::Done { id } => {
            owner_only("moving a card to done")?;
            board_done(core, id)
        }
        BoardAction::Verify { id, actor } => {
            owner_only("signing off a card outside the review (agents use `board verdict`)")?;
            board_verify(core, id, &resolve_actor(actor)?)
        }
        BoardAction::Archive { id, undo } => {
            owner_only("archiving a card")?;
            board_archive(core, id, !undo)
        }
        BoardAction::Plan { action } => board_plan(core, action).await,
        BoardAction::Dep { action } => board_flow::dep_cmd(core, action),
        BoardAction::Report { id, actor } => {
            mcp_only("reporting a card done")?;
            board_flow::report(core, id, &resolve_actor(actor)?)
        }
        BoardAction::Verdict {
            id,
            actor,
            approve,
            send_back: _,
            note,
        } => {
            mcp_only("judging a card")?;
            board_flow::verdict(core, id, &resolve_actor(actor)?, approve, &note)
        }
        BoardAction::Events { id, limit } => board_flow::events(core, id, limit),
        BoardAction::Note { id, text, actor } => {
            board_flow::note(core, id, &resolve_actor(actor)?, &text)
        }
        BoardAction::Input {
            id,
            ask,
            clear: _,
            actor,
        } => board_flow::input(core, id, &resolve_actor(actor)?, ask.as_deref()),
        BoardAction::Fail { id, reason, actor } => board_flow::mark(
            core,
            id,
            &resolve_actor(actor)?,
            board_flow::Mark::Fail,
            &reason,
        ),
        BoardAction::Cancel { id, reason, actor } => board_flow::mark(
            core,
            id,
            &resolve_actor(actor)?,
            board_flow::Mark::Cancel,
            &reason,
        ),
        BoardAction::Reopen { id, actor } => {
            owner_only("reopening a card")?;
            board_flow::mark(
                core,
                id,
                &resolve_actor(actor)?,
                board_flow::Mark::Reopen,
                "",
            )
        }
        BoardAction::TakenOver { id, actor } => {
            owner_only("recording a take-over")?;
            board_flow::mark(
                core,
                id,
                &resolve_actor(actor)?,
                board_flow::Mark::TakeOver,
                "",
            )
        }
        BoardAction::Approve { id, actor } => {
            owner_only("approving a proposal")?;
            board_flow::approve_proposal(core, id, &resolve_actor(actor)?)
        }
    }
}

/// What an agent session may not do with `ide …`, by command: start sessions (which issues their secrets), change
/// what a session runs, or edit the owner's projects, engines and roles. Reading stays open (`docs/plans/a2a.md`,
/// A39/CP-A5: without this an agent could run `ide agents prepare <reviewer>` and read the fresh secret from the
/// file it writes).
fn ide_owner_gate(action: &IdeAction) -> Option<&'static str> {
    match action {
        IdeAction::Projects { action } => match action {
            ProjectAction::List => None,
            _ => Some("changing a project"),
        },
        IdeAction::Agents { action } => match action {
            AgentAction::List { .. }
            | AgentAction::Status { .. }
            | AgentAction::Diff { .. }
            | AgentAction::Base { .. } => None,
            _ => Some("starting, changing or taking over a session"),
        },
        IdeAction::Engines { action } => match action {
            EngineAction::List => None,
            _ => Some("changing the engine catalog"),
        },
        IdeAction::Roles { action } => match action {
            RoleAction::List | RoleAction::Show { .. } | RoleAction::Project { .. } => None,
            _ => Some("changing roles"),
        },
    }
}

async fn ide_cmd(core: &AxiomataCore, action: IdeAction) -> Result<()> {
    if let Some(what) = ide_owner_gate(&action) {
        board_flow::owner_only(what)?;
    }
    match action {
        IdeAction::Projects { action } => match action {
            ProjectAction::List => project_list(core),
            ProjectAction::New { name, path } => project_new(core, &name, &path),
            ProjectAction::Rename { id, name } => project_rename(core, id, &name),
            ProjectAction::SetRoot { id, path } => project_set_root(core, id, &path),
            ProjectAction::Delete { id } => project_delete(core, id),
        },
        IdeAction::Agents { action } => match action {
            AgentAction::List { project } => agent_list(core, project),
            AgentAction::New {
                project,
                name,
                harness,
                command,
                model,
                env,
            } => agent_new(core, project, &name, &harness, &command, model, &env),
            AgentAction::Edit {
                id,
                name,
                harness,
                command,
                model,
                env,
            } => agent_edit(core, id, name, harness, command, model, env),
            AgentAction::Delete { id } => agent_delete(core, id).await,
            AgentAction::Prepare { id } => agent_prepare(core, id).await,
            AgentAction::NewSession { id } => {
                if axiomata_core::ide_start::new_session(&core.db, id)? {
                    println!("agent #{id} starts a fresh Opencode session next time");
                    Ok(())
                } else {
                    anyhow::bail!("no agent #{id}")
                }
            }
            AgentAction::Status { project } => agent_status(core, project).await,
            AgentAction::DiscardWorktree { id, force } => {
                agent_discard_worktree(core, id, force).await
            }
            AgentAction::Diff { id, file } => agent_diff(core, id, file),
            AgentAction::Base { id, path } => agent_base(core, id, &path),
            AgentAction::Commit { id, message } => {
                let commit = agent_repo_or_bail(core, id)?.commit_all(&message)?;
                println!("committed {commit}");
                Ok(())
            }
            AgentAction::Discard { id, paths } => {
                agent_repo_or_bail(core, id)?.discard(&paths)?;
                println!("put {} file(s) back to the base", paths.len());
                Ok(())
            }
            AgentAction::DiscardHunk { id, path, index } => {
                let repo = agent_repo_or_bail(core, id)?;
                let diff = repo.file_diff(&path, None)?;
                let Some(hunk) = diff.hunks.get(index) else {
                    bail!(
                        "{path} has {} hunk(s); there is no hunk {index}",
                        diff.hunks.len()
                    );
                };
                repo.discard_hunk(&path, None, index, &hunk.header)?;
                println!("put hunk {index} of {path} back to the base");
                Ok(())
            }
            AgentAction::TakeOver { id, message, no_ff } => {
                agent_take_over(core, id, &message, no_ff)
            }
        },
        IdeAction::Engines { action } => engine_cmd(core, action),
        IdeAction::Roles { action } => role_cmd(core, action),
    }
}

fn parse_billing(raw: &str) -> Result<axiomata_roster::Billing> {
    match raw {
        "metered" => Ok(axiomata_roster::Billing::Metered),
        "subscription" => Ok(axiomata_roster::Billing::Subscription),
        _ => bail!("unknown billing {raw:?} — expected metered or subscription"),
    }
}

fn billing_name(billing: axiomata_roster::Billing) -> &'static str {
    match billing {
        axiomata_roster::Billing::Metered => "metered",
        axiomata_roster::Billing::Subscription => "subscription",
    }
}

fn config_snapshot(core: &AxiomataCore) -> Config {
    core.config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Stores `config`'s engines back into the live config after the roster changed them.
fn adopt_engines(core: &AxiomataCore, config: Config) {
    core.config
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .agents
        .engines = config.agents.engines;
}

fn engine_cmd(core: &AxiomataCore, action: EngineAction) -> Result<()> {
    use axiomata_core::roster;
    match action {
        EngineAction::List => {
            let config = config_snapshot(core);
            if config.agents.engines.is_empty() {
                println!("no engines — add one with `ide engines add <id> --label …`");
                return Ok(());
            }
            let db = core.db_lock();
            for engine in config.agents.engines.values() {
                let sessions: i64 = db.query_row(
                    "SELECT COUNT(*) FROM ide_agents WHERE engine_id = ?1",
                    [&engine.id],
                    |row| row.get(0),
                )?;
                println!(
                    "{:<32} {:<12} {:<13} {} session(s)  {}  (model: {})",
                    engine.id,
                    engine.harness.as_str(),
                    billing_name(engine.billing),
                    sessions,
                    engine.label,
                    engine.model.as_deref().unwrap_or("harness default"),
                );
            }
            Ok(())
        }
        EngineAction::Add {
            id,
            label,
            harness,
            command,
            model,
            env,
            billing,
        } => {
            let mut config = config_snapshot(core);
            if config.agents.engines.contains_key(&id) {
                bail!("there is already an engine {id:?} — use `ide engines edit`");
            }
            roster::save_engine(
                &mut config,
                axiomata_roster::Engine {
                    id: id.clone(),
                    label,
                    harness: parse_harness(&harness)?,
                    command,
                    model,
                    env,
                    billing: parse_billing(&billing)?,
                },
            )?;
            adopt_engines(core, config);
            println!("added engine {id}");
            Ok(())
        }
        EngineAction::Edit {
            id,
            label,
            harness,
            command,
            model,
            env,
            billing,
        } => {
            let mut config = config_snapshot(core);
            let Some(existing) = config.agents.engines.get(&id).cloned() else {
                bail!("no engine {id:?}");
            };
            roster::save_engine(
                &mut config,
                axiomata_roster::Engine {
                    id: id.clone(),
                    label: label.unwrap_or(existing.label),
                    harness: match harness {
                        Some(raw) => parse_harness(&raw)?,
                        None => existing.harness,
                    },
                    command: command.unwrap_or(existing.command),
                    model: model.or(existing.model),
                    env: env.unwrap_or(existing.env),
                    billing: match billing {
                        Some(raw) => parse_billing(&raw)?,
                        None => existing.billing,
                    },
                },
            )?;
            adopt_engines(core, config);
            println!("updated engine {id}");
            Ok(())
        }
        EngineAction::Delete { id } => {
            let mut config = config_snapshot(core);
            let removed = {
                let db = core.db_lock();
                roster::delete_engine(&db, &mut config, &id)?
            };
            if !removed {
                bail!("no engine {id:?}");
            }
            adopt_engines(core, config);
            println!("removed engine {id}");
            Ok(())
        }
    }
}

fn role_cmd(core: &AxiomataCore, action: RoleAction) -> Result<()> {
    use axiomata_core::roster;
    match action {
        RoleAction::List => {
            let loaded = roster::list_roles()?;
            for role in &loaded.roles {
                println!(
                    "{:<24} {:<10} {:<7} engine: {:<28} {}",
                    role.name,
                    role.kind,
                    format!("{:?}", role.tier).to_lowercase(),
                    role.engine.as_deref().unwrap_or("(chosen at start)"),
                    role.description,
                );
            }
            for skipped in &loaded.skipped {
                println!(
                    "skipped {}: {}",
                    skipped.name.escape_debug(),
                    skipped.reason.escape_debug()
                );
            }
            if loaded.roles.is_empty() && loaded.skipped.is_empty() {
                println!("no roles under {}", paths::agent_roles_dir().display());
            }
            Ok(())
        }
        RoleAction::Show { name } => {
            let loaded = roster::list_roles()?;
            let Some(role) = loaded.roles.iter().find(|r| r.name == name) else {
                bail!("no role {name:?}");
            };
            print!("{}", role.to_markdown());
            Ok(())
        }
        RoleAction::Save { file } => {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("could not read {}", file.display()))?;
            let role = axiomata_roster::Role::parse(&text)?;
            let name = role.name.clone();
            roster::save_role(&config_snapshot(core), role)?;
            println!("saved role {name}");
            Ok(())
        }
        RoleAction::Delete { name } => {
            let db = core.db_lock();
            if !roster::delete_role(&db, &name)? {
                bail!("no role {name:?}");
            }
            println!("deleted role {name}");
            Ok(())
        }
        RoleAction::Project { project } => {
            let root = project_root(core, project)?;
            let found = roster::project_roles(&root, &config_snapshot(core))?;
            if !found.overrides.present {
                println!("the project brings no roles of its own");
            } else {
                println!(
                    "{} role file(s) in {} — hash {} — {}",
                    found.overrides.roles.len() + found.overrides.skipped.len(),
                    found.overrides.dir.display(),
                    found.overrides.hash,
                    if found.confirmed {
                        "confirmed"
                    } else {
                        "NOT confirmed, so they do not apply (`ide roles confirm`)"
                    },
                );
                for skipped in &found.overrides.skipped {
                    println!(
                        "  skipped {}: {}",
                        skipped.name.escape_debug(),
                        skipped.reason.escape_debug()
                    );
                }
            }
            for role in &found.effective {
                println!(
                    "  {:<24} {:?}  {}",
                    role.name, role.source, role.description
                );
            }
            Ok(())
        }
        RoleAction::Confirm { project, hash } => {
            let root = project_root(core, project)?;
            roster::confirm_project_roles(&root, &hash)?;
            println!("confirmed the role files of project #{project}");
            Ok(())
        }
    }
}

fn project_root(core: &AxiomataCore, project: i64) -> Result<PathBuf> {
    let db = core.db_lock();
    match ide::store::get_project(&db, project)? {
        Some(found) => Ok(found.repo_root),
        None => bail!("no project #{project}"),
    }
}

/// Parses a harness name, listing the valid ones when it is not one of them.
fn parse_harness(raw: &str) -> Result<ide::Harness> {
    ide::Harness::parse(raw).ok_or_else(|| {
        anyhow::anyhow!("unknown harness {raw:?} — expected claude_code, opencode or mini")
    })
}

/// Gives a new or changed agent its engine (a2a.md, CP-A1). A failure only leaves it unassigned until the next
/// start, so it is reported, not raised.
fn assign_engines(core: &AxiomataCore, db: &rusqlite::Connection) {
    if let Err(err) = axiomata_core::roster::sync_live(db, &core.config) {
        eprintln!("note: could not assign an engine to the agent yet: {err}");
    }
}

fn agent_list(core: &AxiomataCore, project_id: i64) -> Result<()> {
    let db = core.db_lock();
    if ide::store::get_project(&db, project_id)?.is_none() {
        anyhow::bail!("no project #{project_id}");
    }
    let agents = ide::agent_store::list_agents(&db, project_id)?;
    if agents.is_empty() {
        println!(
            "no agents in project #{project_id} — add one with `ide agents new <project> <name>`"
        );
        return Ok(());
    }
    for agent in agents {
        let model = agent
            .model
            .clone()
            .unwrap_or_else(|| "harness default".into());
        println!(
            "#{:<4} {:<20} {:<12} {}  (model: {model})",
            agent.id,
            agent.name,
            agent.harness.as_str(),
            agent.effective_command,
        );
    }
    Ok(())
}

fn agent_new(
    core: &AxiomataCore,
    project_id: i64,
    name: &str,
    harness: &str,
    command: &str,
    model: Option<String>,
    env: &str,
) -> Result<()> {
    let db = core.db_lock();
    let created = ide::agent_store::create_agent(
        &db,
        ide::NewAgent {
            project_id,
            fields: ide::AgentFields {
                name: name.to_string(),
                harness: parse_harness(harness)?,
                command: command.to_string(),
                model,
                env: env.to_string(),
            },
        },
    )?;
    assign_engines(core, &db);
    println!(
        "added agent #{} {} ({}, runs `{}`)",
        created.id,
        created.name,
        created.harness.as_str(),
        created.effective_command
    );
    Ok(())
}

/// Merges the given flags over what is stored, then hands the store a
/// complete `AgentFields`.
///
/// The merge is here rather than in the store on purpose: the store's update
/// is a true full replace, which is what an editing form wants, while a
/// command line wants "change this one thing". Both end at the same statement.
fn agent_edit(
    core: &AxiomataCore,
    id: i64,
    name: Option<String>,
    harness: Option<String>,
    command: Option<String>,
    model: Option<String>,
    env: Option<String>,
) -> Result<()> {
    let db = core.db_lock();
    let Some(existing) = ide::agent_store::get_agent(&db, id)? else {
        anyhow::bail!("no agent #{id}");
    };
    let fields = ide::AgentFields {
        name: name.unwrap_or(existing.name),
        harness: match harness {
            Some(raw) => parse_harness(&raw)?,
            None => existing.harness,
        },
        command: command.unwrap_or(existing.command),
        model: model.or(existing.model),
        env: env.unwrap_or(existing.env),
    };
    match ide::agent_store::update_agent(&db, id, fields)? {
        Some(agent) => {
            assign_engines(core, &db);
            println!(
                "updated agent #{} {} ({}, runs `{}`)",
                agent.id,
                agent.name,
                agent.harness.as_str(),
                agent.effective_command
            );
            Ok(())
        }
        None => anyhow::bail!("no agent #{id}"),
    }
}

async fn agent_prepare(core: &AxiomataCore, id: i64) -> Result<()> {
    let ready = axiomata_core::ide_start::start_agent(core, id).await?;
    println!("agent #{} {}", ready.agent.id, ready.agent.name);
    println!("  runs in: {}", ready.cwd.display());
    if ready.shared_folder {
        println!("  (the project is not a git repository — agents share its folder)");
    } else {
        println!(
            "  branch:  {}",
            ready.agent.branch.as_deref().unwrap_or("(none)")
        );
    }
    match ready.agent.port {
        Some(port) => println!("  port:    {port} (AXIOMATA_PORT)"),
        None => println!("  port:    none free in the range"),
    }
    if let Some(session) = &ready.agent.opencode_session {
        println!("  session: {session}");
    }
    println!("  command: {}", ready.launch_command);
    if !ready.status_connected {
        println!("  status:  no channel for this harness yet");
    }
    let mcp = &ready.mcp;
    match &mcp.note {
        Some(note) => println!("  team:    {:?} — {note}", mcp.status),
        None => println!(
            "  team:    {:?} — tools: {}",
            mcp.status,
            mcp.tools.join(", ")
        ),
    }
    Ok(())
}

async fn agent_status(core: &AxiomataCore, project_id: i64) -> Result<()> {
    let agents = ide::agent_store::list_agents(&core.db_lock(), project_id)?;
    let names: std::collections::HashMap<i64, String> = agents
        .iter()
        .map(|agent| (agent.id, agent.name.clone()))
        .collect();
    let mut statuses =
        ide::provision::agent_statuses(&agents, &axiomata_core::paths::ide_locations().channels);
    // Opencode agents report through the Opencode service, not the channel.
    axiomata_core::ide_status::overlay_once(&mut statuses, &agents).await;
    if statuses.is_empty() {
        println!("project #{project_id} has no agents");
    }
    for status in statuses {
        let name = names.get(&status.agent_id).map_or("?", String::as_str);
        let since = status
            .since
            .map(|t| {
                format!(
                    " since {}",
                    t.with_timezone(&chrono::Local).format("%H:%M:%S")
                )
            })
            .unwrap_or_default();
        println!(
            "#{} {name}: {}{since}",
            status.agent_id,
            status.state.as_str()
        );
        if let Some(document) = &status.plan_document {
            let earlier = if document.from_earlier_session {
                ", earlier session"
            } else {
                ""
            };
            println!("  plan ({}{earlier}):", document.name);
            for line in document.markdown.lines().take(12) {
                println!("    {line}");
            }
        }
        let Some(plan) = status.plan else { continue };
        if plan.from_earlier_session {
            println!("  (plan from an earlier session)");
        }
        for step in plan.steps {
            let mark = match step.state {
                ide::lifecycle::StepState::Done => "✓",
                ide::lifecycle::StepState::Doing => "▸",
                ide::lifecycle::StepState::Todo => "○",
                ide::lifecycle::StepState::Cancelled => "✗",
            };
            println!("  {mark} {}", step.text);
        }
    }
    Ok(())
}

/// The agent's repository; the database lock is released before git runs.
fn agent_repo_or_bail(core: &AxiomataCore, id: i64) -> Result<ide::git::AgentRepo> {
    let db = core.db_lock();
    Ok(ide::provision::agent_repo(&db, id)?.ready()?)
}

fn agent_diff(core: &AxiomataCore, id: i64, file: Option<String>) -> Result<()> {
    let repo = agent_repo_or_bail(core, id)?;
    let Some(path) = file else {
        let changes = repo.changes()?;
        let fallback = if changes.base.fallback {
            " (not recorded; what the project folder has checked out)"
        } else {
            ""
        };
        println!(
            "base: {} @ {}{fallback}",
            changes.base.branch,
            &changes.base.commit[..changes.base.commit.len().min(10)]
        );
        if changes.files.is_empty() {
            println!("no changes");
        }
        for file in changes.files {
            let counts = match (file.additions, file.deletions) {
                (Some(add), Some(del)) => format!("+{add} -{del}"),
                _ => "binary".into(),
            };
            let renamed = file
                .old_path
                .map(|old| format!(" (from {old})"))
                .unwrap_or_default();
            let open = if file.uncommitted {
                "  [uncommitted]"
            } else {
                ""
            };
            println!(
                "{:<12} {:>12}  {}{renamed}{open}",
                format!("{:?}", file.kind).to_lowercase(),
                counts,
                file.path
            );
        }
        return Ok(());
    };
    let diff = repo.file_diff(&path, None)?;
    if diff.binary {
        println!("{path}: binary file");
        return Ok(());
    }
    for hunk in diff.hunks {
        println!("{}", hunk.header);
        for line in hunk.lines {
            let mark = match line.kind {
                ide::git::LineKind::Add => "+",
                ide::git::LineKind::Remove => "-",
                ide::git::LineKind::Context => " ",
                ide::git::LineKind::NoNewline => "",
            };
            println!("{mark}{}", line.text);
        }
    }
    if diff.truncated {
        println!("… (cut off)");
    }
    Ok(())
}

fn agent_base(core: &AxiomataCore, id: i64, path: &str) -> Result<()> {
    use std::io::Write as _;
    let limit = ide::git::MAX_DIFF_BYTES as u64;
    match agent_repo_or_bail(core, id)?.base_blob(path, limit)? {
        ide::git::BaseBlob::Absent => bail!("{path} is not on the agent's base branch"),
        ide::git::BaseBlob::TooLarge { size } => {
            bail!("{path} is {size} bytes, over the {limit}-byte limit")
        }
        ide::git::BaseBlob::Bytes(bytes) => std::io::stdout().write_all(&bytes)?,
    }
    Ok(())
}

fn agent_take_over(core: &AxiomataCore, id: i64, message: &str, no_ff: bool) -> Result<()> {
    // Read under the lock, release it, then take over — which refuses while
    // the agent is working or waiting (G12).
    let target = {
        let db = core.db_lock();
        ide::provision::TakeOverTarget::read(&db, id)?
    };
    let mode = if no_ff {
        ide::git::TakeOverMode::NoFf
    } else {
        ide::git::TakeOverMode::Squash
    };
    let roots = axiomata_core::paths::ide_locations().channels;
    match target.run(&roots, mode, message)? {
        ide::git::TakeOver::Done {
            commit,
            committed: false,
        } => {
            // Nothing was committed, so the agent's branch was not moved either (G11 does not run): say what is
            // actually the case rather than the sentence that follows a real take-over.
            println!("nothing to take over: {commit} already holds the agent's work");
        }
        ide::git::TakeOver::Done {
            commit,
            committed: true,
        } => {
            println!("taken over as {commit}; the agent's branch now starts from there")
        }
        ide::git::TakeOver::Conflict { files } => {
            println!("conflict — undone, the project folder is as it was. Conflicting files:");
            for file in files {
                println!("  {file}");
            }
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn agent_discard_worktree(core: &AxiomataCore, id: i64, force: bool) -> Result<()> {
    let worktree = ide::agent_store::get_agent(&core.db_lock(), id)?
        .filter(|agent| agent.harness == ide::model::Harness::Opencode)
        .and_then(|agent| agent.worktree_path);
    let removed = {
        let db = core.db_lock();
        if !force && ide::provision::worktree_has_changes(&db, id)? {
            anyhow::bail!(
                "agent #{id} has uncommitted work in its worktree — pass --force to discard it"
            );
        }
        ide::provision::discard_worktree(&db, id, force)?
    };
    if removed {
        if let Some(worktree) = worktree {
            axiomata_core::agents::opencode::forget_mcp(&worktree).await;
        }
        println!("removed the worktree of agent #{id}");
    } else {
        println!("agent #{id} has no worktree");
    }
    Ok(())
}

async fn agent_delete(core: &AxiomataCore, id: i64) -> Result<()> {
    let agent = {
        let db = core.db_lock();
        let Some(agent) = ide::agent_store::get_agent(&db, id)? else {
            anyhow::bail!("no agent #{id}");
        };
        if !ide::agent_store::delete_agent(&db, id)? {
            anyhow::bail!("no agent #{id}");
        }
        agent
    };
    if agent.harness == ide::model::Harness::Opencode
        && let Some(worktree) = &agent.worktree_path
    {
        axiomata_core::agents::opencode::forget_mcp(worktree).await;
    }
    ide::provision::forget_channel(&axiomata_core::paths::ide_locations().channels, id)
        .context("the agent was removed, but its status folder was not")?;
    println!("removed agent #{} {}", id, agent.name);
    Ok(())
}

fn project_list(core: &AxiomataCore) -> Result<()> {
    let db = core.db_lock();
    let projects = ide::store::list_projects(&db)?;
    if projects.is_empty() {
        println!("no projects yet — add one with `ide projects new <name> <path>`");
        return Ok(());
    }
    for project in projects {
        let opened = match project.last_opened_at {
            Some(when) => when.format("%Y-%m-%d %H:%M").to_string(),
            None => "never".to_string(),
        };
        // A missing folder is the one thing worth shouting about here: it is
        // why the list checks at all, rather than finding out at open time.
        let missing = if project.root_exists { "" } else { "  MISSING" };
        let layout = if project.layout_json.is_some() {
            "layout"
        } else {
            "no layout"
        };
        println!(
            "#{:<4} {:<28} {}  (opened {opened}, {layout}){missing}",
            project.id,
            project.name,
            project.repo_root.display()
        );
    }
    Ok(())
}

fn project_new(core: &AxiomataCore, name: &str, path: &Path) -> Result<()> {
    let db = core.db_lock();
    let created = ide::store::create_project(
        &db,
        ide::NewProject {
            name: name.to_string(),
            repo_root: path.to_path_buf(),
        },
    )
    .with_context(|| format!("failed to add project {name:?}"))?;
    println!(
        "added project #{} {:?} at {}",
        created.id,
        created.name,
        created.repo_root.display()
    );
    Ok(())
}

fn project_rename(core: &AxiomataCore, id: i64, name: &str) -> Result<()> {
    let db = core.db_lock();
    match ide::store::rename_project(&db, id, name)? {
        Some(project) => {
            println!("renamed project #{} to {:?}", project.id, project.name);
            Ok(())
        }
        None => bail!("no project #{id}"),
    }
}

fn project_set_root(core: &AxiomataCore, id: i64, path: &Path) -> Result<()> {
    let db = core.db_lock();
    match ide::store::set_repo_root(&db, id, path)? {
        Some(project) => {
            println!(
                "project #{} now points at {}",
                project.id,
                project.repo_root.display()
            );
            Ok(())
        }
        None => bail!("no project #{id}"),
    }
}

/// Removes the row. The folder stays — see `ide::store::delete_project`.
fn project_delete(core: &AxiomataCore, id: i64) -> Result<()> {
    let db = core.db_lock();
    let Some(project) = ide::store::get_project(&db, id)? else {
        bail!("no project #{id}");
    };
    if !ide::store::delete_project(&db, id)? {
        bail!("no project #{id}");
    }
    println!(
        "removed project #{} {:?} from the list — {} is untouched",
        id,
        project.name,
        project.repo_root.display()
    );
    Ok(())
}

/// A card's one-line summary. The two signatures are the interesting part on
/// the command line — they are what a second actor needs to see before
/// deciding whether to touch the card at all.
pub(crate) fn card_line(card: &board::Card) -> String {
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
    marks.push_str(&board_flow::flow_marks(card));
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
        let role = column
            .stage
            .map_or(String::new(), |stage| format!(" · {}", stage.as_str()));
        println!(
            "\n  {} [#{} · {}{role}]  {} cards",
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
    board_mirror::after_change(&db, &read_config(core), created.id);
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

fn board_rename(core: &AxiomataCore, id: i64, name: &str) -> Result<()> {
    let db = core.db_lock();
    let Some(renamed) = board::store::rename_board(&db, id, name)? else {
        bail!("no board with id {id}");
    };
    board_mirror::after_change(&db, &read_config(core), id);
    println!("board #{id} is now {:?}", renamed.name);
    Ok(())
}

async fn board_delete(core: &AxiomataCore, id: i64, force: bool) -> Result<()> {
    // The cards' ids are read before the board goes — archived cards among them, because the delete takes those too.
    let (cards, card_ids) = {
        let mut db = core.db_lock();
        if board::store::get_board(&db, id)?.is_none() {
            bail!("no board with id {id}");
        }
        // Deleting a board takes its cards with it, so the count has to be said
        // out loud before it happens rather than reported afterwards.
        let cards = board::store::count_cards(&db, id)?;
        if cards > 0 && !force {
            bail!("board #{id} holds {cards} cards — pass --force to delete it with them");
        }
        let ids: Vec<i64> = board::store::list_cards(&db, id, true)?
            .into_iter()
            .map(|card| card.id)
            .collect();
        board::store::delete_board(&mut db, id)?;
        board_mirror::remove(&read_config(core), id);
        (cards, ids)
    };
    // The cards are gone: their sessions can be neither claimed nor restarted, so they end with them.
    let ended = axiomata_core::card_session::forget_sessions_of_cards(core, &card_ids).await;
    println!("board #{id} deleted ({cards} cards)");
    if !ended.is_empty() {
        println!("ended {} card session(s) that went with it", ended.len());
    }
    Ok(())
}

fn board_add(
    core: &AxiomataCore,
    column: i64,
    title: &str,
    body: Option<String>,
    labels: Vec<String>,
    flow: &board_flow::FlowFlags,
) -> Result<()> {
    let db = core.db_lock();
    // An agent proposes: its cards land in the board's proposal column and wait for the owner's yes (a2a.md A7, A17).
    if axiomata_core::session::session_actor().is_some() {
        let target = board::store::get_column(&db, column)?;
        if target.and_then(|c| c.stage) != Some(board::ColumnStage::Proposal) {
            bail!("an agent adds cards to the proposal column only; the owner approves them");
        }
    }
    let mut fields = board::CardFields {
        title: title.to_string(),
        body: body.unwrap_or_default(),
        labels,
        ..board::CardFields::default()
    };
    flow.apply(&mut fields)?;
    let card = board::store::create_card(
        &db,
        &board::NewCard {
            column_id: column,
            fields,
        },
    )
    .with_context(|| format!("failed to add a card to column #{column}"))?;
    board_mirror::after_change(&db, &read_config(core), card.board_id);
    println!("created card #{} {:?}", card.id, card.title);
    Ok(())
}

/// The fields a card has after `board edit`: what was passed replaces the old
/// value, everything else — assignee and due date included — stays as it was,
/// because the store's update is a full replace of the writable fields.
fn edited_fields(
    card: &board::Card,
    edit: CardEdit,
    flow: &board_flow::FlowFlags,
) -> Result<board::CardFields> {
    let mut fields = board::CardFields {
        title: edit.title.unwrap_or_else(|| card.title.clone()),
        body: edit.body.unwrap_or_else(|| card.body.clone()),
        labels: if edit.clear_labels {
            Vec::new()
        } else if edit.labels.is_empty() {
            card.labels.clone()
        } else {
            edit.labels
        },
        assignee: card.assignee.clone(),
        due_at: card.due_at,
        // The store's update is a full replace, so the agent fields have to be carried over or an edit of the title
        // would wipe them.
        plan_id: card.plan_id,
        agent: card.agent.clone(),
        agent_reason: card.agent_reason.clone(),
        tier: card.tier,
        kind: card.kind.clone(),
        acceptance: card.acceptance.clone(),
    };
    flow.apply(&mut fields)?;
    if edit.no_plan {
        fields.plan_id = None;
    }
    Ok(fields)
}

/// What `board edit` was asked to change about the card's own text; the agent fields travel as
/// [`board_flow::FlowFlags`].
struct CardEdit {
    title: Option<String>,
    body: Option<String>,
    labels: Vec<String>,
    clear_labels: bool,
    no_plan: bool,
}

fn board_edit(
    core: &AxiomataCore,
    id: i64,
    edit: CardEdit,
    flow: &board_flow::FlowFlags,
) -> Result<()> {
    let db = core.db_lock();
    let Some(card) = board::store::get_card(&db, id)? else {
        bail!("no card with id {id}");
    };
    let fields = edited_fields(&card, edit, flow)?;
    let updated = board::store::update_card(&db, id, &fields)
        .with_context(|| format!("failed to edit card #{id}"))?
        .with_context(|| format!("no card with id {id}"))?;
    board_mirror::after_change(&db, &read_config(core), updated.board_id);
    println!("edited {}", card_line(&updated));
    Ok(())
}

fn board_move(core: &AxiomataCore, id: i64, column: i64, index: usize, actor: &str) -> Result<()> {
    let mut db = core.db_lock();
    // `usize::MAX` is the "no --index given" sentinel; the store clamps an
    // out-of-range index to the end of the column on its own.
    if !board::store::move_card_as(&mut db, id, column, index, Some(actor))? {
        bail!("no card with id {id}");
    }
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!("card #{id} moved to column #{column}");
    Ok(())
}

async fn board_start(
    core: &AxiomataCore,
    id: i64,
    project: i64,
    engine: Option<String>,
) -> Result<()> {
    let session = axiomata_core::card_session::start_card_session(
        core,
        &axiomata_core::card_session::StartRequest {
            card_id: id,
            project_id: project,
            engine_id: engine,
        },
    )?;
    board_mirror::after_card_change(&core.db_lock(), &read_config(core), id);
    println!(
        "card #{id} taken for the new session #{} {} (role {}, engine {})",
        session.agent.id, session.agent.name, session.role, session.engine_id
    );
    // A session that cannot be started would hold the card for nothing: give it back.
    if let Err(err) = agent_prepare(core, session.agent.id).await {
        axiomata_core::card_session::release_card_session(core, id)?;
        axiomata_core::card_session::discard_session(core, session.agent.id).await?;
        board_mirror::after_card_change(&core.db_lock(), &read_config(core), id);
        return Err(err.context(format!(
            "the session did not start; card #{id} is waiting again"
        )));
    }
    Ok(())
}

async fn board_review(
    core: &AxiomataCore,
    id: i64,
    engine: Option<String>,
    allow_agent_config: bool,
) -> Result<()> {
    let session = axiomata_core::card_session::start_review_session(
        core,
        &axiomata_core::card_session::ReviewRequest {
            card_id: id,
            engine_id: engine,
            allow_agent_config,
        },
    )
    .await?;
    board_mirror::after_card_change(&core.db_lock(), &read_config(core), id);
    println!(
        "card #{id} is reviewed by the new session #{} {} (role {}, engine {})",
        session.agent.id, session.agent.name, session.role, session.engine_id
    );
    // A reviewer that cannot start is of no use: forget it, the card waits for the next try.
    if let Err(err) = agent_prepare(core, session.agent.id).await {
        axiomata_core::card_session::discard_session(core, session.agent.id).await?;
        return Err(err.context("the reviewer did not start"));
    }
    Ok(())
}

/// `board plan …`: the asynchronous steps around a plan — starting its planner and ending the planners of a plan that
/// was
/// approved, closed or deleted — and the rest as [`board_flow::plan_cmd`].
async fn board_plan(core: &AxiomataCore, action: board_flow::PlanAction) -> Result<()> {
    use board_flow::PlanAction;
    if let PlanAction::Start {
        id,
        project,
        engine,
        grill,
    } = action
    {
        board_flow::owner_only("starting a planner")?;
        let session = axiomata_core::plan_session::start_plan_session(
            core,
            &axiomata_core::plan_session::PlanStartRequest {
                plan_id: id,
                project_id: project,
                engine_id: engine,
                grill,
                role: None,
            },
        )
        .await?;
        println!(
            "plan #{id} has the new planner #{} {} (role {}, engine {})",
            session.agent.id, session.agent.name, session.role, session.engine_id
        );
        if let Err(err) = agent_prepare(core, session.agent.id).await {
            axiomata_core::plan_session::forget_plan_sessions(core, id).await;
            return Err(err.context(format!(
                "the planner did not start; plan #{id} has no planner again"
            )));
        }
        return Ok(());
    }
    let ended = match &action {
        PlanAction::Approve { id, .. } | PlanAction::Close { id } | PlanAction::Delete { id } => {
            Some(*id)
        }
        _ => None,
    };
    board_flow::plan_cmd(core, action)?;
    if let Some(id) = ended {
        let count = axiomata_core::plan_session::forget_plan_sessions(core, id).await;
        if count > 0 {
            println!("{count} planner session(s) of plan #{id} ended");
        }
    }
    Ok(())
}

async fn board_take_over_plan(core: &AxiomataCore, id: i64) -> Result<()> {
    use axiomata_core::card_session::PlanTakeOver;
    let outcome = axiomata_core::card_session::take_over_plan(core, id).await?;
    match outcome {
        PlanTakeOver::Done {
            commit,
            card_ids,
            cleanup,
            ..
        } => {
            {
                let db = core.db_lock();
                let config = read_config(core);
                for card in &card_ids {
                    board_mirror::after_card_change(&db, &config, *card);
                }
            }
            println!(
                "plan #{id} taken over: {} card(s), the branch is now at {commit}",
                card_ids.len()
            );
            for note in cleanup {
                println!("  not done: {note}");
            }
            println!("not pushed: push from the Studio's Git tab");
        }
        PlanTakeOver::Conflict { files } => {
            println!(
                "plan #{id} does not fit the branch any more: {}; nothing was changed",
                files.join(", ")
            );
        }
    }
    Ok(())
}

async fn board_integrate(core: &AxiomataCore, id: i64) -> Result<()> {
    use axiomata_core::card_session::CardIntegration;
    let outcome = axiomata_core::card_session::integrate_card_with(core, id, true).await?;
    board_mirror::after_card_change(&core.db_lock(), &read_config(core), id);
    match outcome {
        CardIntegration::Done {
            plan_id, commit, ..
        } => match commit {
            Some(commit) => println!("card #{id} is on the line of plan #{plan_id} as {commit}"),
            None => println!("card #{id} changed nothing compared to the line of plan #{plan_id}"),
        },
        CardIntegration::Conflict { files, gave_up, .. } => {
            println!(
                "card #{id} does not fit the line any more: {}",
                files.join(", ")
            );
            if gave_up {
                println!("it was not put back again; its session stays for you to look at");
            } else {
                println!("it was put back to be done again on the line as it is");
            }
        }
        CardIntegration::Busy { .. } => {
            println!("its worker is in the middle of a turn; try again in a moment")
        }
        // Only the studio's own looks leave a card alone; this is the owner's request.
        CardIntegration::LeftForOwner { .. } => {
            println!("the studio gave up on card #{id}; nothing was done")
        }
    }
    Ok(())
}

async fn board_usage(core: &AxiomataCore, id: i64) -> Result<()> {
    if board::store::get_card(&core.db_lock(), id)?.is_none() {
        bail!("no card with id {id}");
    }
    let sessions = axiomata_core::session_limits::card_usage(core, id).await?;
    if sessions.is_empty() {
        println!("no session was started for card #{id}");
    }
    for session in sessions {
        let kind = if session.review { "reviewer" } else { "worker" };
        println!("{} ({kind}, role {})", session.name, session.role);
        if !session.measured {
            println!(
                "  usage unknown: the harness's record could not be read, so no limit can stop it"
            );
        }
        println!(
            "  steps  {} of {}",
            session.usage.steps, session.limits.max_steps
        );
        println!(
            "  tokens {} of {}",
            session.usage.tokens(),
            session.limits.max_tokens
        );
        match session.cost_usd {
            Some(cost) => println!("  money  ${cost:.2} of ${:.2}", session.limits.max_cost_usd),
            None => println!("  money  not metered (subscription, or no price for its model)"),
        }
        if let Some(reason) = session.stopped {
            println!("  STOPPED: {reason}");
        }
    }
    Ok(())
}

async fn board_take_over(core: &AxiomataCore, id: i64, message: Option<String>) -> Result<()> {
    use axiomata_core::card_session::CardTakeOver;
    let outcome = axiomata_core::card_session::take_over_card(core, id, message.as_deref()).await?;
    board_mirror::after_card_change(&core.db_lock(), &read_config(core), id);
    match outcome {
        CardTakeOver::Done {
            commit,
            project_id,
            already_on_base,
            cleanup,
        } => {
            if already_on_base {
                println!(
                    "card #{id} closed: its work was already on the main line ({commit}), so nothing was committed — \
                     its sessions are cleaned up"
                );
            } else {
                println!("card #{id} taken over as {commit}; its sessions are cleaned up");
            }
            for note in cleanup {
                println!("  not removed: {note}");
            }
            // The studio never pushes; say what is waiting, so it is not forgotten.
            let root = ide::store::get_project(&core.db_lock(), project_id)?
                .map(|project| project.repo_root);
            if let Some(status) = root.and_then(|root| axiomata_git::repo::status(&root).ok()) {
                match (status.ahead, status.upstream.as_deref()) {
                    (0, Some(_)) => {}
                    (0, None) => println!(
                        "not pushed yet: the branch has no upstream — push it from the Studio's Git tab"
                    ),
                    (ahead, _) => println!(
                        "not pushed yet: {ahead} commit(s) ahead of {} — push from the Studio's Git tab",
                        status.upstream.as_deref().unwrap_or("the upstream")
                    ),
                }
            }
            Ok(())
        }
        CardTakeOver::Conflict { files } => {
            println!("conflict — undone, the project folder is as it was. Conflicting files:");
            for file in files {
                println!("  {file}");
            }
            std::process::exit(1);
        }
    }
}

fn board_release(core: &AxiomataCore, id: i64) -> Result<()> {
    let board_id = board::store::get_card(&core.db_lock(), id)?
        .map(|card| card.board_id)
        .with_context(|| format!("no card with id {id}"))?;
    if !axiomata_core::card_session::release_card_session(core, id)? {
        bail!("card #{id} is not held by anybody");
    }
    board_mirror::after_change(&core.db_lock(), &read_config(core), board_id);
    println!("card #{id} is waiting in its open column again");
    Ok(())
}

fn board_claim(core: &AxiomataCore, id: i64, actor: &str) -> Result<()> {
    let db = core.db_lock();
    if board::store::claim_card(&db, id, actor)? {
        board_mirror::after_card_change(&db, &read_config(core), id);
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
            board_mirror::after_change(&db, &read_config(core), column.board_id);
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
        board_mirror::after_card_change(&db, &read_config(core), id);
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
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!(
        "card #{id} {}",
        if archived { "archived" } else { "restored" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn an_agent_session_may_read_the_ide_but_not_start_or_change_anything() {
        let agents = |action| IdeAction::Agents { action };
        assert!(ide_owner_gate(&agents(AgentAction::List { project: 1 })).is_none());
        assert!(ide_owner_gate(&agents(AgentAction::Status { project: 1 })).is_none());
        // Starting a session issues its secret; deleting or re-opening one changes what runs.
        for gated in [
            AgentAction::Prepare { id: 2 },
            AgentAction::Delete { id: 2 },
            AgentAction::NewSession { id: 2 },
        ] {
            assert!(ide_owner_gate(&agents(gated)).is_some());
        }
        assert!(
            ide_owner_gate(&IdeAction::Projects {
                action: ProjectAction::List
            })
            .is_none()
        );
        assert!(
            ide_owner_gate(&IdeAction::Projects {
                action: ProjectAction::Delete { id: 1 }
            })
            .is_some()
        );
        assert!(
            ide_owner_gate(&IdeAction::Engines {
                action: EngineAction::List
            })
            .is_none()
        );
        assert!(
            ide_owner_gate(&IdeAction::Engines {
                action: EngineAction::Delete { id: "x".into() }
            })
            .is_some()
        );
        assert!(
            ide_owner_gate(&IdeAction::Roles {
                action: RoleAction::List
            })
            .is_none()
        );
        assert!(
            ide_owner_gate(&IdeAction::Roles {
                action: RoleAction::Delete { name: "x".into() }
            })
            .is_some()
        );
    }

    fn card() -> board::Card {
        let now = Utc::now();
        board::Card {
            id: 7,
            board_id: 1,
            column_id: 8,
            position: 0.0,
            title: "Old title".to_string(),
            body: "Old body".to_string(),
            labels: vec!["bug".to_string(), "mail".to_string()],
            assignee: Some("human:owner".to_string()),
            claimed_by: None,
            claimed_at: None,
            verified_by: None,
            verified_at: None,
            due_at: Some(now),
            archived_at: None,
            created_at: now,
            updated_at: now,
            plan_id: Some(3),
            agent: Some("implementer-light".to_string()),
            agent_reason: Some("klein".to_string()),
            tier: Some(board::Tier::Light),
            kind: Some("implement".to_string()),
            acceptance: "- Tests grün".to_string(),
            returned_count: 0,
            input_required: None,
            integrated_at: None,
            taken_over_at: None,
            failed_at: None,
            canceled_at: None,
            depends_on: Vec::new(),
            waiting_on: Vec::new(),
            state: board::TaskState::Ready,
        }
    }

    fn edit(
        card: &board::Card,
        title: Option<String>,
        labels: Vec<String>,
        clear: bool,
    ) -> board::CardFields {
        let change = CardEdit {
            title,
            labels,
            clear_labels: clear,
            ..untouched()
        };
        edited_fields(card, change, &board_flow::FlowFlags::default()).unwrap()
    }

    fn untouched() -> CardEdit {
        CardEdit {
            title: None,
            body: None,
            labels: Vec::new(),
            clear_labels: false,
            no_plan: false,
        }
    }

    #[test]
    fn an_edit_keeps_the_agent_fields_of_the_card() {
        let card = card();
        let fields = edit(&card, Some("Neu".to_string()), Vec::new(), false);
        assert_eq!(fields.plan_id, Some(3));
        assert_eq!(fields.agent.as_deref(), Some("implementer-light"));
        assert_eq!(fields.agent_reason.as_deref(), Some("klein"));
        assert_eq!(fields.tier, Some(board::Tier::Light));
        assert_eq!(fields.kind.as_deref(), Some("implement"));
        assert_eq!(fields.acceptance, "- Tests grün");
    }

    #[test]
    fn flow_flags_replace_only_what_was_passed_and_no_plan_removes_the_plan() {
        let card = card();
        let flags = board_flow::FlowFlags {
            tier: Some("heavy".to_string()),
            acceptance: Some("neu".to_string()),
            ..board_flow::FlowFlags::default()
        };
        let fields = edited_fields(&card, untouched(), &flags).unwrap();
        assert_eq!(
            (fields.tier, fields.acceptance.as_str()),
            (Some(board::Tier::Heavy), "neu")
        );
        assert_eq!(fields.kind.as_deref(), Some("implement"));

        let none = edited_fields(
            &card,
            CardEdit {
                no_plan: true,
                ..untouched()
            },
            &board_flow::FlowFlags::default(),
        )
        .unwrap();
        assert_eq!(none.plan_id, None);

        let bad = board_flow::FlowFlags {
            tier: Some("mighty".to_string()),
            ..board_flow::FlowFlags::default()
        };
        assert!(edited_fields(&card, untouched(), &bad).is_err());
    }

    #[test]
    fn an_edit_without_flags_changes_nothing() {
        let card = card();
        let fields = edit(&card, None, Vec::new(), false);
        assert_eq!(fields.title, card.title);
        assert_eq!(fields.body, card.body);
        assert_eq!(fields.labels, card.labels);
        assert_eq!(fields.assignee, card.assignee);
        assert_eq!(fields.due_at, card.due_at);
    }

    #[test]
    fn labels_are_replaced_as_a_whole_and_can_be_cleared() {
        let card = card();
        let replaced = edit(&card, None, vec!["error".to_string()], false);
        assert_eq!(replaced.labels, ["error"]);
        let cleared = edit(&card, None, Vec::new(), true);
        assert!(cleared.labels.is_empty());
    }

    #[test]
    fn a_new_title_keeps_body_labels_assignee_and_due_date() {
        let card = card();
        let fields = edit(&card, Some("New".to_string()), Vec::new(), false);
        assert_eq!(fields.title, "New");
        assert_eq!(fields.body, card.body);
        assert_eq!(fields.labels, card.labels);
        assert_eq!(fields.due_at, card.due_at);
    }
}
