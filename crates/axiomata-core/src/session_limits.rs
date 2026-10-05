//! Limits per card session: measuring what a session used, and stopping it when it is used up
//! (`docs/plans/a2a.md`, A9, A27 and A33; built in CP-A6c).
//!
//! A card session runs unattended, so nothing but a limit keeps a confused one from spending all night. The limits are
//! the role's own (`limits:` in its `AGENT.md`) over the defaults of its tier, and there are three: **steps** (tool
//! calls), **tokens**, and — for an engine paid per token only — **money**, metered from the owner's price table like
//! every other spend (`spend::metered_cost_usd`). A subscription engine is limited in tokens and steps, never in a dollar
//! figure that would be made up.
//!
//! What a stop is: a file in the session's channel ([`Channel::set_limit_reached`]) that Claude Code's `PreToolUse` hook
//! turns into a refusal of every further tool call, so the step it is in ends cleanly and nothing is left half written;
//! an Opencode session, which has no such hook, is interrupted through the service. The card **keeps its claim and its
//! worktree** — the owner decides, in the history of the card, whether to raise the limit (the role's) and go on, or to
//! release the card. A stop that the limit no longer justifies (it was raised) lifts itself at the next look.
//!
//! The look is [`Meter::check`], called every few seconds by the app; [`card_usage`] is the one-off reading behind
//! `board usage` and the card's detail. Neither needs the session to cooperate: both read what the harness leaves
//! behind ([`axiomata_ide::usage`]). A source that cannot be read gives `measured: false` and no stop — except by
//! steps, which are counted from the same source, so an unreadable session is not limited at all. That is a known gap,
//! reported in the card's usage rather than hidden.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axiomata_ide::lifecycle::Channel;
use axiomata_ide::model::{Agent, Harness};
use axiomata_ide::usage::{ClaudeTally, Usage};
use axiomata_roster::{Billing, ResolvedLimits, Role, Tier};
use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::agents::opencode;
use crate::board::{EventKind, flow};
use crate::config::Config;
use crate::ide::agent_store;
use crate::paths;
use crate::session::actor_from;
use crate::spend;

/// What a session was used for and what it has used, for the owner to read.
#[derive(Debug, Clone, Serialize)]
pub struct SessionUsage {
    pub agent_id: i64,
    pub name: String,
    pub role: String,
    /// A reviewer's usage is shown beside the worker's but counts against its own limits.
    pub review: bool,
    pub billing: Billing,
    pub usage: Usage,
    /// Money, for an engine paid per token whose model has a price in the owner's table; `None` otherwise.
    pub cost_usd: Option<f64>,
    pub limits: ResolvedLimits,
    /// `false` when the harness's own record could not be read: the figures above are then zeros, not a finding.
    pub measured: bool,
    /// Why not, when it is not.
    pub unmeasured: Option<Unmeasured>,
    /// Why the session is stopped, while it is.
    pub stopped: Option<String>,
}

/// Why a session's usage could not be read, for the owner to be told (the Studio says it in words).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unmeasured {
    /// No Claude Code session id was noted for the session: it was started before they were, or with a command of its own.
    NoSessionId,
    /// Ids were noted, but the harness has written no record under any of them yet — a session that has just started has
    /// said nothing so far.
    NoRecordYet,
    /// A record exists and could not be read.
    ReadFailed,
    /// The Opencode session was not made yet.
    NoOpencodeSession,
    /// The Opencode service is not running.
    ServiceDown,
    /// The Opencode service did not answer, or refused.
    ServiceFailed,
    /// This harness is not measured.
    NotMeasured,
}

/// A session that was stopped just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breach {
    /// The card the session worked on, or reviewed; `None` for a planner.
    pub card_id: Option<i64>,
    /// The plan a planner works on; `None` for a card session.
    pub plan_id: Option<i64>,
    pub project_id: i64,
    pub agent_id: i64,
    pub agent_name: String,
    pub reason: String,
}

/// Which limit a measurement hits, in words; `None` while it is within all of them.
///
/// The money limit counts only for an engine paid per token and only when a cost could be metered: a missing price
/// must not read as "free".
pub fn reached(
    usage: Usage,
    cost_usd: Option<f64>,
    limits: ResolvedLimits,
    billing: Billing,
) -> Option<String> {
    if usage.steps >= limits.max_steps {
        return Some(format!(
            "{} steps of {} used",
            usage.steps, limits.max_steps
        ));
    }
    if usage.tokens() >= limits.max_tokens {
        return Some(format!(
            "{} tokens of {} used",
            usage.tokens(),
            limits.max_tokens
        ));
    }
    if billing == Billing::Metered
        && let Some(cost) = cost_usd
        && cost >= limits.max_cost_usd
    {
        return Some(format!("${cost:.2} of ${:.2} spent", limits.max_cost_usd));
    }
    None
}

/// Everything the watcher needs about one session, read while the database is locked so the measuring can run without
/// it.
struct Watched {
    agent: Agent,
    role_name: String,
    limits: ResolvedLimits,
    billing: Billing,
    model: Option<String>,
    channel: Channel,
    /// The studio still wants this session: its card is where it works on it. Only such a session is stopped.
    wanted: bool,
}

fn gather(core: &AxiomataCore, only_card: Option<i64>) -> Result<Vec<Watched>, AxiomataError> {
    let config = core.config_read().clone();
    let db = core.db_lock();
    let roots = paths::ide_locations().channels;
    let mut roles: HashMap<i64, Vec<Role>> = HashMap::new();
    let mut watched = Vec::new();
    for agent in agent_store::card_sessions(&db)?
        .into_iter()
        .chain(agent_store::plan_sessions(&db)?)
    {
        if only_card.is_some_and(|card| agent.card_id != Some(card)) {
            continue;
        }
        let project_roles = roles
            .entry(agent.project_id)
            .or_insert_with(|| crate::roster::roles_for_project(&db, &config, agent.project_id));
        let role = project_roles
            .iter()
            .find(|role| role.name == agent.agent_role);
        let limits = role.map_or_else(
            || Tier::default().default_limits(),
            |role| role.limits.resolve(role.tier),
        );
        let engine = agent
            .engine_id
            .as_deref()
            .and_then(|id| config.agents.engines.get(id));
        watched.push(Watched {
            billing: engine.map_or_else(Billing::default, |engine| engine.billing),
            model: agent
                .model
                .clone()
                .or_else(|| engine.and_then(|engine| engine.model.clone())),
            role_name: agent.agent_role.clone(),
            channel: Channel::for_agent(&roots, agent.id),
            wanted: crate::ide_start::launch_of(&db, &agent).is_some(),
            limits,
            agent,
        });
    }
    Ok(watched)
}

/// Remembers how far each Claude Code transcript was read, so a look every few seconds reads only what was appended.
#[derive(Debug)]
pub struct Meter {
    /// Where Claude Code keeps its transcripts.
    projects: PathBuf,
    transcripts: HashMap<PathBuf, ClaudeTally>,
    /// The most an Opencode session was read to have used: the service is asked for a bounded window of its messages,
    /// so a long session would otherwise read as using *less* once old messages fall out of it.
    opencode_floor: HashMap<String, Usage>,
    /// The Opencode service as found by this look (`Some(None)`: not running). Asked once per look: finding it runs a
    /// process, and there is one call per session otherwise.
    service: Option<Option<axiomata_opencode::Service>>,
}

/// The longest one Opencode request of a look may take. The service is local; one that does not answer must not hold
/// back the stop of the sessions behind it.
const OPENCODE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl Default for Meter {
    fn default() -> Self {
        Meter::with_projects(paths::claude_projects_dir())
    }
}

impl Meter {
    /// A meter that looks for Claude Code's transcripts under `projects`.
    pub fn with_projects(projects: PathBuf) -> Self {
        Meter {
            projects,
            transcripts: HashMap::new(),
            opencode_floor: HashMap::new(),
            service: None,
        }
    }

    async fn service(&mut self) -> Option<&axiomata_opencode::Service> {
        if self.service.is_none() {
            self.service = Some(opencode::running_service().await);
        }
        self.service.as_ref()?.as_ref()
    }

    /// What `watched` has used, `None` when the harness's record could not be read.
    async fn measure(
        &mut self,
        watched: &Watched,
        seen: &mut Vec<PathBuf>,
    ) -> Result<Usage, Unmeasured> {
        match watched.agent.harness {
            Harness::ClaudeCode => {
                let ids = watched.channel.claude_sessions();
                if ids.is_empty() {
                    return Err(Unmeasured::NoSessionId);
                }
                let mut total: Option<Usage> = None;
                let mut failed = false;
                for id in ids {
                    let Some(path) = find_transcript(&self.projects, &id) else {
                        continue;
                    };
                    // Reading a file is blocking work, and the file may be large: off the runtime's threads.
                    let mut tally = self.transcripts.remove(&path).unwrap_or_default();
                    let reading = path.clone();
                    let (tally, read) = tokio::task::spawn_blocking(move || {
                        let read = tally.advance(&reading);
                        (tally, read)
                    })
                    .await
                    .map_err(|_| Unmeasured::ReadFailed)?;
                    let usage = tally.usage();
                    self.transcripts.insert(path.clone(), tally);
                    if let Err(err) = read {
                        tracing::debug!(%err, path = %path.display(), "could not read a Claude Code transcript");
                        failed = true;
                        continue;
                    }
                    total = Some(total.unwrap_or_default().plus(usage));
                    seen.push(path);
                }
                total.ok_or(if failed {
                    Unmeasured::ReadFailed
                } else {
                    Unmeasured::NoRecordYet
                })
            }
            Harness::Opencode => {
                let session = watched
                    .agent
                    .opencode_session
                    .as_deref()
                    .ok_or(Unmeasured::NoOpencodeSession)?;
                let service = self.service().await.ok_or(Unmeasured::ServiceDown)?.clone();
                match tokio::time::timeout(
                    OPENCODE_TIMEOUT,
                    opencode::session_usage(&service, session),
                )
                .await
                {
                    Ok(Ok(usage)) => {
                        let floor = self.opencode_floor.entry(session.to_owned()).or_default();
                        *floor = floor.at_least(usage);
                        Ok(*floor)
                    }
                    Ok(Err(err)) => {
                        tracing::debug!(%err, "could not read an Opencode session's messages");
                        Err(Unmeasured::ServiceFailed)
                    }
                    Err(_) => {
                        tracing::debug!("the Opencode service did not answer in time");
                        Err(Unmeasured::ServiceFailed)
                    }
                }
            }
            Harness::Mini => Err(Unmeasured::NotMeasured),
        }
    }

    async fn read(
        &mut self,
        watched: &Watched,
        config: &Config,
        seen: &mut Vec<PathBuf>,
    ) -> SessionUsage {
        let (usage, unmeasured) = match self.measure(watched, seen).await {
            Ok(usage) => (Some(usage), None),
            Err(why) => (None, Some(why)),
        };
        let cost_usd = usage.and_then(|usage| {
            spend::metered_cost_usd(
                config,
                watched.model.as_deref(),
                Some(usage.input_tokens),
                Some(usage.output_tokens),
            )
        });
        SessionUsage {
            agent_id: watched.agent.id,
            name: watched.agent.name.clone(),
            role: watched.role_name.clone(),
            review: watched.agent.card_review,
            billing: watched.billing,
            usage: usage.unwrap_or_default(),
            cost_usd: if watched.billing == Billing::Metered {
                cost_usd
            } else {
                None
            },
            limits: watched.limits,
            measured: usage.is_some(),
            unmeasured,
            stopped: watched.channel.limit_reached().map(|text| reason_of(&text)),
        }
    }

    /// Opencode has no hook to refuse a tool call, so its session is interrupted; Claude Code's stop is the hook.
    async fn interrupt(&mut self, entry: &Watched) {
        if entry.agent.harness != Harness::Opencode {
            return;
        }
        let Some(session) = entry.agent.opencode_session.as_deref() else {
            return;
        };
        let Some(service) = self.service().await.cloned() else {
            return;
        };
        match tokio::time::timeout(
            OPENCODE_TIMEOUT,
            opencode::interrupt_session(&service, session),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::warn!(%err, "could not interrupt a session at its limit"),
            Err(_) => tracing::warn!("the Opencode service did not answer an interrupt in time"),
        }
    }

    /// One look at every card session the studio still wants: stops the ones that used up a limit, lifts the stop of
    /// the ones that no longer do, and returns the stops made just now.
    ///
    /// Stopping writes the marker, adds a `limit_stop` line to the card's history and, for Opencode, interrupts the
    /// session — and keeps interrupting it at every look while the stop stands, because the owner may type into the
    /// pane again; going on is done by raising the limit, not by talking past it.
    pub async fn check(&mut self, core: &AxiomataCore) -> Vec<Breach> {
        let watched = match gather(core, None) {
            Ok(watched) => watched,
            Err(err) => {
                tracing::warn!(%err, "could not look for card sessions at their limits");
                return Vec::new();
            }
        };
        let config = core.config_read().clone();
        self.service = None;
        let mut seen = Vec::new();
        let mut breaches = Vec::new();
        for entry in watched.iter().filter(|entry| entry.wanted) {
            let read = self.read(entry, &config, &mut seen).await;
            let hit = if read.measured {
                reached(read.usage, read.cost_usd, read.limits, read.billing)
            } else {
                None
            };
            match decide(hit, read.stopped.is_some(), read.measured) {
                Step::Stop(reason) => {
                    let breach = {
                        let config = core.config_read().clone();
                        let db = core.db_lock();
                        stop(&db, &config, entry, &reason)
                    };
                    breaches.extend(breach);
                    self.interrupt(entry).await;
                }
                Step::KeepStopped => self.interrupt(entry).await,
                Step::Lift => {
                    if let Err(err) = entry.channel.clear_limit_reached() {
                        tracing::warn!(%err, "could not lift a session's limit stop");
                    }
                }
                Step::Leave => {}
            }
        }
        // A transcript of a session that is gone needs no memory.
        self.transcripts.retain(|path, _| seen.contains(path));
        breaches
    }
}

/// The transcript of Claude Code session `id`: the file `<id>.jsonl` in one of the folders under `projects` (one per
/// working directory, named after it in a way that is Claude's to change). `id` is a UUID, so the match is exact and
/// cannot leave the folder.
fn find_transcript(projects: &Path, id: &str) -> Option<PathBuf> {
    if !axiomata_ide::usage::is_claude_session_id(id) {
        return None;
    }
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|folder| folder.path().join(format!("{id}.jsonl")))
        .find(|path| path.is_file())
}

const MARKER_BEFORE: &str = "Axiomata: this session has reached its limit (";
const MARKER_AFTER: &str = "). Do not call any more tools. Tell the owner in one sentence where you stopped; the owner \
     decides whether you go on.";

/// What the stop file says: what Claude Code's hook hands the model when it refuses a tool call.
fn marker_text(reason: &str) -> String {
    format!("{MARKER_BEFORE}{reason}{MARKER_AFTER}")
}

/// The reason out of what [`marker_text`] wrote, for showing to the owner without the instruction to the model.
fn reason_of(marker: &str) -> String {
    marker
        .strip_prefix(MARKER_BEFORE)
        .and_then(|rest| rest.strip_suffix(MARKER_AFTER))
        .unwrap_or(marker)
        .to_owned()
}

/// What one look at one session leads to.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Over a limit and not stopped yet.
    Stop(String),
    /// Over a limit and stopped already: the stop stands.
    KeepStopped,
    /// Stopped, and measured to be within its limits now (they were raised): the stop is lifted.
    Lift,
    Leave,
}

/// A session that could not be measured is never stopped and never released: no reading is not a finding.
fn decide(hit: Option<String>, stopped: bool, measured: bool) -> Step {
    match (hit, stopped) {
        (Some(reason), false) => Step::Stop(reason),
        (Some(_), true) => Step::KeepStopped,
        (None, true) if measured => Step::Lift,
        (None, _) => Step::Leave,
    }
}

/// Stops a session: marker, history line. `None` when the marker could not be written — then nothing was stopped and
/// the next look tries again.
fn stop(
    db: &rusqlite::Connection,
    config: &Config,
    entry: &Watched,
    reason: &str,
) -> Option<Breach> {
    if let Err(err) = entry.channel.set_limit_reached(&marker_text(reason)) {
        tracing::warn!(%err, "could not stop a session at its limit");
        return None;
    }
    if let Some(card_id) = entry.agent.card_id {
        let text = format!(
            "{} was stopped: {reason}. The card stays with it. To go on, raise the limit of the role \"{}\"; \
             otherwise release the card.",
            entry.agent.name, entry.role_name
        );
        let actor = actor_from(Some(&entry.agent.id.to_string()), Some(&entry.agent.name))
            .unwrap_or_else(|| "human:owner".to_owned());
        if let Err(err) = flow::add_event(db, card_id, &actor, EventKind::LimitStop, &text) {
            tracing::warn!(%err, "could not write the limit stop into the card's history");
        }
        crate::board_mirror::after_card_change(db, config, card_id);
    }
    // A planner has no card to write the stop on: the owner hears of it through the notice, and the plan's panel shows
    // the session stopped.
    Some(Breach {
        card_id: entry.agent.card_id,
        plan_id: entry.agent.plan_id,
        project_id: entry.agent.project_id,
        agent_id: entry.agent.id,
        agent_name: entry.agent.name.clone(),
        reason: reason.to_owned(),
    })
}

/// What the sessions of a card have used, worker and reviewers, for `board usage` and the card's detail. A fresh
/// reading: it looks at every transcript from the start.
///
/// # Errors
///
/// A database error.
pub async fn card_usage(
    core: &AxiomataCore,
    card_id: i64,
) -> Result<Vec<SessionUsage>, AxiomataError> {
    let watched = gather(core, Some(card_id))?;
    let config = core.config_read().clone();
    let mut meter = Meter::default();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for entry in &watched {
        out.push(meter.read(entry, &config, &mut seen).await);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ResolvedLimits {
        ResolvedLimits {
            max_cost_usd: 1.0,
            max_tokens: 1_000,
            max_steps: 10,
        }
    }

    fn used(tokens: u64, steps: u32) -> Usage {
        Usage {
            input_tokens: tokens,
            output_tokens: 0,
            steps,
        }
    }

    #[test]
    fn a_session_within_every_limit_is_not_stopped() {
        assert_eq!(
            reached(used(999, 9), Some(0.99), limits(), Billing::Metered),
            None
        );
    }

    #[test]
    fn each_limit_stops_a_session_and_the_reason_names_it() {
        let steps = reached(used(1, 10), None, limits(), Billing::Metered).unwrap();
        assert!(steps.contains("steps"), "{steps}");
        let tokens = reached(used(1_000, 1), None, limits(), Billing::Subscription).unwrap();
        assert!(tokens.contains("tokens"), "{tokens}");
        let money = reached(used(1, 1), Some(1.0), limits(), Billing::Metered).unwrap();
        assert!(money.contains('$'), "{money}");
    }

    #[test]
    fn a_subscription_is_never_stopped_for_money() {
        assert_eq!(
            reached(used(1, 1), Some(50.0), limits(), Billing::Subscription),
            None
        );
    }

    #[test]
    fn a_missing_price_does_not_read_as_free_money_or_as_a_stop() {
        assert_eq!(reached(used(1, 1), None, limits(), Billing::Metered), None);
    }

    #[test]
    fn a_transcript_is_found_by_its_session_id_in_any_project_folder_and_nothing_else_is() {
        let root = std::env::temp_dir().join(format!("axiomata-limits-{}", std::process::id()));
        let folder = root.join("-some-worktree");
        std::fs::create_dir_all(&folder).unwrap();
        let id = axiomata_ide::usage::new_claude_session_id().unwrap();
        assert_eq!(find_transcript(&root, &id), None);
        std::fs::write(folder.join(format!("{id}.jsonl")), "").unwrap();
        assert_eq!(
            find_transcript(&root, &id),
            Some(folder.join(format!("{id}.jsonl")))
        );
        assert_eq!(find_transcript(&root, "../x"), None);
        assert_eq!(find_transcript(&root.join("missing"), &id), None);
    }

    use std::sync::atomic::{AtomicU32, Ordering};

    use axiomata_board::{CardFields, NewCard, store};
    use axiomata_ide::lifecycle::ChannelRoots;
    use axiomata_ide::model::{AgentFields, NewAgent, NewProject};
    use serde_json::json;

    use crate::db;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A database with one card, a session working on it and the session's channel, in a scratch directory.
    struct World {
        dir: PathBuf,
        db: rusqlite::Connection,
        watched: Watched,
        card: i64,
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn world(harness: Harness) -> World {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-session-limits-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let mut db = db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = axiomata_ide::store::create_project(
            &db,
            NewProject {
                name: "P".into(),
                repo_root: repo,
            },
        )
        .unwrap()
        .id;
        let board = store::create_board(&mut db, "B").unwrap();
        let column = store::list_columns(&db, board.id).unwrap()[1].id;
        let card = store::create_card(
            &db,
            &NewCard {
                column_id: column,
                fields: CardFields {
                    title: "work".into(),
                    ..CardFields::default()
                },
            },
        )
        .unwrap()
        .id;
        let agent = agent_store::create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: AgentFields {
                    name: "builder-1".into(),
                    harness,
                    command: String::new(),
                    model: Some("deepseek/flash".into()),
                    env: String::new(),
                },
            },
        )
        .unwrap();
        agent_store::set_card(&db, agent.id, Some(card), false, None).unwrap();
        let agent = agent_store::get_agent(&db, agent.id).unwrap().unwrap();
        let roots = ChannelRoots {
            events: dir.join("events"),
            claude_tasks: dir.join("tasks"),
            claude_plans: dir.join("plans"),
        };
        let channel = Channel::for_agent(&roots, agent.id);
        channel.reset().unwrap();
        World {
            watched: Watched {
                role_name: "builder".into(),
                limits: limits(),
                billing: Billing::Metered,
                model: agent.model.clone(),
                channel,
                wanted: true,
                agent,
            },
            dir,
            db,
            card,
        }
    }

    fn reply(id: &str, input: u64, output: u64, tool: &str) -> String {
        json!({"type": "assistant", "message": {
            "id": id,
            "usage": {"input_tokens": input, "output_tokens": output},
            "content": [{"type": "tool_use", "id": tool}],
        }})
        .to_string()
    }

    #[test]
    fn the_stop_file_keeps_the_reason_apart_from_the_instruction_to_the_model() {
        let text = marker_text("60 steps of 60 used");
        assert!(text.contains("Do not call any more tools"));
        assert_eq!(reason_of(&text), "60 steps of 60 used");
        assert_eq!(reason_of("something else"), "something else");
    }

    #[test]
    fn what_a_look_leads_to() {
        let hit = || Some("steps".to_owned());
        assert_eq!(decide(hit(), false, true), Step::Stop("steps".into()));
        assert_eq!(decide(hit(), true, true), Step::KeepStopped);
        assert_eq!(decide(None, true, true), Step::Lift);
        assert_eq!(decide(None, false, true), Step::Leave);
        // No reading is not a finding: a stop is not lifted on one.
        assert_eq!(decide(None, true, false), Step::Leave);
    }

    #[test]
    fn a_claude_session_is_counted_from_its_transcripts_across_restarts_and_a_look_reads_only_the_new_part()
     {
        let world = world(Harness::ClaudeCode);
        let projects = world.dir.join("projects");
        let folder = projects.join("-worktree");
        std::fs::create_dir_all(&folder).unwrap();
        let first = axiomata_ide::usage::new_claude_session_id().unwrap();
        let second = axiomata_ide::usage::new_claude_session_id().unwrap();
        world.watched.channel.record_claude_session(&first).unwrap();
        std::fs::write(
            folder.join(format!("{first}.jsonl")),
            format!("{}\n", reply("m1", 100, 10, "t1")),
        )
        .unwrap();
        let mut meter = Meter::with_projects(projects);
        let config = Config::default();
        let mut seen = Vec::new();
        let read = futures_block(meter.read(&world.watched, &config, &mut seen));
        assert!(read.measured);
        assert_eq!((read.usage.tokens(), read.usage.steps), (110, 1));

        // The pane was restarted: a second transcript, counted on top of the first.
        world
            .watched
            .channel
            .record_claude_session(&second)
            .unwrap();
        std::fs::write(
            folder.join(format!("{second}.jsonl")),
            format!("{}\n", reply("m2", 200, 20, "t2")),
        )
        .unwrap();
        let read = futures_block(meter.read(&world.watched, &config, &mut seen));
        assert_eq!((read.usage.tokens(), read.usage.steps), (330, 2));
    }

    #[test]
    fn a_session_whose_transcript_is_not_there_is_unmeasured_not_free() {
        let world = world(Harness::ClaudeCode);
        let mut meter = Meter::with_projects(world.dir.join("projects"));
        let id = axiomata_ide::usage::new_claude_session_id().unwrap();
        world.watched.channel.record_claude_session(&id).unwrap();
        let read = futures_block(meter.read(&world.watched, &Config::default(), &mut Vec::new()));
        assert!(!read.measured);
        assert_eq!(read.usage, Usage::default());
        assert_eq!(
            read.unmeasured,
            Some(Unmeasured::NoRecordYet),
            "a session that has just started has written nothing yet"
        );
    }

    #[test]
    fn the_reason_a_session_is_not_measured_is_told_apart() {
        let world = world(Harness::ClaudeCode);
        let mut meter = Meter::with_projects(world.dir.join("projects"));
        let config = Config::default();
        // No id was ever noted for it.
        let none = futures_block(meter.read(&world.watched, &config, &mut Vec::new()));
        assert_eq!(none.unmeasured, Some(Unmeasured::NoSessionId));

        // A record that cannot be read: a link is not a transcript.
        let id = axiomata_ide::usage::new_claude_session_id().unwrap();
        world.watched.channel.record_claude_session(&id).unwrap();
        let folder = world.dir.join("projects").join("-w");
        std::fs::create_dir_all(&folder).unwrap();
        let target = world.dir.join("target.jsonl");
        std::fs::write(&target, "").unwrap();
        // The link is found, and refused when it is read.
        std::os::unix::fs::symlink(&target, folder.join(format!("{id}.jsonl"))).unwrap();
        let linked = futures_block(meter.read(&world.watched, &config, &mut Vec::new()));
        assert_eq!(linked.unmeasured, Some(Unmeasured::ReadFailed));

        let opencode = self::world(Harness::Opencode);
        let mut meter = Meter::with_projects(opencode.dir.join("projects"));
        let unnamed = futures_block(meter.read(&opencode.watched, &config, &mut Vec::new()));
        assert_eq!(unnamed.unmeasured, Some(Unmeasured::NoOpencodeSession));
    }

    #[test]
    fn money_is_metered_from_the_owners_price_table_for_an_engine_paid_per_token_only() {
        let mut world = world(Harness::ClaudeCode);
        let projects = world.dir.join("projects");
        let folder = projects.join("-worktree");
        std::fs::create_dir_all(&folder).unwrap();
        let id = axiomata_ide::usage::new_claude_session_id().unwrap();
        world.watched.channel.record_claude_session(&id).unwrap();
        std::fs::write(
            folder.join(format!("{id}.jsonl")),
            format!("{}\n", reply("m1", 1_000_000, 1_000_000, "t1")),
        )
        .unwrap();
        let mut config = Config::default();
        config.agents.costs.insert(
            "deepseek/flash".into(),
            crate::config::ModelCost {
                input_per_m: 0.5,
                output_per_m: 1.5,
            },
        );
        let mut meter = Meter::with_projects(projects);
        let metered = futures_block(meter.read(&world.watched, &config, &mut Vec::new()));
        assert_eq!(metered.cost_usd, Some(2.0));
        world.watched.billing = Billing::Subscription;
        let flat = futures_block(meter.read(&world.watched, &config, &mut Vec::new()));
        assert_eq!(
            flat.cost_usd, None,
            "a subscription has no dollar figure to make up"
        );
    }

    #[test]
    fn stopping_writes_the_marker_and_the_history_and_a_second_look_finds_it_stopped() {
        let world = world(Harness::ClaudeCode);
        let breach = stop(
            &world.db,
            &Config::default(),
            &world.watched,
            "60 steps of 60 used",
        )
        .unwrap();
        assert_eq!(breach.card_id, Some(world.card));
        assert_eq!(breach.agent_name, "builder-1");
        let marker = world.watched.channel.limit_reached().unwrap();
        assert!(
            marker.contains("60 steps of 60 used") && marker.contains("Do not call any more tools")
        );
        let events = flow::list_events(&world.db, world.card, 50).unwrap();
        let event = events
            .iter()
            .find(|event| event.kind == EventKind::LimitStop)
            .expect("the stop is in the card's history");
        assert!(
            event
                .text
                .contains("raise the limit of the role \"builder\""),
            "{}",
            event.text
        );
        assert!(event.actor.starts_with("agent:"), "{}", event.actor);
        assert_eq!(
            decide(
                Some("again".into()),
                world.watched.channel.limit_reached().is_some(),
                true
            ),
            Step::KeepStopped
        );
    }

    #[test]
    fn a_planner_has_no_card_to_write_the_stop_on_but_is_stopped_and_reported_with_its_plan() {
        let mut world = world(Harness::ClaudeCode);
        let id = world.watched.agent.id;
        agent_store::set_card(&world.db, id, None, false, None).unwrap();
        agent_store::set_plan(&world.db, id, Some(5), None).unwrap();
        world.watched.agent = agent_store::get_agent(&world.db, id).unwrap().unwrap();
        let watched = &world.watched;

        let breach = stop(
            &world.db,
            &Config::default(),
            watched,
            "40 steps of 40 used",
        )
        .unwrap();
        assert_eq!((breach.card_id, breach.plan_id), (None, Some(5)));
        assert!(
            watched.channel.limit_reached().is_some(),
            "the session is stopped all the same"
        );
        assert!(
            flow::list_events(&world.db, world.card, 50)
                .unwrap()
                .is_empty(),
            "nothing is written on a card that is not its own"
        );
    }

    /// Runs a future to its end on a throwaway runtime; the tests above are not async because the code under test
    /// reaches the Opencode service only for an Opencode session.
    fn futures_block<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }
}
