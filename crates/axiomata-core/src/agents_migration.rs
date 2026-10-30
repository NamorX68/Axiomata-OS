//! The one-time move from roles that carry engines to roles at tiers (`docs/plans/agents-model.md`, AM6, Q14, Q9, Q13).
//!
//! Before AM1 a role was its job *and* its engine, so the owner's folder grew copies — `planner-haiku`,
//! `reviewer-heavy`, `allrounder-glut` — and the config held engines that are now automatic. This pass:
//!
//! * **roles** (`~/.axiomata/agents/`): makes sure the seven base roles exist (from the bundled text, at the variants'
//!   most common tier); every variant `<base>-<suffix>` is moved into a backup folder — it was a copy of its base for
//!   another engine, and its strength is now the tier its agent runs at (owner, 2026-10-30, after the first live pass
//!   kept 13 that differed by a line or two); the roles that stay lose `engine`/`fallback_engines` — their original
//!   files are copied into the backup first;
//! * **engines**: an own engine that is just an automatic one (same harness and model, no command, no environment, no
//!   tier or billing of its own) is removed from the config — its model switched on if it was off — and its id mapped
//!   to the automatic one's; the removed entries are written to the backup as `engines.toml`;
//! * **default engine per tier**, for every tier the owner set none for: the engine the old roles of that tier used
//!   most; a default or a favorite that named a removed engine follows it to the automatic one;
//! * **sessions**: a session of a moved variant plays its base; one on a removed engine — a master's too — runs on the
//!   automatic one. Names, worktrees and branches stay.
//!
//! Nothing is deleted: each pass's backup folder (`~/.axiomata/agents-backup-<date>[-n]/`, owner-only, never
//! overwritten) stays until the owner removes it in Settings → Storage. Project roles in repositories are never
//! touched. The pass works out its whole [`Plan`] first, stores it as "in progress", then applies it — backups, the
//! sessions in one transaction, the config in one save, the role files last — and only then records itself as done; a
//! pass cut off goes on from the stored plan. It runs once at the app's start ([`run_once`]);
//! `axiomata-cli ide migrate-agents [--dry-run]` runs it on the owner's word. One pass at a time, across processes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axiomata_roster::{Billing, Engine, Harness, Loaded, ROLE_FILE, Role, Source, Tier};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::agent_start::Favorite;
use crate::config::{self, Config, ProviderId};
use crate::engines::{self, EngineOrigin};
use crate::error::AxiomataError;
use crate::ide::agent_store;
use crate::model_tiers::{self, ModelChoice, ModelSources};
use crate::roster::refusal;
use crate::{json_state, paths};

type Result<T> = std::result::Result<T, AxiomataError>;

/// The `app_meta` key that says the pass ran (RFC 3339 of when).
pub const DONE_KEY: &str = "agents_model.migrated";

/// The prefix of a backup folder's name in `~/.axiomata`.
pub const BACKUP_PREFIX: &str = "agents-backup-";

/// On-disk schema version of `agents-migration.json`.
const STATE_VERSION: u64 = 1;

/// How long the app's start waits for a pass another process runs before it skips the pass for this start.
pub const START_LOCK_WAIT: Duration = Duration::from_secs(3);

/// How often a bounded wait for the lock looks again.
const LOCK_POLL: Duration = Duration::from_millis(50);

/// A variant an earlier pass kept as an own role because its text differed from its base's; [`fold_leftovers`] folds it
/// at the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptVariant {
    pub name: String,
    pub base: String,
}

/// What the pass did — or, in a dry run, would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub dry_run: bool,
    /// Where the originals went; `None` when nothing needed a backup.
    pub backup: Option<PathBuf>,
    /// Base roles that were missing and were made.
    pub created: Vec<String>,
    /// Variants equal to their base, moved into the backup.
    pub moved: Vec<String>,
    /// Variants an earlier pass kept as own roles, not folded yet; a pass today keeps none.
    pub kept: Vec<KeptVariant>,
    /// Roles whose `engine`/`fallback_engines` were dropped (their originals are in the backup).
    pub stripped: Vec<String>,
    /// Own engines that were automatic ones: old id → the automatic id.
    pub engines_mapped: BTreeMap<String, String>,
    /// Models switched on so their automatic engine exists for an own engine that ran them.
    pub models_switched_on: Vec<String>,
    /// The default engine seeded per tier (`light`/`medium`/`heavy` → engine id).
    pub defaults_seeded: BTreeMap<String, String>,
    /// Sessions whose role or engine changed.
    pub sessions_remapped: usize,
    /// What the pass left alone and why: role files that could not be read (a base whose place holds one is not
    /// made), a role or an engine that changed while a pass cut off waited.
    pub skipped: Vec<String>,
}

impl Report {
    /// Whether the pass found nothing to do.
    pub fn is_empty(&self) -> bool {
        self.created.is_empty()
            && self.moved.is_empty()
            && self.stripped.is_empty()
            && self.engines_mapped.is_empty()
            && self.models_switched_on.is_empty()
            && self.defaults_seeded.is_empty()
            && self.sessions_remapped == 0
    }

    /// Whether there is nothing to tell the owner either: no change and no role left for the owner to look at.
    fn says_nothing(&self) -> bool {
        self.is_empty() && self.kept.is_empty() && self.skipped.is_empty()
    }
}

/// The app's memory of the pass (`~/.axiomata/agents-migration.json`): its report, whether the owner saw the notice,
/// and the kept variants already folded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub version: u64,
    #[serde(default)]
    pub report: Option<Report>,
    #[serde(default)]
    pub notice_seen: bool,
    /// Kept variants already folded (or, before 2026-10-30, kept by the owner's word).
    #[serde(default)]
    pub decided: BTreeSet<String>,
    /// The plan of a pass that started and has not finished: the next run goes on with it.
    #[serde(default)]
    pub in_progress: Option<Plan>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            report: None,
            notice_seen: false,
            decided: BTreeSet::new(),
            in_progress: None,
        }
    }
}

/// The stored state; the default when there is none.
///
/// # Errors
///
/// A state file this build cannot read — of another version, or not of this shape. It is never replaced: the pass and
/// the fold of kept variants and the dismiss refuse until a build that knows it runs.
pub fn read_state() -> Result<State> {
    let path = paths::agents_migration_path();
    let loaded = json_state::load(&path, || {
        serde_json::to_string(&State::default()).unwrap_or_default()
    })?;
    let unknown = |reason: String| {
        refusal(
            "migration state",
            format!(
                "{} {reason}; the agents migration does not run and the file is kept as it is",
                path.display()
            ),
        )
    };
    let value: serde_json::Value =
        serde_json::from_str(&loaded.json).map_err(|err| unknown(err.to_string()))?;
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(STATE_VERSION) => {}
        Some(version) => {
            return Err(unknown(format!(
                "is of version {version}, this build knows {STATE_VERSION}"
            )));
        }
        None => return Err(unknown("names no version".to_owned())),
    }
    serde_json::from_value(value).map_err(|err| unknown(err.to_string()))
}

/// The stored state for a view that only reads; the default when there is none or it cannot be read.
pub fn load_state() -> State {
    read_state().unwrap_or_default()
}

fn save_state(state: &State) -> Result<()> {
    let json =
        serde_json::to_string(state).map_err(|err| AxiomataError::InvalidDashboardState {
            path: paths::agents_migration_path(),
            reason: err.to_string(),
        })?;
    json_state::save(&paths::agents_migration_path(), &json)
}

/// The base a role name is a variant of (`planner-haiku` → `planner`), if it is one.
fn base_of(name: &str) -> Option<&'static str> {
    BASES.iter().copied().find(|base| {
        name.len() > base.len() + 1 && name.starts_with(base) && name.as_bytes()[base.len()] == b'-'
    })
}

/// The seven base roles' names (Q5).
const BASES: [&str; 7] = [
    "allrounder",
    "implementer",
    "documenter",
    "planner",
    "reviewer",
    "tester",
    "grill",
];

/// The most common of `items`; ties go to the first one met.
fn most_common<T: Clone + Eq>(items: impl IntoIterator<Item = T>) -> Option<T> {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for item in items {
        match counts.iter_mut().find(|(seen, _)| *seen == item) {
            Some((_, n)) => *n += 1,
            None => counts.push((item, 1)),
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max()?;
    counts
        .into_iter()
        .find(|(_, n)| *n == best)
        .map(|(item, _)| item)
}

/// SHA-256 of `bytes`, lower-case hex.
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The hash of the file at `path`; `None` when it cannot be read.
fn file_hash(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|bytes| hash(&bytes))
}

/// `favorites` with each one only once, the first kept.
fn unique(favorites: &mut Vec<Favorite>) {
    let mut seen: Vec<Favorite> = Vec::new();
    favorites.retain(|favorite| {
        let first = !seen.contains(favorite);
        if first {
            seen.push(favorite.clone());
        }
        first
    });
}

/// The provider and the provider's id of the model an engine runs, when it is one the catalog could offer.
fn catalog_model(engine: &Engine) -> Option<(ProviderId, String)> {
    let model = engine.model.as_deref()?.trim();
    match engine.harness {
        Harness::ClaudeCode if !model.is_empty() => Some((ProviderId::Anthropic, model.to_owned())),
        Harness::Glut => model_tiers::parse_model_key(model)
            .filter(|(provider, _)| *provider != ProviderId::Anthropic)
            .map(|(provider, id)| (provider, id.to_owned())),
        _ => None,
    }
}

/// What the pass will change, worked out before anything is written and stored as "in progress" before the first
/// change: a pass that was cut off goes on from this plan, never from what it left on disk.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub report: Report,
    /// This run's backup folder (a direct child of `~/.axiomata`).
    pub backup: PathBuf,
    /// Bases to write.
    pub create: Vec<Role>,
    /// Roles that stay and are rewritten without their engines (their originals copied into the backup first).
    pub strip: Vec<Role>,
    /// The SHA-256 of each stripped role's file as the plan saw it: a file that differs is never rewritten.
    pub originals: BTreeMap<String, String>,
    /// Own engines to remove, with their entries for the backup.
    pub removed_engines: BTreeMap<String, Engine>,
    /// Model keys to switch on.
    pub switch_on: Vec<String>,
    /// Default engines to seed for tiers the owner set none for.
    pub defaults: Vec<(Tier, String)>,
    /// Session id → (new role, new engine id).
    pub sessions: Vec<SessionChange>,
}

/// The steps of [`apply`], in their order; a test stops after one to check that a rerun ends the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    /// Copies of what changes: `engines.toml`, the originals of the roles that are rewritten. Nothing changes yet.
    Backups,
    /// The sessions' roles and engines, in one transaction.
    Database,
    /// Engines, switched-on models, default engines and favorites, in one save of the file config.
    Config,
    /// The role files: rewritten, made, moved into the backup.
    Files,
}

/// Whether an engine is nothing but its model on its harness: no command, no environment, no tier of the owner's, the
/// harness's own billing.
fn plain(engine: &Engine) -> bool {
    let billing = if engine.harness == Harness::ClaudeCode {
        Billing::Subscription
    } else {
        Billing::Metered
    };
    engine.command.trim().is_empty()
        && engine.env.trim().is_empty()
        && engine.tier.is_none()
        && engine.billing == billing
}

/// A backup folder for a run of `today` in `home` no earlier run took: `agents-backup-<date>`, then `-2`, `-3`, ….
fn fresh_backup_dir(home: &Path, today: NaiveDate) -> PathBuf {
    let base = format!("{BACKUP_PREFIX}{}", today.format("%Y-%m-%d"));
    std::iter::once(home.join(&base))
        .chain((2..).map(|n| home.join(format!("{base}-{n}"))))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| home.join(base))
}

/// Whether `path` is a backup folder's place: a direct child of `home` whose name has [`BACKUP_PREFIX`].
fn is_backup_place(home: &Path, path: &Path) -> bool {
    path.parent() == Some(home)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(BACKUP_PREFIX) && !name.contains(['/', '\\']))
}

/// Works out what the pass changes for `roles` (the owner's), the file config `disk`, `sources` and the sessions in
/// `db`.
fn plan(
    db: &Connection,
    disk: &Config,
    sources: &ModelSources,
    loaded: &Loaded,
    backup: PathBuf,
) -> Result<Plan> {
    let roles = &loaded.roles;
    let mut plan = Plan {
        backup,
        ..Plan::default()
    };
    let dir = paths::agent_roles_dir();
    let by_name: HashMap<&str, &Role> = roles
        .iter()
        .map(|role| (role.name.as_str(), role))
        .collect();
    plan.report.skipped = loaded
        .skipped
        .iter()
        .map(|s| format!("{}: {}", s.name, s.reason))
        .collect();

    // Roles: bases, variants.
    for bundled in axiomata_roster::base_roles() {
        let variants: Vec<&Role> = roles
            .iter()
            .filter(|r| base_of(&r.name) == Some(bundled.name.as_str()))
            .collect();
        if !by_name.contains_key(bundled.name.as_str()) {
            // Something is there that is no readable role (a link, a broken file): it is said once, never replaced.
            if fs::symlink_metadata(dir.join(&bundled.name)).is_ok() {
                continue;
            }
            let mut made = bundled.clone();
            if let Some(tier) = most_common(variants.iter().map(|v| v.tier)) {
                made.tier = tier;
            }
            plan.report.created.push(made.name.clone());
            plan.create.push(made);
        }
        // A variant was a copy of its base for another engine; its strength is now the tier its agent runs at. What its
        // text said beyond the base's stays in the backup.
        plan.report
            .moved
            .extend(variants.iter().map(|variant| variant.name.clone()));
    }
    let moved: BTreeSet<&str> = plan.report.moved.iter().map(String::as_str).collect();
    for role in roles {
        if !moved.contains(role.name.as_str())
            && (role.engine.is_some() || !role.fallback_engines.is_empty())
        {
            // Without its original read there is no backup copy, so the role keeps its file as it is.
            let Some(original) = file_hash(&dir.join(&role.name).join(ROLE_FILE)) else {
                plan.report.skipped.push(format!(
                    "{}: its file could not be read, so it keeps its engine",
                    role.name
                ));
                continue;
            };
            plan.originals.insert(role.name.clone(), original);
            plan.report.stripped.push(role.name.clone());
            let mut stripped = role.clone();
            stripped.engine = None;
            stripped.fallback_engines.clear();
            plan.strip.push(stripped);
        }
    }

    // Engines: an own engine that is an automatic one goes, its model switched on.
    let mut next = disk.clone();
    for engine in disk.agents.engines.values().filter(|engine| plain(engine)) {
        let Some((provider, id)) = catalog_model(engine) else {
            continue;
        };
        if !model_tiers::offered(&sources.catalog, provider)
            .iter()
            .any(|m| m.id == id)
        {
            continue;
        }
        let key = model_tiers::model_key(provider, &id);
        if !model_tiers::enabled_models(&next, &sources.catalog, provider)
            .iter()
            .any(|m| m.id == id)
        {
            next.agents
                .models
                .choices
                .entry(key.clone())
                .or_default()
                .enabled = Some(true);
            plan.switch_on.push(key);
        }
        next.agents.engines.remove(&engine.id);
        plan.removed_engines
            .insert(engine.id.clone(), engine.clone());
    }
    let autos: Vec<Engine> = engines::all(&next, sources)
        .into_iter()
        .filter(|info| info.origin == EngineOrigin::Auto)
        .map(|info| info.engine)
        .collect();
    let mut mapping: BTreeMap<String, String> = BTreeMap::new();
    for (old, engine) in &plan.removed_engines {
        if let Some(auto) = autos
            .iter()
            .find(|a| a.harness == engine.harness && a.model == engine.model)
        {
            mapping.insert(old.clone(), auto.id.clone());
        }
    }
    // An own engine whose automatic one did not come about after all stays where it is.
    plan.removed_engines
        .retain(|old, _| mapping.contains_key(old));
    let kept_models: Vec<String> = plan
        .removed_engines
        .values()
        .filter_map(catalog_model)
        .map(|(provider, id)| model_tiers::model_key(provider, &id))
        .collect();
    plan.switch_on.retain(|key| kept_models.contains(key));
    plan.report.engines_mapped = mapping.clone();
    plan.report.models_switched_on = plan.switch_on.clone();
    let resolve = |id: &str| mapping.get(id).cloned().unwrap_or_else(|| id.to_owned());

    // The default engine of every tier the owner set none for: what the old roles of that tier used most.
    let lookup = engines::Lookup::with_sources(&next, sources.clone());
    for tier in Tier::ALL {
        if disk.agents.models.default_engines.get(tier).is_some() {
            continue;
        }
        let used = roles
            .iter()
            .filter(|role| role.tier == tier)
            .filter_map(|role| role.engine.as_deref())
            .map(resolve)
            .filter(|id| lookup.exists(id));
        if let Some(id) = most_common(used) {
            plan.report
                .defaults_seeded
                .insert(tier.as_str().to_owned(), id.clone());
            plan.defaults.push((tier, id));
        }
    }

    plan.sessions = sessions_for(db, &moved, &mapping)?;
    plan.report.sessions_remapped = plan.sessions.len();
    plan.report.backup = (plan.needs_backup()).then(|| plan.backup.clone());
    Ok(plan)
}

/// The sessions to change, from their state now: a moved variant's plays its base; any session on a removed engine —
/// masters too — runs on the automatic one.
fn sessions_for(
    db: &Connection,
    moved: &BTreeSet<&str>,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<SessionChange>> {
    let mut out = Vec::new();
    for project in crate::ide::store::list_projects(db)? {
        for agent in agent_store::list_agents(db, project.id)? {
            let role = (!agent.is_master() && moved.contains(agent.agent_role.as_str()))
                .then(|| base_of(&agent.agent_role).map(str::to_owned))
                .flatten();
            let engine = agent
                .engine_id
                .as_deref()
                .and_then(|id| mapping.get(id))
                .filter(|new| agent.engine_id.as_deref() != Some(new.as_str()))
                .cloned();
            if role.is_some() || engine.is_some() {
                out.push((agent.id, role, engine));
            }
        }
    }
    Ok(out)
}

/// A session's id with its new role and its new engine id, each only when it changes.
type SessionChange = (i64, Option<String>, Option<String>);

/// A stored plan checked against what is there now, before a pass cut off goes on with it: a role whose file changed
/// since (other than by the pass's own rewrite) keeps it, an own engine whose definition changed stays, and the
/// sessions are worked out again — a session that plays another role or runs another engine by now is left alone, one
/// made on a removed engine since is moved along too.
fn refresh(db: &Connection, disk: &Config, mut plan: Plan) -> Result<Plan> {
    let dir = paths::agent_roles_dir();
    let mut skipped = Vec::new();
    let strip = std::mem::take(&mut plan.strip);
    for role in strip {
        let planned = plan.originals.get(&role.name).cloned();
        let current = file_hash(&dir.join(&role.name).join(ROLE_FILE));
        let copied = file_hash(&plan.backup.join(&role.name).join(ROLE_FILE));
        // Still the original (the pass will rewrite it), or rewritten by an earlier try beside its copy.
        let rewritten = current.as_deref() == Some(hash(role.to_markdown().as_bytes()).as_str());
        if planned.is_some() && (current == planned || (rewritten && copied == planned)) {
            plan.strip.push(role);
        } else {
            skipped.push(format!(
                "{}: its file changed since the pass began, so it stays as it is",
                role.name
            ));
            plan.report.stripped.retain(|name| *name != role.name);
            plan.originals.remove(&role.name);
        }
    }
    let changed: Vec<String> = plan
        .removed_engines
        .iter()
        .filter(|(id, planned)| {
            disk.agents
                .engines
                .get(*id)
                .is_some_and(|now| now != *planned)
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in &changed {
        skipped.push(format!(
            "engine {id}: it changed since the pass began, so it stays"
        ));
        plan.removed_engines.remove(id);
        plan.report.engines_mapped.remove(id);
    }
    let kept_models: Vec<String> = plan
        .removed_engines
        .values()
        .filter_map(catalog_model)
        .map(|(provider, id)| model_tiers::model_key(provider, &id))
        .collect();
    plan.switch_on.retain(|key| kept_models.contains(key));
    plan.report.models_switched_on = plan.switch_on.clone();
    let moved: BTreeSet<&str> = plan.report.moved.iter().map(String::as_str).collect();
    let sessions = sessions_for(db, &moved, &plan.report.engines_mapped)?;
    let counted: BTreeSet<i64> = plan
        .sessions
        .iter()
        .chain(&sessions)
        .map(|(id, _, _)| *id)
        .collect();
    plan.report.sessions_remapped = counted.len();
    plan.sessions = sessions;
    for line in skipped {
        if !plan.report.skipped.contains(&line) {
            plan.report.skipped.push(line);
        }
    }
    Ok(plan)
}

impl Plan {
    fn needs_backup(&self) -> bool {
        !self.report.moved.is_empty() || !self.strip.is_empty() || !self.removed_engines.is_empty()
    }
}

/// Creates `dir` for the backup, owner-only, refusing a link at its place.
fn ensure_private_dir(dir: &Path) -> Result<()> {
    let io = |source| AxiomataError::Io {
        path: dir.to_path_buf(),
        source,
    };
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(refusal(
            "backup",
            format!("{} is no folder of its own", dir.display()),
        )),
        Err(_) => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(PRIVATE_DIR_MODE);
            }
            builder.create(dir).map_err(io)
        }
    }
}

/// Owner-only, for a backup's folders.
const PRIVATE_DIR_MODE: u32 = 0o700;
/// Owner-only, for a backup's files.
const PRIVATE_FILE_MODE: u32 = 0o600;

/// Writes `bytes` to `path` as a new owner-only file; one that is there already (an earlier try of the same run) is
/// left as it is — a backup never overwrites.
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    match options.open(path) {
        Ok(mut file) => file.write_all(bytes).map_err(|source| AxiomataError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(AxiomataError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Moves role folder `name` from `dir` into `backup` — done already when it is in the backup and gone from `dir`.
fn move_role(dir: &Path, backup: &Path, name: &str) -> Result<()> {
    let from = dir.join(name);
    if fs::symlink_metadata(&from).is_err() {
        return Ok(());
    }
    ensure_private_dir(backup)?;
    let target = std::iter::once(backup.join(name))
        .chain((2..).map(|n| backup.join(format!("{name}-{n}"))))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| backup.join(name));
    fs::rename(&from, &target).map_err(|source| AxiomataError::Io { path: from, source })
}

/// Applies `plan` step by step, each step safe to run again; with `stop_after` a test stops after that step.
fn apply(
    db: &Connection,
    config: &mut Config,
    plan: &Plan,
    stop_after: Option<Step>,
) -> Result<()> {
    let stop = |step: Step| -> Result<()> {
        if stop_after == Some(step) {
            return Err(refusal("migration", format!("stopped after {step:?}")));
        }
        Ok(())
    };
    let dir = paths::agent_roles_dir();

    // Backups first: nothing changes yet.
    if plan.needs_backup() {
        ensure_private_dir(&plan.backup)?;
    }
    if !plan.removed_engines.is_empty() {
        let toml = toml::to_string_pretty(&plan.removed_engines)
            .map_err(|source| AxiomataError::ConfigSerialize { source })?;
        write_new(&plan.backup.join("engines.toml"), toml.as_bytes())?;
    }
    for role in &plan.strip {
        // Only the original the plan saw is copied: a file that cannot be read, or that changed (or was rewritten by an
        // earlier try, whose copy is there), is not — and the files step rewrites a role only beside its copy.
        let Some(planned) = plan.originals.get(&role.name) else {
            continue;
        };
        let Ok(text) = fs::read(dir.join(&role.name).join(ROLE_FILE)) else {
            continue;
        };
        if hash(&text) != *planned {
            continue;
        }
        let copy_dir = plan.backup.join(&role.name);
        ensure_private_dir(&copy_dir)?;
        write_new(&copy_dir.join(ROLE_FILE), &text)?;
    }
    stop(Step::Backups)?;

    // The sessions, in one transaction.
    let tx = db.unchecked_transaction()?;
    for (id, role, engine) in &plan.sessions {
        if let Some(role) = role {
            agent_store::set_role(&tx, *id, role)?;
        }
        if let Some(engine) = engine {
            agent_store::set_engine(&tx, *id, Some(engine))?;
        }
    }
    tx.commit()?;
    stop(Step::Database)?;

    // The file config, in one save under its lock: engines, models, defaults and favorites.
    {
        let _guard = config::file_edit_lock();
        let mut disk = Config::load()?;
        // An engine is removed only as the plan saw it; one changed since stays, and nothing is remapped away from it.
        for (id, planned) in &plan.removed_engines {
            if disk.agents.engines.get(id) == Some(planned) {
                disk.agents.engines.remove(id);
            }
        }
        let mapping: BTreeMap<String, String> = plan
            .report
            .engines_mapped
            .iter()
            .filter(|(old, _)| !disk.agents.engines.contains_key(*old))
            .map(|(old, new)| (old.clone(), new.clone()))
            .collect();
        for key in &plan.switch_on {
            disk.agents
                .models
                .choices
                .entry(key.clone())
                .or_insert_with(ModelChoice::default)
                .enabled = Some(true);
        }
        for tier in Tier::ALL {
            let current = disk
                .agents
                .models
                .default_engines
                .get(tier)
                .map(str::to_owned);
            match current {
                Some(id) => {
                    if let Some(new) = mapping.get(&id) {
                        disk.agents
                            .models
                            .default_engines
                            .set(tier, Some(new.clone()));
                    }
                }
                None => {
                    if let Some((_, id)) = plan.defaults.iter().find(|(t, _)| *t == tier) {
                        disk.agents
                            .models
                            .default_engines
                            .set(tier, Some(id.clone()));
                    }
                }
            }
        }
        for favorite in &mut disk.agents.favorites {
            if let Some(new) = favorite.engine.as_ref().and_then(|id| mapping.get(id)) {
                favorite.engine = Some(new.clone());
            }
        }
        unique(&mut disk.agents.favorites);
        disk.validate_for_save()
            .map_err(|reason| refusal("config", reason))?;
        disk.save()?;
        config.agents.engines = disk.agents.engines.clone();
        config.agents.models = disk.agents.models.clone();
        config.agents.favorites = disk.agents.favorites.clone();
    }
    stop(Step::Config)?;

    // The role files last; a role is rewritten only while its file is the plan's original and its copy is in the
    // backup.
    for role in &plan.strip {
        let Some(planned) = plan.originals.get(&role.name) else {
            continue;
        };
        let current = file_hash(&dir.join(&role.name).join(ROLE_FILE));
        let copied = file_hash(&plan.backup.join(&role.name).join(ROLE_FILE));
        if current.as_ref() == Some(planned) && copied.as_ref() == Some(planned) {
            axiomata_roster::save_role(&dir, role)?;
        }
    }
    for role in &plan.create {
        if fs::symlink_metadata(dir.join(&role.name)).is_err() {
            axiomata_roster::save_role(&dir, role)?;
        }
    }
    for name in &plan.report.moved {
        move_role(&dir, &plan.backup, name)?;
    }
    stop(Step::Files)?;
    Ok(())
}

/// Serialises the pass in this process; [`MigrationLock`] does it across processes (the app and the CLI).
static MIGRATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The migration's lock across processes: an advisory lock on `agents-migration.lock`, held for a whole pass (and for
/// the fold of a kept variant), taken while this process holds [`MIGRATION_LOCK`].
struct MigrationLock {
    _thread: std::sync::MutexGuard<'static, ()>,
    file: fs::File,
}

impl MigrationLock {
    /// Takes the lock — waiting for it, or with `wait` at most that long; then a refusal says another pass runs.
    fn take(wait: Option<Duration>) -> Result<Self> {
        let deadline = wait.map(|wait| Instant::now() + wait);
        let busy = || {
            refusal(
                "migration",
                "another Axiomata process is running the agents migration; it is left for now"
                    .to_owned(),
            )
        };
        let thread = match deadline {
            None => MIGRATION_LOCK
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
            Some(deadline) => loop {
                match MIGRATION_LOCK.try_lock() {
                    Ok(guard) => break guard,
                    Err(std::sync::TryLockError::Poisoned(poison)) => break poison.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                        std::thread::sleep(LOCK_POLL);
                    }
                    Err(std::sync::TryLockError::WouldBlock) => return Err(busy()),
                }
            },
        };
        let path = paths::agents_migration_lock_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let io = |source| AxiomataError::Io {
            path: path.clone(),
            source,
        };
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io)?;
        match deadline {
            None => file.lock().map_err(io)?,
            Some(deadline) => loop {
                match file.try_lock() {
                    Ok(()) => break,
                    Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                        std::thread::sleep(LOCK_POLL);
                    }
                    Err(fs::TryLockError::WouldBlock) => return Err(busy()),
                    Err(fs::TryLockError::Error(source)) => return Err(io(source)),
                }
            },
        }
        Ok(Self {
            _thread: thread,
            file,
        })
    }
}

impl Drop for MigrationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Runs the pass: works out what changes and, unless `dry_run`, does it — or goes on with a pass that was cut off.
/// `config` (the live one) takes the file config's engines, models and favorites afterwards. A real run records itself
/// as done (`app_meta`) and keeps its report for the notice, the app's run and the CLI's alike.
///
/// # Errors
///
/// A failing read of the roles, the config or the database; a failing write. A pass that fails half-way stays "in
/// progress" and the next run goes on with the same plan.
pub fn run(
    db: &Connection,
    config: &mut Config,
    dry_run: bool,
    today: NaiveDate,
) -> Result<Report> {
    run_with(db, config, dry_run, today, None, None)
}

fn run_with(
    db: &Connection,
    config: &mut Config,
    dry_run: bool,
    today: NaiveDate,
    stop_after: Option<Step>,
    wait: Option<Duration>,
) -> Result<Report> {
    let _lock = MigrationLock::take(wait)?;
    let mut state = read_state()?;
    let home = paths::axiomata_home();
    let plan = match state
        .in_progress
        .clone()
        .filter(|plan| is_backup_place(&home, &plan.backup))
    {
        Some(plan) => refresh(db, &Config::load()?, plan)?,
        None => {
            let dir = paths::agent_roles_dir();
            let loaded = if dir.exists() {
                axiomata_roster::load_roles(&dir, Source::User)?
            } else {
                axiomata_roster::Loaded::default()
            };
            let disk = Config::load()?;
            plan(
                db,
                &disk,
                &ModelSources::load(),
                &loaded,
                fresh_backup_dir(&home, today),
            )?
        }
    };
    if dry_run {
        return Ok(Report {
            dry_run: true,
            ..plan.report
        });
    }
    // Nothing to change: the last pass's record stays, and only a role left for the owner that it does not name yet is
    // added to it (and shown).
    if plan.report.is_empty() && state.in_progress.is_none() {
        if remember(&mut state, &plan.report) {
            save_state(&state)?;
        }
        record_done(db)?;
        return Ok(plan.report);
    }
    // Stored before anything changes: a pass cut off goes on from here.
    state.in_progress = Some(plan.clone());
    save_state(&state)?;
    apply(db, config, &plan, stop_after)?;
    state.in_progress = None;
    state.notice_seen = plan.report.says_nothing();
    state.report = Some(plan.report.clone());
    save_state(&state)?;
    record_done(db)?;
    Ok(plan.report)
}

/// Adds to the stored record what `report` leaves for the owner and it does not name yet — a kept variant, a skipped
/// file — and shows the notice again; `false` when there is nothing new.
fn remember(state: &mut State, report: &Report) -> bool {
    match &mut state.report {
        None if report.says_nothing() => return false,
        None => state.report = Some(report.clone()),
        Some(stored) => {
            let kept: Vec<KeptVariant> = report
                .kept
                .iter()
                .filter(|kept| !stored.kept.contains(kept))
                .cloned()
                .collect();
            let skipped: Vec<String> = report
                .skipped
                .iter()
                .filter(|line| !stored.skipped.contains(line))
                .cloned()
                .collect();
            if kept.is_empty() && skipped.is_empty() {
                return false;
            }
            stored.kept.extend(kept);
            stored.skipped.extend(skipped);
        }
    }
    state.notice_seen = false;
    true
}

/// Records in `app_meta` that the pass ran.
fn record_done(db: &Connection) -> Result<()> {
    db.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [DONE_KEY, &chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// Runs the pass once — at the app's start — unless it is recorded as done; a pass cut off earlier is finished. `None`
/// when there was nothing to run (or it failed, or another process held the pass longer than [`START_LOCK_WAIT`];
/// then it is tried again at the next start). It never keeps the window waiting longer than that.
pub fn run_once(db: &Connection, config: &mut Config, today: NaiveDate) -> Option<Report> {
    run_once_within(db, config, today, START_LOCK_WAIT)
}

fn run_once_within(
    db: &Connection,
    config: &mut Config,
    today: NaiveDate,
    wait: Duration,
) -> Option<Report> {
    let done: Option<String> = db
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [DONE_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if done.is_some() && load_state().in_progress.is_none() {
        let folded = fold_leftovers(db, config, today);
        if !folded.is_empty() {
            tracing::info!(?folded, "folded the role variants an earlier pass kept");
        }
        return None;
    }
    match run_with(db, config, false, today, None, Some(wait)) {
        Ok(report) => Some(report),
        Err(err) => {
            tracing::warn!(%err, "the agents migration did not run at this start");
            None
        }
    }
}

/// Whether the pass is recorded as done — the app seeds the base roles only then, so a missing base is made by the pass
/// at its variants' tier.
pub fn done(db: &Connection) -> bool {
    db.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        [DONE_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// The kept variant `name` of the last pass and its base, if `name` is one.
fn kept_base(state: &State, name: &str) -> Result<String> {
    axiomata_roster::check_slug("name", name)?;
    state
        .report
        .as_ref()
        .and_then(|report| report.kept.iter().find(|kept| kept.name == name))
        .map(|kept| kept.base.clone())
        .ok_or_else(|| refusal("role", format!("“{name}” is no role the migration kept")))
}

/// Folds the variants an earlier pass kept as own roles — one that differed from its base by a line was shown to the
/// owner then; now every variant goes into its base. Runs at the app's start, waiting for the lock no longer than
/// [`START_LOCK_WAIT`] each; returns the names it folded. One whose base is gone stays (its sessions would play no
/// role); at the first failure the rest wait for the next start.
pub fn fold_leftovers(db: &Connection, config: &mut Config, today: NaiveDate) -> Vec<String> {
    let state = load_state();
    let Some(report) = &state.report else {
        return Vec::new();
    };
    let dir = paths::agent_roles_dir();
    let mut folded = Vec::new();
    for kept in &report.kept {
        if state.decided.contains(&kept.name)
            || fs::symlink_metadata(dir.join(&kept.name)).is_err()
            || !dir.join(&kept.base).join(ROLE_FILE).is_file()
        {
            continue;
        }
        if let Err(err) = fold_variant(db, config, &kept.name, today, Some(START_LOCK_WAIT)) {
            tracing::warn!(%err, role = %kept.name, "a kept role variant was not folded at this start");
            break;
        }
        folded.push(kept.name.clone());
    }
    folded
}

/// Folds the kept variant `name` into its base: its sessions and the favorites naming it play the base, then the
/// variant's folder goes into the backup, then it is recorded — each step safe to run again, so a fold cut off is
/// finished at the next start. `config` (the live one) takes the file config's favorites; `wait` bounds the wait for
/// the lock (`None`: as long as it takes).
///
/// # Errors
///
/// A refusal for a name that is no kept variant of the pass; a failing move or write.
fn fold_variant(
    db: &Connection,
    config: &mut Config,
    name: &str,
    today: NaiveDate,
    wait: Option<Duration>,
) -> Result<()> {
    let _lock = MigrationLock::take(wait)?;
    let mut state = read_state()?;
    let base = kept_base(&state, name)?;
    let home = paths::axiomata_home();
    let backup = state
        .report
        .as_ref()
        .and_then(|report| report.backup.clone())
        .filter(|path| is_backup_place(&home, path))
        .unwrap_or_else(|| fresh_backup_dir(&home, today));
    let tx = db.unchecked_transaction()?;
    for project in crate::ide::store::list_projects(&tx)? {
        for agent in agent_store::list_agents(&tx, project.id)? {
            if agent.agent_role == name && !agent.is_master() {
                agent_store::set_role(&tx, agent.id, &base)?;
            }
        }
    }
    tx.commit()?;
    {
        let _guard = config::file_edit_lock();
        let mut disk = Config::load()?;
        if disk
            .agents
            .favorites
            .iter()
            .any(|favorite| favorite.role == name)
        {
            for favorite in &mut disk.agents.favorites {
                if favorite.role == name {
                    favorite.role.clone_from(&base);
                }
            }
            unique(&mut disk.agents.favorites);
            disk.save()?;
        }
        config.agents.favorites = disk.agents.favorites;
    }
    move_role(&paths::agent_roles_dir(), &backup, name)?;
    state.decided.insert(name.to_owned());
    if let Some(report) = &mut state.report {
        report.kept.retain(|kept| kept.name != name);
        report.moved.push(name.to_owned());
        if report
            .backup
            .as_ref()
            .is_none_or(|path| !is_backup_place(&home, path))
        {
            report.backup = Some(backup);
        }
    }
    save_state(&state)
}

/// The owner closed the result notice.
///
/// # Errors
///
/// A state file this build cannot read; a failing write of the state.
pub fn dismiss_notice() -> Result<()> {
    let _lock = MigrationLock::take(None)?;
    let mut state = read_state()?;
    state.notice_seen = true;
    save_state(&state)
}

/// A backup folder the pass made, with its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backup {
    pub path: PathBuf,
    pub bytes: u64,
}

/// The backup folders in `~/.axiomata`.
pub fn backups() -> Vec<Backup> {
    let home = paths::axiomata_home();
    let Ok(entries) = fs::read_dir(&home) else {
        return Vec::new();
    };
    let mut found: Vec<Backup> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(BACKUP_PREFIX)
        })
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| Backup {
            bytes: dir_size(&entry.path()),
            path: entry.path(),
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| match entry.file_type() {
                    Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
                    Ok(kind) if kind.is_file() => entry.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Deletes a backup folder — the owner's click in Settings → Storage, after the app asked. Only a folder [`backups`]
/// lists is accepted, so no other path can be named.
///
/// # Errors
///
/// A refusal for a path that is no backup folder; a failing delete.
pub fn delete_backup(path: &Path) -> Result<()> {
    if !backups().iter().any(|backup| backup.path == path) {
        return Err(refusal(
            "path",
            "only a backup folder of the agents migration can be deleted".to_owned(),
        ));
    }
    fs::remove_dir_all(path).map_err(|source| AxiomataError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests;
