//! The one-time move from roles that carry engines to roles at tiers (`docs/plans/agents-model.md`, AM6, Q14, Q9, Q13).
//!
//! Before AM1 a role was its job *and* its engine, so the owner's folder grew copies — `planner-haiku`,
//! `reviewer-heavy`, `allrounder-glut` — and the config held engines that are now automatic. This pass:
//!
//! * **roles** (`~/.axiomata/agents/`): makes sure the seven base roles exist (from the bundled text, at the variants'
//!   most common tier); a variant `<base>-<suffix>` that says the same as its base (instructions, permissions, limits,
//!   kind, creates) is moved into a backup folder, one that differs stays as an own role and is shown to the owner
//!   ([`pending`]); the roles that stay lose `engine`/`fallback_engines` — their original files are copied into the
//!   backup first;
//! * **engines**: an own engine that is just an automatic one (same harness and model, no command, no environment) is
//!   removed from the config — its model switched on if it was off — and its id mapped to the automatic one's; the
//!   removed entries are written to the backup as `engines.toml`;
//! * **default engine per tier**, when the owner set none: the engine the old roles of that tier used most;
//! * **sessions**: a session of a moved variant plays its base; one on a removed engine runs on the automatic one.
//!   Names, worktrees and branches stay; masters are not touched.
//!
//! Nothing is deleted: the backup folder (`~/.axiomata/agents-backup-<date>/`) stays until the owner removes it in
//! Settings → Storage. Project roles in repositories are never touched. The pass is idempotent and runs once at the
//! app's start ([`run_once`]); `axiomata-cli ide migrate-agents [--dry-run]` runs it on the owner's word.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use axiomata_roster::{Engine, Harness, Role, Source, Tier};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

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

/// A variant that differs from its base and stays as an own role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptVariant {
    pub name: String,
    pub base: String,
}

/// What the pass did — or, in a dry run, would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub dry_run: bool,
    /// Where the originals went; `None` when nothing needed a backup.
    pub backup: Option<PathBuf>,
    /// Base roles that were missing and were made.
    pub created: Vec<String>,
    /// Variants equal to their base, moved into the backup.
    pub moved: Vec<String>,
    /// Variants that differ from their base and stay as own roles.
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
}

/// The app's memory of the pass (`~/.axiomata/agents-migration.json`): its report, whether the owner saw the notice,
/// and the kept variants the owner already decided on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub version: u64,
    #[serde(default)]
    pub report: Option<Report>,
    #[serde(default)]
    pub notice_seen: bool,
    /// Kept variants the owner kept or folded: no longer asked about.
    #[serde(default)]
    pub decided: BTreeSet<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            report: None,
            notice_seen: false,
            decided: BTreeSet::new(),
        }
    }
}

/// The stored state; the default when there is none.
pub fn load_state() -> State {
    json_state::load(&paths::agents_migration_path(), || {
        serde_json::to_string(&State::default()).unwrap_or_default()
    })
    .ok()
    .and_then(|state| serde_json::from_str::<State>(&state.json).ok())
    .unwrap_or_default()
}

fn save_state(state: &State) -> Result<()> {
    let json =
        serde_json::to_string(state).map_err(|err| AxiomataError::InvalidDashboardState {
            path: paths::agents_migration_path(),
            reason: err.to_string(),
        })?;
    json_state::save(&paths::agents_migration_path(), &json)
}

/// Whether variant `variant` says what `base` says: instructions, permissions, limits, kind and what it may create —
/// its name, description, engines and tier aside.
fn same_job(variant: &Role, base: &Role) -> bool {
    variant.instructions.trim() == base.instructions.trim()
        && variant.permissions == base.permissions
        && variant.limits == base.limits
        && variant.kind == base.kind
        && variant.creates == base.creates
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

/// What the pass will change, worked out before anything is written.
#[derive(Debug, Default)]
struct Plan {
    report: Report,
    /// Bases to write.
    create: Vec<Role>,
    /// Roles (by name) that stay and are rewritten without their engines.
    strip: Vec<Role>,
    /// Own engines to remove, with their entries for the backup.
    removed_engines: BTreeMap<String, Engine>,
    /// Model keys to switch on.
    switch_on: Vec<String>,
    /// Default engines to seed.
    defaults: Vec<(Tier, String)>,
    /// Session id → (new role, new engine id).
    sessions: Vec<(i64, Option<String>, Option<String>)>,
}

/// Works out what the pass changes for `roles` (the owner's), the file config `disk`, `sources` and the sessions in
/// `db`.
fn plan(db: &Connection, disk: &Config, sources: &ModelSources, roles: &[Role]) -> Result<Plan> {
    let mut plan = Plan::default();
    let by_name: HashMap<&str, &Role> = roles
        .iter()
        .map(|role| (role.name.as_str(), role))
        .collect();

    // Roles: bases, variants.
    let mut bases: HashMap<String, Role> = HashMap::new();
    for bundled in axiomata_roster::base_roles() {
        let variants: Vec<&Role> = roles
            .iter()
            .filter(|r| base_of(&r.name) == Some(bundled.name.as_str()))
            .collect();
        let base = match by_name.get(bundled.name.as_str()) {
            Some(own) => (*own).clone(),
            None => {
                let mut made = bundled.clone();
                if let Some(tier) = most_common(variants.iter().map(|v| v.tier)) {
                    made.tier = tier;
                }
                plan.report.created.push(made.name.clone());
                plan.create.push(made.clone());
                made
            }
        };
        for variant in variants {
            if same_job(variant, &base) {
                plan.report.moved.push(variant.name.clone());
            } else {
                plan.report.kept.push(KeptVariant {
                    name: variant.name.clone(),
                    base: base.name.clone(),
                });
            }
        }
        bases.insert(base.name.clone(), base);
    }
    let moved: BTreeSet<&str> = plan.report.moved.iter().map(String::as_str).collect();
    for role in roles {
        if !moved.contains(role.name.as_str())
            && (role.engine.is_some() || !role.fallback_engines.is_empty())
        {
            plan.report.stripped.push(role.name.clone());
            let mut stripped = role.clone();
            stripped.engine = None;
            stripped.fallback_engines.clear();
            plan.strip.push(stripped);
        }
    }

    // Engines: an own engine that is an automatic one goes, its model switched on.
    let mut next = disk.clone();
    for engine in disk.agents.engines.values() {
        let plain = engine.command.trim().is_empty() && engine.env.trim().is_empty();
        let Some((provider, id)) = catalog_model(engine).filter(|_| plain) else {
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
            let choice = next.agents.models.choices.entry(key.clone()).or_default();
            choice.enabled = Some(true);
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
    plan.switch_on.retain(|key| {
        plan.removed_engines
            .values()
            .any(|e| catalog_model(e).is_some_and(|(p, id)| model_tiers::model_key(p, &id) == *key))
    });
    plan.report.engines_mapped = mapping.clone();
    plan.report.models_switched_on = plan.switch_on.clone();
    let resolve = |id: &str| mapping.get(id).cloned().unwrap_or_else(|| id.to_owned());

    // The default engine of each tier, when the owner set none: what the old roles of that tier used most.
    let defaults = &disk.agents.models.default_engines;
    if Tier::ALL.iter().all(|tier| defaults.get(*tier).is_none()) {
        let lookup = engines::Lookup::with_sources(&next, sources.clone());
        for tier in Tier::ALL {
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
    }

    // Sessions: a moved variant plays its base; a removed engine's sessions run on its automatic one.
    for project in crate::ide::store::list_projects(db)? {
        for agent in agent_store::list_agents(db, project.id)? {
            if agent.is_master() {
                continue;
            }
            let role = moved
                .contains(agent.agent_role.as_str())
                .then(|| base_of(&agent.agent_role).map(str::to_owned))
                .flatten();
            let engine = agent
                .engine_id
                .as_deref()
                .and_then(|id| mapping.get(id))
                .filter(|new| agent.engine_id.as_deref() != Some(new.as_str()));
            if role.is_some() || engine.is_some() {
                plan.sessions.push((agent.id, role, engine.cloned()));
            }
        }
    }
    plan.report.sessions_remapped = plan.sessions.len();
    Ok(plan)
}

/// The backup folder of `today` in `home`.
fn backup_dir(home: &Path, today: NaiveDate) -> PathBuf {
    home.join(format!("{BACKUP_PREFIX}{}", today.format("%Y-%m-%d")))
}

/// Moves `from` to `to_dir/<name>`, or `<name>-2`, … when the backup already holds that name.
fn move_into(from: &Path, to_dir: &Path, name: &str) -> Result<PathBuf> {
    let io = |path: &Path, source| AxiomataError::Io {
        path: path.to_path_buf(),
        source,
    };
    fs::create_dir_all(to_dir).map_err(|source| io(to_dir, source))?;
    let target = std::iter::once(to_dir.join(name))
        .chain((2..).map(|n| to_dir.join(format!("{name}-{n}"))))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| to_dir.join(name));
    fs::rename(from, &target).map_err(|source| io(from, source))?;
    Ok(target)
}

/// Runs the pass: works out what changes and, unless `dry_run`, does it. `config` (the live one) takes the file
/// config's engines and models afterwards.
///
/// # Errors
///
/// A failing read of the roles, the config or the database; a failing write. A pass that fails half-way leaves
/// what it moved in the backup and is run again by the next start (it is not recorded as done).
pub fn run(
    db: &Connection,
    config: &mut Config,
    dry_run: bool,
    today: NaiveDate,
) -> Result<Report> {
    let dir = paths::agent_roles_dir();
    let roles = if dir.exists() {
        axiomata_roster::load_roles(&dir, Source::User)?.roles
    } else {
        Vec::new()
    };
    let _guard = config::file_edit_lock();
    let mut disk = Config::load()?;
    let sources = ModelSources::load();
    let mut plan = plan(db, &disk, &sources, &roles)?;
    plan.report.dry_run = dry_run;
    let needs_backup =
        !plan.report.moved.is_empty() || !plan.strip.is_empty() || !plan.removed_engines.is_empty();
    let backup = backup_dir(&paths::axiomata_home(), today);
    if needs_backup {
        plan.report.backup = Some(backup.clone());
    }
    if dry_run {
        return Ok(plan.report);
    }

    // Roles: the originals of what changes go into the backup first.
    for name in &plan.report.moved {
        move_into(&dir.join(name), &backup, name)?;
    }
    for role in &plan.strip {
        let original = dir.join(&role.name).join(axiomata_roster::ROLE_FILE);
        let copy_dir = backup.join(&role.name);
        fs::create_dir_all(&copy_dir).map_err(|source| AxiomataError::Io {
            path: copy_dir.clone(),
            source,
        })?;
        fs::copy(&original, copy_dir.join(axiomata_roster::ROLE_FILE)).map_err(|source| {
            AxiomataError::Io {
                path: original.clone(),
                source,
            }
        })?;
        axiomata_roster::save_role(&dir, role)?;
    }
    for role in &plan.create {
        axiomata_roster::save_role(&dir, role)?;
    }

    // Engines and defaults, into the file config.
    if !plan.removed_engines.is_empty() {
        let toml = toml::to_string_pretty(&plan.removed_engines)
            .map_err(|source| AxiomataError::ConfigSerialize { source })?;
        fs::create_dir_all(&backup).map_err(|source| AxiomataError::Io {
            path: backup.clone(),
            source,
        })?;
        let target = backup.join("engines.toml");
        fs::write(&target, toml).map_err(|source| AxiomataError::Io {
            path: target,
            source,
        })?;
    }
    let config_changes =
        !plan.removed_engines.is_empty() || !plan.switch_on.is_empty() || !plan.defaults.is_empty();
    if config_changes {
        for id in plan.removed_engines.keys() {
            disk.agents.engines.remove(id);
        }
        for key in &plan.switch_on {
            disk.agents
                .models
                .choices
                .entry(key.clone())
                .or_insert_with(ModelChoice::default)
                .enabled = Some(true);
        }
        for (tier, id) in &plan.defaults {
            disk.agents
                .models
                .default_engines
                .set(*tier, Some(id.clone()));
        }
        disk.validate_for_save()
            .map_err(|reason| refusal("config", reason))?;
        disk.save()?;
    }
    config.agents.engines = disk.agents.engines.clone();
    config.agents.models = disk.agents.models.clone();

    // Sessions.
    for (id, role, engine) in &plan.sessions {
        if let Some(role) = role {
            agent_store::set_role(db, *id, role)?;
        }
        if let Some(engine) = engine {
            agent_store::set_engine(db, *id, Some(engine))?;
        }
    }
    Ok(plan.report)
}

/// Runs the pass once — at the app's start — and records it: its report goes to `agents-migration.json` for the notice,
/// the moment into `app_meta`. `None` when it ran before (or failed; then it is tried again at the next start).
pub fn run_once(db: &Connection, config: &mut Config, today: NaiveDate) -> Option<Report> {
    let done: Option<String> = db
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [DONE_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if done.is_some() {
        return None;
    }
    match run(db, config, false, today) {
        Ok(report) => {
            let mut state = load_state();
            state.notice_seen = report.is_empty();
            state.report = Some(report.clone());
            if let Err(err) = save_state(&state) {
                tracing::warn!(%err, "could not keep the result of the agents migration");
            }
            if let Err(err) = db.execute(
                "INSERT INTO app_meta (key, value) VALUES (?1, ?2) \
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [DONE_KEY, &chrono::Utc::now().to_rfc3339()],
            ) {
                tracing::warn!(%err, "could not record the agents migration");
            }
            Some(report)
        }
        Err(err) => {
            tracing::warn!(%err, "the agents migration did not run");
            None
        }
    }
}

/// A kept variant beside its base, for the owner's side-by-side look.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantDiff {
    pub variant: Role,
    pub base: Role,
    /// The fields that differ: `instructions`, `permissions`, `limits`, `kind`, `creates`.
    pub differs: Vec<&'static str>,
}

/// The kept variants still in the folder that the owner has not decided on.
///
/// # Errors
///
/// A failing read of the roles.
pub fn pending() -> Result<Vec<VariantDiff>> {
    let state = load_state();
    let Some(report) = &state.report else {
        return Ok(Vec::new());
    };
    let dir = paths::agent_roles_dir();
    let roles = axiomata_roster::load_roles(&dir, Source::User)?.roles;
    let find = |name: &str| roles.iter().find(|role| role.name == name).cloned();
    Ok(report
        .kept
        .iter()
        .filter(|kept| !state.decided.contains(&kept.name))
        .filter_map(|kept| {
            let (variant, base) = (find(&kept.name)?, find(&kept.base)?);
            let mut differs = Vec::new();
            if variant.instructions.trim() != base.instructions.trim() {
                differs.push("instructions");
            }
            if variant.permissions != base.permissions {
                differs.push("permissions");
            }
            if variant.limits != base.limits {
                differs.push("limits");
            }
            if variant.kind != base.kind {
                differs.push("kind");
            }
            if variant.creates != base.creates {
                differs.push("creates");
            }
            Some(VariantDiff {
                variant,
                base,
                differs,
            })
        })
        .collect())
}

/// The owner's "Keep as own role": not asked about again.
///
/// # Errors
///
/// A failing write of the state.
pub fn keep_variant(name: &str) -> Result<()> {
    let mut state = load_state();
    state.decided.insert(name.to_owned());
    save_state(&state)
}

/// The owner's "Fold into <base>": the variant's folder goes into the backup, and its sessions play the base.
///
/// # Errors
///
/// A refusal for a name that is no kept variant of the pass; a failing move or write.
pub fn fold_variant(db: &Connection, name: &str, today: NaiveDate) -> Result<()> {
    let mut state = load_state();
    let base = state
        .report
        .as_ref()
        .and_then(|report| report.kept.iter().find(|kept| kept.name == name))
        .map(|kept| kept.base.clone())
        .ok_or_else(|| refusal("role", format!("“{name}” is no role the migration kept")))?;
    let backup = state
        .report
        .as_ref()
        .and_then(|report| report.backup.clone())
        .unwrap_or_else(|| backup_dir(&paths::axiomata_home(), today));
    let dir = paths::agent_roles_dir();
    axiomata_roster::check_slug("name", name)?;
    move_into(&dir.join(name), &backup, name)?;
    for project in crate::ide::store::list_projects(db)? {
        for agent in agent_store::list_agents(db, project.id)? {
            if agent.agent_role == name && !agent.is_master() {
                agent_store::set_role(db, agent.id, &base)?;
            }
        }
    }
    state.decided.insert(name.to_owned());
    if let Some(report) = &mut state.report
        && report.backup.is_none()
    {
        report.backup = Some(backup);
    }
    save_state(&state)
}

/// The owner closed the result notice.
///
/// # Errors
///
/// A failing write of the state.
pub fn dismiss_notice() -> Result<()> {
    let mut state = load_state();
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
