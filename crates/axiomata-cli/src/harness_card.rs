//! `axiomata-cli harness card`: a Studio session on Glut, in its pane (`docs/plans/glut-cards.md`).
//!
//! The studio writes everything the session is started with into its channel (`axiomata_core::glut_card`); this reads
//! it back by the session's id, runs Glut in the session's folder and shows each step in the pane, as `harness run`
//! does. After an answer the pane takes the owner's next line and the same conversation goes on (C7); an empty line or
//! Ctrl-D ends the session. A question of the gate is asked in the pane and in the app at once, and the first answer
//! counts (C5): one reader owns the pane's input for the whole session, so a line read for a question the app answered
//! first is never left hanging.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::agent_entry::HARNESS_RUN_ENV;
use axiomata_core::glut_card::{self, CardSpec};
use axiomata_core::ide::lifecycle::{AgentState, Channel};
use axiomata_core::permission_ask::{self, Decision};
use glut::journal;
use glut::permissions::{
    Answer, AnswerFuture, AskRequest, Asker, GateOptions, ask_display, build_gate, printable,
};
use glut::setup;
use glut::skills;
use glut::{
    Budget, Cancel, Event, EventSink, FinishReason, Message, OpenAiCompat, Prices, RunConfig,
    SharedSink,
};
use tokio::sync::mpsc;

use crate::harness_cmd::Readable;

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

/// Runs the session the environment names.
pub async fn card(core: &AxiomataCore) -> Result<()> {
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
    check_spec(core, &spec, &channel)?;
    let config = crate::read_config(core);
    let endpoint = glut_card::endpoint(&config, spec.model.clone())?;
    let model_name = endpoint.model.clone();

    let input = Input::start();
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|home| home.is_absolute())
        .context("HOME is not set to an absolute folder")?;
    let asker = Arc::new(PaneAsker {
        input: input.clone(),
        db: Arc::clone(&core.db),
        agent_id,
        channel: Channel::for_agent(&roots, agent_id),
    });
    let options = GateOptions {
        mode: Some(spec.mode()?),
        allow: spec.allow.clone(),
        ask: Vec::new(),
        deny: spec.deny.clone(),
        // The project's own `.axh/` rules never widen a session the studio started.
        trusted: false,
    };
    let built = build_gate(&home, &spec.cwd, options, asker)?;
    for note in built.notes() {
        eprintln!("note: {note}");
    }
    let mut sink = SharedSink::new(Box::new(CardSink::new(&channel)));
    // The owner's skills, as a chat turn has them; a project's own only count in a trusted folder, which this is not.
    let skill_dirs = vec![axiomata_core::paths::global_skills_dir()];
    let (catalog, _) = skills::Catalog::load(&home, &spec.cwd, false, &skill_dirs);
    let catalog = Arc::new(catalog);
    let mut run_config = run_config(&spec, &config, &built, &home, &model_name);
    // Glut's own prices first, then the app's table (feedback "Glut standalone").
    let prices = built
        .prices(&model_name)
        .or_else(|| app_prices(&config, &model_name));
    let model = OpenAiCompat::new(endpoint)?;
    // The team server alone: the session's own file, never the owner's `~/.axh/mcp.json` nor the project's.
    let servers = glut::mcp::config::named(&spec.mcp_config)?;
    let extras = setup::Extras {
        search: axiomata_core::web_search::web_search_tool(&config),
        direct: Vec::new(),
    };
    let tools = setup::toolbox(
        &spec.cwd,
        &catalog,
        built.gate,
        servers,
        built.sandbox,
        extras,
        &mut sink,
    )
    .await?;

    let current = Arc::new(Mutex::new(Cancel::new()));
    watch_signals(Arc::clone(&current), Channel::for_agent(&roots, agent_id));
    watch_stop(Channel::for_agent(&roots, agent_id), Arc::clone(&current));

    let conversation_path = channel.dir().join(glut_card::CONVERSATION_FILE);
    let mut conversation = load_conversation(&conversation_path);
    let journals = channel.dir().join(glut_card::JOURNAL_DIR);
    journal::sweep(&journals);
    println!(
        "Glut · {model_name} · {}",
        printable(&spec.cwd.display().to_string())
    );
    let mut next = spec.prompt.clone();
    // A turn the pane lost (the app quit, the pane closed) goes on from its journal (C8): a session nobody watches
    // must not stand still on a question, and the owner can stop it with Ctrl-C. Only the newest one; any older is
    // given up.
    let mut cut_off = journal::list(&journals)
        .into_iter()
        .filter(|run| !run.running);
    if let Some(lost) = cut_off.next() {
        for older in cut_off {
            let _ = journal::discard(&journals, &older.run);
        }
        eprintln!(
            "note: a turn of this session was cut off (started {}); going on with it — Ctrl-C stops it",
            printable(&lost.started_at)
        );
        // The journal counts on from what the lost turn had used, which the session's reading counts already: the
        // budget would take it twice. The studio's stop (`watch_stop`) still bounds the turn.
        run_config.budget = Budget::default();
        // `resume` reads the folder of the journal from the config; the run goes on in its own journal.
        run_config.journal = Some(journal_spec(&journals, agent_id));
        let cancel = Cancel::new();
        *current.lock().unwrap_or_else(|poison| poison.into_inner()) = cancel.clone();
        let _ = channel.set_state(AgentState::Working);
        match glut::resume(&run_config, &model, &tools, &lost.run, &mut sink, &cancel).await {
            Ok((outcome, after)) => {
                conversation = after;
                if let Err(err) = save_conversation(&channel, &conversation) {
                    eprintln!("note: the conversation could not be kept: {err}");
                }
                if outcome.reason != FinishReason::Answered {
                    eprintln!("Glut stopped: {:?}", outcome.reason);
                }
                // The start message was part of the turn that went on: not sent again — unless the studio's start
                // says something the lost turn was not told.
                if next
                    .as_deref()
                    .is_some_and(|prompt| told(&conversation, prompt))
                {
                    next = None;
                }
            }
            Err(why) => {
                eprintln!("note: {}; starting the turn afresh", printable(&why));
                let _ = journal::discard(&journals, &lost.run);
            }
        }
    }
    loop {
        let prompt = match next.take() {
            Some(prompt) => prompt,
            None => {
                let _ = channel.set_state(AgentState::Idle);
                match input.prompt_line("› ").await {
                    Some(line) if !line.trim().is_empty() => line,
                    _ => break,
                }
            }
        };
        if let Some(reason) = channel.limit_reached() {
            eprintln!("error: this session is stopped: {}", printable(&reason));
            // Kept for when the limit is raised: Enter tries again, an empty input ends the session.
            next = Some(prompt);
            match input.prompt_line("(press Enter to try again) ").await {
                Some(_) => continue,
                None => break,
            }
        }
        // What is left of the session's limits, so a turn cannot spend them twice (C6).
        run_config.budget = budget(core, agent_id, prices).await;
        run_config.journal = Some(journal_spec(&journals, agent_id));
        let cancel = Cancel::new();
        *current.lock().unwrap_or_else(|poison| poison.into_inner()) = cancel.clone();
        let _ = channel.set_state(AgentState::Working);
        let (outcome, after) = glut::run_continuing(
            &run_config,
            &model,
            &tools,
            std::mem::take(&mut conversation),
            &prompt,
            &mut sink,
            &cancel,
        )
        .await;
        conversation = after;
        if let Err(err) = save_conversation(&channel, &conversation) {
            eprintln!("note: the conversation could not be kept: {err}");
        }
        if outcome.reason != FinishReason::Answered {
            eprintln!("Glut stopped: {:?}", outcome.reason);
        }
    }
    let _ = channel.set_state(AgentState::Ended);
    Ok(())
}

/// Whether `prompt` is one of the user's messages of `conversation` already.
fn told(conversation: &[Message], prompt: &str) -> bool {
    conversation.iter().any(|message| {
        message.role == glut::model::Role::User && message.content.as_deref() == Some(prompt)
    })
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
async fn budget(core: &AxiomataCore, agent_id: i64, prices: Option<Prices>) -> Budget {
    // A reading that fails is tried again; one that keeps failing gives the turn nothing to spend rather than everything.
    let mut usage = None;
    for _ in 0..BUDGET_TRIES {
        match axiomata_core::session_limits::session_usage(core, agent_id).await {
            Ok(None) => return Budget::default(),
            Ok(Some(found)) => {
                usage = Some(found);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
    let Some(usage) = usage else {
        eprintln!("error: what this session used cannot be read; the turn is not started");
        return Budget {
            max_tokens: Some(0),
            ..Budget::default()
        };
    };
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
    Budget {
        max_tokens: Some(tokens),
        max_usd: money,
        prices,
    }
}

/// The studio's stop (a session at its limit, `session_limits`): the turn under way ends at once, as Claude Code's hook
/// refuses its next call. The pane then refuses further turns until the owner raised the limit.
fn watch_stop(channel: Channel, current: Arc<Mutex<Cancel>>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(STOP_POLL).await;
            if channel.limit_reached().is_some() {
                current
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .cancel();
            }
        }
    });
}

/// The session's lock, held while the pane runs.
fn hold_session(channel: &Channel) -> Result<std::fs::File> {
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
fn journal_spec(dir: &std::path::Path, agent_id: i64) -> journal::JournalSpec {
    journal::JournalSpec {
        dir: dir.to_path_buf(),
        run: format!("turn-{}", chrono::Utc::now().timestamp_millis()),
        meta: serde_json::json!({ "agent": agent_id }),
    }
}

/// How often a failing reading of the session's usage is tried before the turn is refused.
const BUDGET_TRIES: u32 = 3;

/// How often the pane looks for the studio's stop.
const STOP_POLL: Duration = Duration::from_secs(2);

/// The run's settings: the spec's folder and instructions, the project's guide, and Glut's own sources for everything
/// else (`~/.axh/settings.json`, the app's `[agents.glut]`).
fn run_config(
    spec: &CardSpec,
    config: &axiomata_core::config::Config,
    built: &glut::permissions::BuiltGate,
    home: &std::path::Path,
    model: &str,
) -> RunConfig {
    let mut run_config = RunConfig::new(&spec.cwd);
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
    run_config.instructions = Some(instructions);
    run_config.max_turns = CARD_MAX_TURNS;
    run_config.timeout = CARD_TIMEOUT;
    run_config.refusals = built.refusals;
    run_config.compaction = built.compaction(model);
    run_config.hooks = built.hooks.clone();
    run_config.sandbox = Some(built.sandbox.clone());
    run_config.codemode = config.agents.glut.codemode;
    // The team tools are the session's work, not a lookup: offered directly.
    run_config.mcp_behind_code = false;
    run_config.snapshots = Some(home.join(".axh").join("snapshots"));
    run_config.savings = Some(home.join(".axh").join(glut::lean::FILE));
    run_config
}

/// The project's `AGENTS.md`, read through the guarded walk (no link followed out of the folder), when it has one.
fn project_guide(cwd: &std::path::Path) -> Option<String> {
    let root = axiomata_files::Root::dir(cwd, axiomata_files::LinkPolicy::Strict).ok()?;
    let file = axiomata_files::read_text(&root, PROJECT_GUIDE, MAX_GUIDE_BYTES).ok()?;
    let text = file.content.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn load_conversation(path: &std::path::Path) -> Vec<Message> {
    if !path.exists() {
        return Vec::new();
    }
    match glut_card::read_private(path, glut_card::MAX_CONVERSATION_BYTES)
        .map_err(anyhow::Error::from)
        .and_then(|text| serde_json::from_str(&text).map_err(anyhow::Error::from))
    {
        Ok(messages) => messages,
        Err(err) => {
            eprintln!("note: the earlier conversation is not read back ({err}); starting afresh");
            Vec::new()
        }
    }
}

fn save_conversation(channel: &Channel, conversation: &[Message]) -> Result<()> {
    let text = serde_json::to_string(conversation)?;
    channel.write_private(glut_card::CONVERSATION_FILE, &text)?;
    Ok(())
}

/// Ctrl-C ends the turn under way, not the session; SIGTERM and SIGHUP (the pane closed) end both, and the session
/// says it ended — a state left at `working` would read as busy to the studio forever.
fn watch_signals(current: Arc<Mutex<Cancel>>, channel: Channel) {
    use tokio::signal::unix::{SignalKind, signal};
    tokio::spawn(async move {
        let (Ok(mut int), Ok(mut term), Ok(mut hup)) = (
            signal(SignalKind::interrupt()),
            signal(SignalKind::terminate()),
            signal(SignalKind::hangup()),
        ) else {
            return;
        };
        loop {
            let last = tokio::select! {
                _ = int.recv() => false,
                _ = term.recv() => true,
                _ = hup.recv() => true,
            };
            current
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .cancel();
            if last {
                // Nothing is left to read once the pane is gone: the session ends where it stands, its journal kept.
                let _ = channel.set_state(AgentState::Ended);
                std::process::exit(130);
            }
        }
    });
}

/// The pane's input, read by one thread for the whole session: each line goes to whoever waits for one — a question
/// of the gate or the next message.
#[derive(Clone)]
struct Input {
    lines: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>>,
}

impl Input {
    fn start() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                match stdin.read_line(&mut line) {
                    // The end of the input closes the channel: whoever waits hears "no more lines".
                    Ok(0) => break,
                    // A terminal another program left non-blocking answers "not now" rather than waiting: read on.
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                    Ok(_) => {
                        if tx
                            .send(line.trim_end_matches(['\n', '\r']).to_owned())
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        Input::of(rx)
    }

    /// The input of lines that arrive on `rx`.
    fn of(rx: mpsc::UnboundedReceiver<String>) -> Self {
        Input {
            lines: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    /// Shows `prompt` and waits for the next line; `None` when the input has ended.
    async fn prompt_line(&self, prompt: &str) -> Option<String> {
        let mut lines = self.lines.lock().await;
        print!("{prompt}");
        let _ = std::io::stdout().flush();
        lines.recv().await
    }

    /// Throws away what was typed while nobody asked, so a "y" typed ahead never answers a question it did not see.
    async fn drop_typed_ahead(&self) -> usize {
        let mut lines = self.lines.lock().await;
        let mut dropped = 0;
        while lines.try_recv().is_ok() {
            dropped += 1;
        }
        dropped
    }
}

/// How often the pane looks whether the app answered.
const APP_POLL: Duration = Duration::from_millis(500);

/// Asks in the pane and in the app at once (C5): the question is stored where Claude Code's hook stores its own, so the
/// waiting badge and the app's dialog work as they do; whichever answer comes first settles it.
struct PaneAsker {
    input: Input,
    db: Arc<Mutex<rusqlite::Connection>>,
    agent_id: i64,
    channel: Channel,
}

impl PaneAsker {
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
}

/// The pane's answer to a question: `y`/`ja`, `always`/`immer`, anything else is no.
fn typed_answer(line: &str) -> Answer {
    match line.trim().to_lowercase().as_str() {
        "y" | "j" | "yes" | "ja" => Answer::Allow,
        "always" | "immer" => Answer::AllowAlways,
        _ => Answer::Deny,
    }
}

fn from_app(decision: Decision) -> Option<Answer> {
    match decision {
        Decision::Allow => Some(Answer::Allow),
        Decision::Deny => Some(Answer::Deny),
        // "Answer it in the pane": the pane goes on waiting alone.
        Decision::Terminal => None,
    }
}

impl Asker for PaneAsker {
    fn ask<'a>(&'a self, request: &'a AskRequest) -> AnswerFuture<'a> {
        Box::pin(async move {
            let dropped = self.input.drop_typed_ahead().await;
            if dropped > 0 {
                eprintln!("  (left {dropped} line(s) typed while Glut was working unread)");
            }
            let _ = self.channel.set_state(AgentState::Waiting);
            let stored = self.store(request);
            // Whatever ends this wait — an answer, the input ending, the turn called off — the app stops offering it.
            let _withdrawn = stored.map(|id| Withdraw {
                db: Arc::clone(&self.db),
                agent_id: self.agent_id,
                id,
            });
            let question = format!(
                "  ? {} {} — allow? [y]es / always / [n]o: ",
                printable(&request.tool),
                ask_display(&request.target)
            );
            let mut lines = self.input.lines.lock().await;
            eprint!("{question}");
            let _ = std::io::stderr().flush();
            let mut app_open = stored.is_some();
            let answer = loop {
                tokio::select! {
                    line = lines.recv() => {
                        let Some(line) = line else { break Answer::Deny };
                        let typed = typed_answer(&line);
                        let decision = if typed == Answer::Deny { Decision::Deny } else { Decision::Allow };
                        match stored.and_then(|id| self.settle(id, decision)).and_then(from_app) {
                            Some(first) => {
                                eprintln!("  (answered in the app first: {})", describe(first));
                                break first;
                            }
                            None => break typed,
                        }
                    }
                    () = tokio::time::sleep(APP_POLL), if app_open => {
                        let Some(id) = stored else { continue };
                        match self.app_decision(id) {
                            None => {}
                            Some(decision) => match from_app(decision) {
                                Some(answer) => {
                                    eprintln!("answered in the app: {}", describe(answer));
                                    break answer;
                                }
                                None => app_open = false,
                            },
                        }
                    }
                }
            };
            let _ = self.channel.set_state(AgentState::Working);
            answer
        })
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

/// The pane's output, and every event as a JSON line into the channel for the meter and the Flow tiles (C6).
struct CardSink {
    readable: Readable,
    log: Option<std::fs::File>,
}

impl CardSink {
    fn new(channel: &Channel) -> Self {
        use std::os::unix::fs::OpenOptionsExt as _;
        let path = channel.dir().join(glut_card::EVENTS_FILE);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)
            .inspect_err(|err| eprintln!("note: the session's events are not kept ({err})"))
            .ok();
        CardSink {
            readable: Readable,
            log,
        }
    }
}

impl EventSink for CardSink {
    fn emit(&mut self, event: Event) {
        // The streamed pieces of text and thinking are the pane's alone: the whole text follows as one event, and
        // the thinking is no part of the log the tiles and the meter read.
        if let Some(log) = &mut self.log
            && !streamed(&event)
            && let Some(line) = logged(&event, chrono::Utc::now())
        {
            let _ = writeln!(log, "{line}");
        }
        self.readable.emit(event);
    }
}

/// A streamed piece of text or thinking — a subagent's too: the pane shows them, the log keeps only whole events.
fn streamed(event: &Event) -> bool {
    match event {
        Event::TextDelta { .. } | Event::ReasoningDelta { .. } => true,
        Event::Sub { event, .. } => streamed(event),
        _ => false,
    }
}

/// One line of the event log: the event as Glut writes it, with the time it happened in `at` — the Flow tiles show
/// how long ago a step was, and Glut's events carry no time of their own.
fn logged(event: &Event, at: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let mut value = serde_json::to_value(event).ok()?;
    value
        .as_object_mut()?
        .insert("at".into(), serde_json::Value::String(at.to_rfc3339()));
    serde_json::to_string(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_clear_yes_allows() {
        assert_eq!(typed_answer("y"), Answer::Allow);
        assert_eq!(typed_answer(" Ja "), Answer::Allow);
        assert_eq!(typed_answer("immer"), Answer::AllowAlways);
        assert_eq!(typed_answer("always"), Answer::AllowAlways);
        assert_eq!(typed_answer(""), Answer::Deny);
        assert_eq!(typed_answer("yes please"), Answer::Deny);
    }

    #[test]
    fn a_logged_event_carries_its_time_and_reads_as_a_tile_step() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-21T13:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let call = Event::ToolCall {
            turn: 1,
            id: "c1".into(),
            name: "edit_file".into(),
            arguments: r#"{"path":"README.md"}"#.into(),
        };
        let line = logged(&call, at).unwrap();
        let steps = axiomata_core::ide::activity::glut_recent(&line);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].detail, "README.md");
        assert!(
            steps[0]
                .at
                .as_deref()
                .unwrap()
                .starts_with("2026-10-21T13:00:00")
        );
    }

    #[test]
    fn the_apps_hand_back_keeps_the_pane_waiting() {
        assert_eq!(from_app(Decision::Allow), Some(Answer::Allow));
        assert_eq!(from_app(Decision::Deny), Some(Answer::Deny));
        assert_eq!(from_app(Decision::Terminal), None);
    }

    /// A session's asker over a database of its own and lines the test sends.
    fn asker() -> (PaneAsker, mpsc::UnboundedSender<String>, std::path::PathBuf) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "axiomata-pane-asker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = axiomata_core::db::open_and_migrate_at(&dir.join("t.db")).unwrap();
        let roots = axiomata_core::ide::lifecycle::ChannelRoots {
            events: dir.join("events"),
            claude_tasks: dir.join("tasks"),
            claude_plans: dir.join("plans"),
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let asker = PaneAsker {
            input: Input::of(rx),
            db: Arc::new(Mutex::new(db)),
            agent_id: 9,
            channel: Channel::for_agent(&roots, 9),
        };
        (asker, tx, dir)
    }

    fn request() -> AskRequest {
        AskRequest {
            tool: "bash".into(),
            target: "wc -l README.md".into(),
            scopes: Vec::new(),
        }
    }

    fn open_id(asker: &PaneAsker) -> Option<i64> {
        let db = asker.db.lock().unwrap();
        permission_ask::open_ask(&db, asker.agent_id)
            .unwrap()
            .map(|ask| ask.id)
    }

    #[tokio::test]
    async fn the_pane_answers_first_and_the_app_is_told_by_the_stored_decision() {
        let (asker, tx, _dir) = asker();
        let request = request();
        let typing = async {
            // Typed once the question is there, as the owner does.
            while open_id(&asker).is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            tx.send("y".into()).unwrap();
        };
        let (answer, ()) = tokio::join!(asker.ask(&request), typing);
        assert_eq!(answer, Answer::Allow);
        let db = asker.db.lock().unwrap();
        assert_eq!(permission_ask::open_ask(&db, 9).unwrap(), None);
    }

    #[tokio::test]
    async fn the_app_answers_first_and_the_pane_stops_waiting() {
        let (asker, _tx, _dir) = asker();
        let request = request();
        let answering = async {
            loop {
                if let Some(id) = open_id(&asker) {
                    let db = asker.db.lock().unwrap();
                    permission_ask::decide(&db, 9, id, Decision::Deny, "human:owner").unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let (answer, ()) = tokio::join!(asker.ask(&request), answering);
        assert_eq!(answer, Answer::Deny);
    }

    #[tokio::test]
    async fn the_apps_hand_back_and_a_line_typed_ahead_leave_the_question_to_the_next_line() {
        let (asker, tx, _dir) = asker();
        // Typed while Glut was working: never an answer to a question it did not see.
        tx.send("y".into()).unwrap();
        let request = request();
        let answering = async {
            loop {
                if let Some(id) = open_id(&asker) {
                    {
                        let db = asker.db.lock().unwrap();
                        permission_ask::decide(&db, 9, id, Decision::Terminal, "human:owner")
                            .unwrap();
                    }
                    // The pane goes on waiting after the hand-back; its next line answers.
                    tokio::time::sleep(APP_POLL * 2).await;
                    tx.send("n".into()).unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let (answer, ()) = tokio::join!(asker.ask(&request), answering);
        assert_eq!(answer, Answer::Deny);
    }

    #[tokio::test]
    async fn an_ended_input_refuses_and_withdraws_the_question_from_the_app() {
        let (asker, tx, _dir) = asker();
        drop(tx);
        assert_eq!(asker.ask(&request()).await, Answer::Deny);
        assert_eq!(open_id(&asker), None);
    }
}
