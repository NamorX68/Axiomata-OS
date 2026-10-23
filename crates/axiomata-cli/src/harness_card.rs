//! `axiomata-cli harness card`: a Studio session on Glut, in its pane (`docs/plans/glut-cards.md`), on Glut's own
//! terminal interface (Glut's `docs/plans/glut-tui.md` TUI8b).
//!
//! The studio writes everything the session is started with into its channel (`axiomata_core::glut_card`); this reads
//! it back by the session's id and runs the TUI in the session's folder with the studio as its host ([`CardHost`]):
//! the spec's rights, the team server alone, the session's limits before each turn and the studio's stop during one,
//! a question of the gate in the pane *and* in the app at once (the first answer counts, C5), every step into the
//! channel's event log for the meter and the Flow tiles (C6), the conversation into the channel after each turn, and a
//! turn the pane lost going on from its journal (C8). The app's theme follows into the pane (TUI8e).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::agent_entry::HARNESS_RUN_ENV;
use axiomata_core::glut_card::{self, CardSpec};
use axiomata_core::ide::lifecycle::{AgentState, Channel};
use axiomata_core::permission_ask::{self, Decision};
use glut::journal;
use glut::permissions::{
    Answer, AnswerFuture, AskRequest, BuiltGate, GateOptions, ask_display, printable,
};
use glut::providers::catalog::Protocol;
use glut::{Budget, Message, Prices, RunConfig};
use glut_tui::{
    EngineChoice, Host, HostFuture, HostLink, Instructions, Launch, Open, Search, Servers, Signal,
    Turn,
};

/// A card's work takes far more steps than a skill's; the session's own limits (steps, tokens, money, CP-A6c) are what
/// stops it, so the run's step limit is only the last fence.
const CARD_MAX_TURNS: u32 = 400;
/// How long one turn of the session may run.
const CARD_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);
/// The project's guide for coding agents, given to the run as part of its instructions — Claude Code and Opencode read
/// it from the folder themselves, Glut is handed it.
const PROJECT_GUIDE: &str = "AGENTS.md";
/// The most of the project's guide that is passed on: it is part of every model call.
const MAX_GUIDE_BYTES: u64 = 64 * 1024;
/// What the pane's header shows as the provider: the app's own route to the model.
const PROVIDER_SHOWN: &str = "axiomata";
/// How often a failing reading of the session's usage is tried before the turn is refused.
const BUDGET_TRIES: u32 = 3;
/// How often the pane looks for the studio's stop.
const STOP_POLL: Duration = Duration::from_secs(2);
/// How often the pane looks whether the app answered a question.
const APP_POLL: Duration = Duration::from_millis(500);
/// How often the pane looks whether the app's theme changed.
const THEME_POLL: Duration = Duration::from_secs(2);
/// The most of the app's `dashboard.json` read for its theme.
const MAX_DASHBOARD_BYTES: u64 = 4 * 1024 * 1024;

/// Runs the session the environment names.
pub async fn card(core: Arc<AxiomataCore>) -> Result<()> {
    let agent_id: i64 = std::env::var("AXIOMATA_AGENT_ID")
        .ok()
        .and_then(|id| id.trim().parse().ok())
        .context("`harness card` runs in a Studio session's pane, which names the session (AXIOMATA_AGENT_ID)")?;
    // A run of the app's harnesses does not start a session of its own: it would spend the owner's money in a session's
    // name nobody started.
    if std::env::var_os(HARNESS_RUN_ENV).is_some() {
        bail!("a skill or chat run does not start a Studio session");
    }
    let roots = axiomata_core::paths::ide_locations().channels;
    let spec = glut_card::read(&roots, agent_id)?;
    let channel = Channel::for_agent(&roots, agent_id);
    // One pane per session: a second `harness card` in the same name would run the session twice.
    let _held = hold_session(&channel)?;
    check_spec(&core, &spec, &channel)?;
    let config = crate::read_config(&core);
    let endpoint = glut_card::endpoint(&config, spec.model.clone())?;
    let model_name = endpoint.model.clone();

    let journals = channel.dir().join(glut_card::JOURNAL_DIR);
    journal::sweep(&journals);
    // A turn the pane lost (the app quit, the pane closed) goes on from its journal (C8): a session nobody watches
    // must not stand still on a question, and the owner can stop it with Esc. Only the newest one; any older is given
    // up.
    let mut cut_off = journal::list(&journals)
        .into_iter()
        .filter(|run| !run.running);
    let lost = cut_off.next().map(|run| run.run);
    for older in cut_off {
        let _ = journal::discard(&journals, &older.run);
    }
    // What is said before the screen is up would be wiped by it: said in the pane's stream instead.
    let mut early = Vec::new();
    let conversation = load_conversation(
        &channel.dir().join(glut_card::CONVERSATION_FILE),
        &mut early,
    );

    let launch = Launch {
        cwd: spec.cwd.clone(),
        gate: GateOptions {
            mode: Some(spec.mode()?),
            allow: spec.allow.clone(),
            ask: Vec::new(),
            deny: spec.deny.clone(),
            // The project's own `.axh/` rules never widen a session the studio started.
            trusted: false,
        },
        owner_trust: false,
        instructions: Instructions::Given(instructions(&spec)),
        // The team server alone: the session's own file, never the owner's `~/.axh/mcp.json` nor the project's.
        servers: Servers::Only(spec.mcp_config.clone()),
        // The owner's skills, as a chat turn has them; a project's own only count in a trusted folder, which this is not.
        skill_dirs: Some(vec![axiomata_core::paths::global_skills_dir()]),
        search: match axiomata_core::web_search::web_search_tool(&config) {
            Some(search) => Search::Given(search),
            None => Search::Off,
        },
        engine: EngineChoice::Fixed {
            provider: PROVIDER_SHOWN.to_owned(),
            endpoint,
            protocol: Protocol::ChatCompletions,
            prices: app_prices(&config, &model_name),
            window: None,
        },
        // The studio keeps the conversation in the channel; `glut -r` in the worktree would find it a second time.
        keep_sessions: false,
        // The rights were chosen for the session's role (`agent_entry::glut_card_rights`): Shift+Tab must not widen them.
        mode_locked: true,
        // A `!cmd`'s output goes to the model, on the app's key, from a folder that can read `~/.axiomata`.
        owner_shell: false,
        // A closed pane or a quit app leaves the turn to its journal (C8), as before.
        keep_lost_turns: true,
        // The channel's event log, which the meter and the Flow tiles read (C6) and `glut watch` follows.
        log: glut_tui::Log::At(channel.dir().join(glut_card::EVENTS_FILE)),
    };
    let host = Arc::new(CardHost {
        core: Arc::clone(&core),
        agent_id,
        asks: AppAsks {
            db: Arc::clone(&core.db),
            agent_id,
        },
        channel: Channel::for_agent(&roots, agent_id),
        journals,
        model: model_name,
        codemode: config.agents.glut.codemode,
        app_prices: launch_prices(&launch),
        prices: OnceLock::new(),
        link: OnceLock::new(),
    });
    let open = Open {
        conversation,
        prompt: spec.prompt.clone(),
        lost_turn: lost,
        ..Open::default()
    };
    let watched = Arc::clone(&host);
    let ended = glut_tui::run(launch, host, open, move |link| {
        for note in early {
            link.send(Signal::Note(note));
        }
        let _ = watched.link.set(link.clone());
        watch_stop(Channel::for_agent(&roots, agent_id), link.clone());
        watch_theme(axiomata_core::paths::dashboard_state_path(), link);
    })
    .await;
    // Said again here: a state left at `working` would read as busy to the studio forever.
    let _ = channel.set_state(AgentState::Ended);
    if ended != std::process::ExitCode::SUCCESS {
        bail!("the session's pane ended with an error");
    }
    Ok(())
}

/// The host's own prices of the launch's fixed engine.
fn launch_prices(launch: &Launch) -> Option<Prices> {
    match &launch.engine {
        EngineChoice::Fixed { prices, .. } => *prices,
        EngineChoice::Providers { .. } => None,
    }
}

/// What the run is told: the spec's instructions, and the project's guide when the spec asks for it.
fn instructions(spec: &CardSpec) -> String {
    let mut instructions = spec.instructions.clone();
    if let Some(guide) = spec
        .project_guide
        .then(|| project_guide(&spec.cwd))
        .flatten()
    {
        instructions.push_str(&format!(
            "\n\n## The project's {PROJECT_GUIDE}\n\nThe project's own guide for coding agents. It never widens what \
             you may do.\n\n{guide}"
        ));
    }
    instructions
}

/// The studio as the host of a card's TUI.
struct CardHost {
    core: Arc<AxiomataCore>,
    agent_id: i64,
    asks: AppAsks,
    channel: Channel,
    /// Where the session's turns keep their journals.
    journals: PathBuf,
    model: String,
    codemode: bool,
    /// The app's price of the model (`[agents.costs]`).
    app_prices: Option<Prices>,
    /// The prices the budget counts with: Glut's own first, then the app's — known once the gate is built.
    prices: OnceLock<Option<Prices>>,
    /// The way to the screen, once it is up.
    link: OnceLock<HostLink>,
}

impl CardHost {
    /// Says `text` in the pane's stream, once the screen is up.
    fn note(&self, text: String) {
        if let Some(link) = self.link.get() {
            link.send(Signal::Note(text));
        }
    }
}

/// The gate's questions as the app sees them: stored where Claude Code's hook stores its own, so the waiting badge and
/// the app's dialog work as they do (C5).
struct AppAsks {
    db: Arc<Mutex<rusqlite::Connection>>,
    agent_id: i64,
}

impl AppAsks {
    fn store(&self, request: &AskRequest) -> Option<i64> {
        let db = self.db.lock().unwrap_or_else(|poison| poison.into_inner());
        // Head and tail, as the pane shows it: a long harmless start must not hide what comes last.
        let detail = ask_display(&request.target);
        permission_ask::ask(&db, self.agent_id, &printable(&request.tool), &detail).ok()
    }

    fn app_decision(&self, id: i64) -> Option<Decision> {
        let db = self.db.lock().unwrap_or_else(|poison| poison.into_inner());
        permission_ask::decision_of(&db, id).ok().flatten()
    }

    /// Settles the stored question with the pane's answer; `Some` with the app's when that one came first.
    fn settle(&self, id: i64, typed: Decision) -> Option<Decision> {
        let db = self.db.lock().unwrap_or_else(|poison| poison.into_inner());
        match permission_ask::answer_in_pane(&db, self.agent_id, id, typed) {
            Ok(true) | Err(_) => None,
            Ok(false) => permission_ask::decision_of(&db, id).ok().flatten(),
        }
    }

    /// Asks in the app while the pane's dialog `on_screen` asks too; whichever answer comes first settles it. Gives
    /// the answer, and what the pane should say of an answer the app gave.
    async fn race(
        &self,
        request: &AskRequest,
        on_screen: AnswerFuture<'_>,
    ) -> (Answer, Option<String>) {
        let Some(id) = self.store(request) else {
            return (on_screen.await, None);
        };
        // Whatever ends this wait — an answer, the turn called off — the app stops offering it.
        let _withdrawn = Withdraw {
            db: Arc::clone(&self.db),
            agent_id: self.agent_id,
            id,
        };
        let mut on_screen = on_screen;
        let mut app_open = true;
        loop {
            tokio::select! {
                typed = &mut on_screen => {
                    let decision = if typed == Answer::Deny { Decision::Deny } else { Decision::Allow };
                    return match self.settle(id, decision).and_then(from_app) {
                        Some(first) => (first, Some(format!("answered in the app first: {}", describe(first)))),
                        None => (typed, None),
                    };
                }
                () = tokio::time::sleep(APP_POLL), if app_open => match self.app_decision(id) {
                    None => {}
                    Some(decision) => match from_app(decision) {
                        Some(answer) => return (answer, Some(format!("answered in the app: {}", describe(answer)))),
                        // "Answer it in the pane": the pane goes on waiting alone.
                        None => app_open = false,
                    },
                },
            }
        }
    }
}

impl Host for CardHost {
    fn configure(&self, config: &mut RunConfig, built: &BuiltGate) {
        config.max_turns = CARD_MAX_TURNS;
        config.timeout = CARD_TIMEOUT;
        config.codemode = self.codemode;
        // The team tools are the session's work, not a lookup: offered directly.
        config.mcp_behind_code = false;
        // Glut's own prices first, then the app's table (feedback "Glut standalone").
        let _ = self
            .prices
            .set(built.prices(&self.model).or(self.app_prices));
    }

    fn before_turn<'a>(
        &'a self,
        config: &'a mut RunConfig,
        turn: Turn,
    ) -> HostFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Some(reason) = self.channel.limit_reached() {
                return Err(format!(
                    "this session is stopped: {} — raise its limit, then send again",
                    printable(&reason)
                ));
            }
            config.budget = match turn {
                // What is left of the session's limits, so a turn cannot spend them twice (C6).
                Turn::Message => {
                    budget(
                        &self.core,
                        self.agent_id,
                        self.prices.get().copied().flatten(),
                    )
                    .await?
                }
                // The journal counts on from what the lost turn had used, which the session's reading counts already:
                // the budget would take it twice. The studio's stop still bounds the turn.
                Turn::Resumed => Budget::default(),
            };
            // `resume` reads the folder of the journal from here; the run goes on in a journal of its own.
            config.journal = Some(journal_spec(&self.journals, self.agent_id));
            Ok(())
        })
    }

    fn may_spend(&self) -> Result<(), String> {
        match self.channel.limit_reached() {
            Some(reason) => Err(format!("this session is stopped: {}", printable(&reason))),
            None => Ok(()),
        }
    }

    fn not_resumed(&self, run: &str, _reason: &str) {
        let _ = journal::discard(&self.journals, run);
    }

    fn ask<'a>(&'a self, request: &'a AskRequest, on_screen: AnswerFuture<'a>) -> AnswerFuture<'a> {
        Box::pin(async move {
            let (answer, said) = self.asks.race(request, on_screen).await;
            if let Some(said) = said {
                self.note(said);
            }
            answer
        })
    }

    fn state(&self, state: glut_tui::State) {
        let state = match state {
            glut_tui::State::Working => AgentState::Working,
            glut_tui::State::Waiting => AgentState::Waiting,
            glut_tui::State::Idle => AgentState::Idle,
            glut_tui::State::Ended => AgentState::Ended,
        };
        let _ = self.channel.set_state(state);
    }

    fn kept(&self, conversation: &[Message]) {
        if let Err(err) = save_conversation(&self.channel, conversation) {
            self.note(format!("the conversation could not be kept: {err}"));
        }
    }
}

/// The model's price in the app's table (`[agents.costs]`), in Glut's shape.
fn app_prices(config: &axiomata_core::config::Config, model: &str) -> Option<Prices> {
    let cost = config.agents.costs.get(model)?;
    Some(Prices {
        input_per_m: cost.input_per_m,
        cached_input_per_m: cost.cached_input_per_m,
        output_per_m: cost.output_per_m,
    })
}

/// The next turn's budget: what is left of the session's limits, when the studio watches the session; none for a session
/// the owner started for nothing in particular.
///
/// # Errors
///
/// What the session used cannot be read: the turn is refused rather than given everything.
async fn budget(
    core: &AxiomataCore,
    agent_id: i64,
    prices: Option<Prices>,
) -> Result<Budget, String> {
    // A reading that fails is tried again.
    let mut usage = None;
    for _ in 0..BUDGET_TRIES {
        match axiomata_core::session_limits::session_usage(core, agent_id).await {
            Ok(None) => return Ok(Budget::default()),
            Ok(Some(found)) => {
                usage = Some(found);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
    let usage = usage.ok_or("what this session used cannot be read; the turn is not started")?;
    // The meter's own figure, else the session's tokens at the run's prices: a session that spent nothing yet, or whose
    // engine names no model, has no figure of the meter's.
    let spent = usage.cost_usd.or_else(|| {
        prices.map(|prices| {
            prices.cost_usd(&glut::Usage {
                input_tokens: usage.usage.input_tokens,
                cached_input_tokens: 0,
                output_tokens: usage.usage.output_tokens,
            })
        })
    });
    let (tokens, money) = axiomata_core::session_limits::left_of(&usage, spent);
    Ok(Budget {
        max_tokens: Some(tokens),
        max_usd: money,
        prices,
    })
}

/// The studio's stop (a session at its limit, `session_limits`): the turn under way ends at once, as Claude Code's hook
/// refuses its next call; the next turn is refused in `before_turn` until the owner raised the limit.
fn watch_stop(channel: Channel, link: HostLink) {
    tokio::spawn(async move {
        // Once per time the limit is reached: the turn ends at the first stop, a second would only repeat it.
        let mut stopped = false;
        loop {
            tokio::time::sleep(STOP_POLL).await;
            match channel.limit_reached() {
                Some(reason) if !stopped => {
                    if !link.send(Signal::Stop(printable(&reason))) {
                        return;
                    }
                    stopped = true;
                }
                Some(_) => {}
                None => stopped = false,
            }
        }
    });
}

/// The app's theme into the pane, at the start and whenever the owner picks another (TUI8e).
fn watch_theme(dashboard: PathBuf, link: HostLink) {
    tokio::spawn(async move {
        let mut seen: Option<SystemTime> = None;
        let mut last: Option<String> = None;
        loop {
            let changed = std::fs::metadata(&dashboard)
                .and_then(|meta| meta.modified())
                .ok();
            if changed.is_some() && changed != seen {
                seen = changed;
                if let Some(theme) = app_theme(&dashboard)
                    && last.as_ref() != Some(&theme)
                {
                    if !link.send(Signal::Theme(theme.clone())) {
                        return;
                    }
                    last = Some(theme);
                }
            }
            tokio::time::sleep(THEME_POLL).await;
        }
    });
}

/// The theme the app's `dashboard.json` names (`settings.theme`); `None` when there is none to read.
fn app_theme(dashboard: &Path) -> Option<String> {
    use std::io::Read as _;
    let file = std::fs::File::open(dashboard).ok()?;
    let mut text = String::new();
    file.take(MAX_DASHBOARD_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let theme = value.get("settings")?.get("theme")?.as_str()?;
    (!theme.is_empty()).then(|| theme.to_owned())
}

/// The session's lock, held while the pane runs: one pane per session (a card's, a master's).
pub(crate) fn hold_session(channel: &Channel) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = channel.dir().join(glut_card::LOCK_FILE);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    if file.try_lock().is_err() {
        bail!("this session already runs in another pane");
    }
    Ok(file)
}

/// The spec as the studio wrote it, checked against what the database says of the session: its folder is the session's
/// own, and the team server's file is the one beside the spec — the spec is a file, and a file can be changed.
fn check_spec(core: &AxiomataCore, spec: &CardSpec, channel: &Channel) -> Result<()> {
    let agent = {
        let db = core.db.lock().unwrap_or_else(|poison| poison.into_inner());
        axiomata_core::ide::agent_store::get_agent(&db, spec.agent_id)?
    }
    .context("no such session")?;
    if agent.harness != axiomata_core::ide::model::Harness::Glut {
        bail!("this session is not one of Glut's");
    }
    // A master is the owner's own Glut (`harness master`): a spec left in its channel is no card of its.
    if agent.is_master() {
        bail!("this session is a master, which runs as `harness master`");
    }
    let own = match &agent.worktree_path {
        Some(worktree) => worktree.clone(),
        None => {
            let db = core.db.lock().unwrap_or_else(|poison| poison.into_inner());
            axiomata_core::ide::store::get_project(&db, agent.project_id)?
                .context("the session's project is gone")?
                .repo_root
        }
    };
    if spec.cwd != own {
        bail!("the session's spec names another folder than its own");
    }
    if spec.mcp_config != channel.dir().join("mcp.json") {
        bail!("the session's spec names another team server than its own");
    }
    Ok(())
}

/// A new turn's journal: named by the time it starts, so two turns never share one.
fn journal_spec(dir: &Path, agent_id: i64) -> journal::JournalSpec {
    journal::JournalSpec {
        dir: dir.to_path_buf(),
        run: format!("turn-{}", chrono::Utc::now().timestamp_millis()),
        meta: serde_json::json!({ "agent": agent_id }),
    }
}

/// The project's `AGENTS.md`, read through the guarded walk (no link followed out of the folder), when it has one.
fn project_guide(cwd: &Path) -> Option<String> {
    let root = axiomata_files::Root::dir(cwd, axiomata_files::LinkPolicy::Strict).ok()?;
    let file = axiomata_files::read_text(&root, PROJECT_GUIDE, MAX_GUIDE_BYTES).ok()?;
    let text = file.content.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The conversation the channel kept; why not into `notes` when it cannot be read.
fn load_conversation(path: &Path, notes: &mut Vec<String>) -> Vec<Message> {
    if !path.exists() {
        return Vec::new();
    }
    match glut_card::read_private(path, glut_card::MAX_CONVERSATION_BYTES)
        .map_err(anyhow::Error::from)
        .and_then(|text| serde_json::from_str(&text).map_err(anyhow::Error::from))
    {
        Ok(messages) => messages,
        Err(err) => {
            notes.push(format!(
                "the earlier conversation is not read back ({err}); starting afresh"
            ));
            Vec::new()
        }
    }
}

fn save_conversation(channel: &Channel, conversation: &[Message]) -> Result<()> {
    let text = serde_json::to_string(conversation)?;
    channel.write_private(glut_card::CONVERSATION_FILE, &text)?;
    Ok(())
}

fn from_app(decision: Decision) -> Option<Answer> {
    match decision {
        Decision::Allow => Some(Answer::Allow),
        Decision::Deny => Some(Answer::Deny),
        // "Answer it in the pane": the pane goes on waiting alone.
        Decision::Terminal => None,
    }
}

/// Withdraws a stored question nobody answered when the wait for it ends (`permission_ask::withdraw`); one that was
/// answered stays as it was.
struct Withdraw {
    db: Arc<Mutex<rusqlite::Connection>>,
    agent_id: i64,
    id: i64,
}

impl Drop for Withdraw {
    fn drop(&mut self) {
        let db = self.db.lock().unwrap_or_else(|poison| poison.into_inner());
        let _ = permission_ask::withdraw(&db, self.agent_id, self.id);
    }
}

fn describe(answer: Answer) -> &'static str {
    match answer {
        Answer::Allow => "allowed",
        Answer::AllowAlways | Answer::AllowWithin(_) => "allowed always",
        Answer::Deny => "refused",
    }
}

#[cfg(test)]
mod tests {
    use glut::Event;

    use super::*;

    #[test]
    fn glut_s_log_of_a_card_reads_as_tile_steps() {
        let dir = scratch("log");
        let path = dir.join(glut_card::EVENTS_FILE);
        let log =
            glut_tui::log::EventLog::new(glut_tui::log::Target::At(path.clone()), dir.clone());
        log.owner("Start on card #1");
        log.event(&Event::ToolCall {
            turn: 1,
            id: "c1".into(),
            name: "edit_file".into(),
            arguments: r#"{"path":"README.md"}"#.into(),
        });
        // The log's head and the owner's message are no steps of a tile.
        let steps =
            axiomata_core::ide::activity::glut_recent(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].detail, "README.md");
        assert!(steps[0].at.is_some());
    }

    #[test]
    fn the_project_guide_joins_the_instructions_only_when_the_spec_asks_for_it() {
        let dir = scratch("guide");
        std::fs::write(dir.join(PROJECT_GUIDE), "Run cargo test first.\n").unwrap();
        let spec = CardSpec {
            agent_id: 9,
            cwd: dir.clone(),
            model: None,
            mode: "accept_edits".into(),
            allow: Vec::new(),
            deny: Vec::new(),
            mcp_config: dir.join("mcp.json"),
            instructions: "You work on card #7.".into(),
            prompt: None,
            project_guide: false,
        };
        assert_eq!(instructions(&spec), "You work on card #7.");
        let spec = CardSpec {
            project_guide: true,
            ..spec
        };
        let told = instructions(&spec);
        assert!(told.starts_with("You work on card #7."), "{told}");
        assert!(told.contains("Run cargo test first."), "{told}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_apps_hand_back_keeps_the_pane_waiting() {
        assert_eq!(from_app(Decision::Allow), Some(Answer::Allow));
        assert_eq!(from_app(Decision::Deny), Some(Answer::Deny));
        assert_eq!(from_app(Decision::Terminal), None);
    }

    /// A folder of its own for one test.
    fn scratch(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "axiomata-card-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_apps_theme_is_read_from_its_dashboard_and_nothing_else_counts() {
        let dir = scratch("theme");
        let file = dir.join("dashboard.json");
        std::fs::write(&file, r#"{"version":1,"settings":{"theme":"forest"}}"#).unwrap();
        assert_eq!(app_theme(&file).as_deref(), Some("forest"));
        std::fs::write(&file, r#"{"settings":{"theme":""}}"#).unwrap();
        assert_eq!(app_theme(&file), None);
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(app_theme(&file), None);
        assert_eq!(app_theme(&dir.join("gone.json")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session's questions over a database of its own.
    fn asks() -> (AppAsks, PathBuf) {
        let dir = scratch("asks");
        let db = axiomata_core::db::open_and_migrate_at(&dir.join("t.db")).unwrap();
        (
            AppAsks {
                db: Arc::new(Mutex::new(db)),
                agent_id: 9,
            },
            dir,
        )
    }

    fn request() -> AskRequest {
        AskRequest {
            tool: "bash".into(),
            target: "wc -l README.md".into(),
            scopes: Vec::new(),
        }
    }

    fn open_id(asks: &AppAsks) -> Option<i64> {
        let db = asks.db.lock().unwrap();
        permission_ask::open_ask(&db, asks.agent_id)
            .unwrap()
            .map(|ask| ask.id)
    }

    #[tokio::test]
    async fn the_pane_answers_first_and_the_app_is_told_by_the_stored_decision() {
        let (asks, dir) = asks();
        let request = request();
        let (answer, said) = asks.race(&request, Box::pin(async { Answer::Allow })).await;
        assert_eq!(answer, Answer::Allow);
        assert_eq!(said, None);
        let db = asks.db.lock().unwrap();
        assert_eq!(permission_ask::open_ask(&db, 9).unwrap(), None);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_app_answers_first_and_the_pane_stops_waiting() {
        let (asks, dir) = asks();
        let request = request();
        let answering = async {
            loop {
                if let Some(id) = open_id(&asks) {
                    let db = asks.db.lock().unwrap();
                    permission_ask::decide(&db, 9, id, Decision::Deny, "human:owner").unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let never: AnswerFuture<'_> = Box::pin(std::future::pending());
        let ((answer, said), ()) = tokio::join!(asks.race(&request, never), answering);
        assert_eq!(answer, Answer::Deny);
        assert_eq!(said.as_deref(), Some("answered in the app: refused"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_apps_hand_back_leaves_the_question_to_the_pane() {
        let (asks, dir) = asks();
        let request = request();
        let (typed, answer_typed) = tokio::sync::oneshot::channel::<Answer>();
        let answering = async {
            loop {
                if let Some(id) = open_id(&asks) {
                    {
                        let db = asks.db.lock().unwrap();
                        permission_ask::decide(&db, 9, id, Decision::Terminal, "human:owner")
                            .unwrap();
                    }
                    // The pane goes on waiting after the hand-back; its answer settles it.
                    tokio::time::sleep(APP_POLL * 2).await;
                    typed.send(Answer::Deny).unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let on_screen: AnswerFuture<'_> =
            Box::pin(async { answer_typed.await.unwrap_or(Answer::Deny) });
        let ((answer, _), ()) = tokio::join!(asks.race(&request, on_screen), answering);
        assert_eq!(answer, Answer::Deny);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_wait_called_off_withdraws_the_question_from_the_app() {
        let (asks, dir) = asks();
        let request = request();
        let never: AnswerFuture<'_> = Box::pin(std::future::pending());
        let called_off =
            tokio::time::timeout(Duration::from_millis(200), asks.race(&request, never)).await;
        assert!(called_off.is_err());
        assert_eq!(open_id(&asks), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
