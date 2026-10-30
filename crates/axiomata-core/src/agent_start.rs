//! Starting an agent from the Agents tab (`docs/plans/agents-model.md`, AM4, Q7, Q8, Q16, Q18), and the owner's
//! favorites.
//!
//! A start is a role at a tier on an engine — an engine picked, or "the tier's default", resolved now through
//! [`engines::engine_for`] (the owner's default first, then the cheapest that can run). The session is made the one way
//! sessions are made, `roster::create_agent_on_engine`; nothing here starts a process. It is named by
//! [`agent_choice::agent_name`] ("Glut Medium Allrounder (Sonnet 5.5)") and gets ` #2`, ` #3`, … when the project
//! has one of that name already.
//!
//! A favorite is role + tier + engine, or role + tier + "the tier's default" (which follows the default). Kept in the
//! file config (`agents.favorites`), saved under its lock like engines; none at first. A favorite whose engine is gone
//! is shown as unavailable, never quietly moved to another engine.

use axiomata_roster::{Role, Tier, check_slug};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::agent_choice;
use crate::config::Config;
use crate::engines::{self, EngineInfo};
use crate::error::AxiomataError;
use crate::error::coded;
use crate::ide::model::Agent;
use crate::model_tiers::ModelSources;
use crate::roster::{self, refusal};

type Result<T> = std::result::Result<T, AxiomataError>;

/// The longest session name the agent store takes, in characters (`axiomata-ide::agent_store`).
pub const MAX_SESSION_NAME_CHARS: usize = 60;

/// The most ` #n` tried for a free name before giving up.
const MAX_NAME_TRIES: u32 = 99;

/// The most favorites kept: a row of tiles, not a second roster.
pub const MAX_FAVORITES: usize = 40;

/// A favorite of the owner's: a role at a tier, on an engine or on the tier's default (`engine: None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub role: String,
    pub tier: Tier,
    /// `None` = whatever the tier's default engine is at the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

impl Favorite {
    /// Checks the ids.
    ///
    /// # Errors
    ///
    /// A message naming the bad field.
    pub fn validate(&self) -> std::result::Result<(), String> {
        check_slug("role", &self.role).map_err(|err| err.to_string())?;
        if let Some(engine) = &self.engine {
            check_slug("engine", engine).map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

/// Checks a whole list of favorites, as `Config::validate_for_save` does.
///
/// # Errors
///
/// A message naming the first bad favorite, or a list that is too long.
pub fn validate_favorites(favorites: &[Favorite]) -> std::result::Result<(), String> {
    if favorites.len() > MAX_FAVORITES {
        return Err(format!("at most {MAX_FAVORITES} favorites"));
    }
    favorites.iter().try_for_each(|favorite| {
        favorite
            .validate()
            .map_err(|err| format!("favorite: {err}"))
    })
}

/// The engines of each tier in the order a start tries them; the first is the tier's standard.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TierEngines {
    pub tier: Tier,
    pub engines: Vec<EngineInfo>,
}

/// [`engines::engine_for`] for every tier, light → heavy.
pub fn tier_engines(config: &Config, sources: &ModelSources) -> Vec<TierEngines> {
    Tier::ALL
        .into_iter()
        .map(|tier| TierEngines {
            tier,
            engines: engines::engine_for(tier, config, sources),
        })
        .collect()
}

/// A favorite as a tile shows it: whether it can start now, and on what.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FavoriteView {
    #[serde(flatten)]
    pub favorite: Favorite,
    /// The engine a click would start on now; `None` when it cannot start.
    pub engine_id: Option<String>,
    /// Why it cannot start now (its role or engine is gone, the engine cannot run, the tier has none).
    pub unavailable: Option<String>,
    /// The agent's name and its two lines (Q8); the engine's id stands in for a model that is gone.
    pub name: String,
    pub title: String,
    pub subtitle: String,
}

/// The engine a start of `tier` runs on: `engine` when one is asked for (it must exist and be able to run), else the
/// tier's first that can ([`engines::engine_for`]).
///
/// # Errors
///
/// A refusal naming what to do: the engine does not exist or cannot run, or the tier has no engine that can.
pub fn resolve_engine(
    config: &Config,
    sources: &ModelSources,
    tier: Tier,
    engine: Option<&str>,
) -> Result<EngineInfo> {
    match engine {
        Some(id) => {
            let info = engines::find(config, sources, id).ok_or_else(|| {
                coded(
                    "engine.gone",
                    &[("id", id)],
                    format!("there is no engine “{id}” any more; pick another one"),
                )
            })?;
            if let Some(why) = &info.unusable {
                return Err(coded(
                    "engine.unusable",
                    &[("id", id), ("why", why)],
                    format!("the engine “{id}” cannot run now: {why}"),
                ));
            }
            Ok(info)
        }
        None => engines::engine_for(tier, config, sources)
            .into_iter()
            .next()
            .ok_or_else(|| {
                coded(
                    "engine.noneForTier",
                    &[("tier", tier.as_str())],
                    format!(
                        "no engine of the tier {} can run now; switch a model of that tier on in the Studio's \
                         Models tab",
                        tier.as_str()
                    ),
                )
            }),
    }
}

/// The name of a new session (Q8): [`agent_choice::agent_name`], the model cut where the name would pass
/// [`MAX_SESSION_NAME_CHARS`] with room for ` #n`, and ` #2`, ` #3`, … while `taken` has it (any case).
///
/// # Errors
///
/// A refusal after [`MAX_NAME_TRIES`] taken names.
pub fn session_name(
    engine: &EngineInfo,
    tier: Tier,
    role: &str,
    taken: &[String],
) -> Result<String> {
    let model = agent_choice::model_label(engine);
    let harness = engine.engine.harness;
    let room = " #99".len();
    let mut name = agent_choice::agent_name(harness, tier, role, &model);
    if name.chars().count() + room > MAX_SESSION_NAME_CHARS {
        let without_model = agent_choice::agent_name(harness, tier, role, "")
            .chars()
            .count();
        let keep = (MAX_SESSION_NAME_CHARS - room).saturating_sub(without_model + 1);
        let cut: String = model.chars().take(keep).collect();
        name = agent_choice::agent_name(harness, tier, role, &format!("{}…", cut.trim_end()));
    }
    let free = |candidate: &str| {
        !taken
            .iter()
            .any(|name| name.eq_ignore_ascii_case(candidate))
    };
    if free(&name) {
        return Ok(name);
    }
    (2..=MAX_NAME_TRIES)
        .map(|n| format!("{name} #{n}"))
        .find(|candidate| free(candidate))
        .ok_or_else(|| {
            coded(
                "agent.nameExhausted",
                &[("name", &name), ("n", &MAX_NAME_TRIES.to_string())],
                format!("“{name}” has been started {MAX_NAME_TRIES} times; remove its old sessions first"),
            )
        })
}

/// Starts `role` at `tier` on `engine` (or the tier's default) in project `project_id`: a new session made the one way
/// sessions are made. `roles` are the roles in force for the project.
///
/// # Errors
///
/// [`resolve_engine`]'s and [`session_name`]'s refusals; `roster::create_agent_on_engine`'s (an unknown role).
#[allow(clippy::too_many_arguments)]
pub fn start(
    db: &Connection,
    config: &Config,
    sources: &ModelSources,
    roles: &[Role],
    project_id: i64,
    role: &str,
    tier: Tier,
    engine: Option<&str>,
) -> Result<Agent> {
    let info = resolve_engine(config, sources, tier, engine)?;
    let taken: Vec<String> = crate::ide::agent_store::list_agents(db, project_id)?
        .into_iter()
        .map(|agent| agent.name)
        .collect();
    let name = session_name(&info, tier, role, &taken)?;
    roster::create_agent_on_engine(db, config, roles, project_id, &name, &info.engine.id, role)
}

/// Every favorite with what it would start now.
pub fn favorite_views(
    config: &Config,
    sources: &ModelSources,
    roles: &[Role],
) -> Vec<FavoriteView> {
    config
        .agents
        .favorites
        .iter()
        .map(|favorite| {
            let role_known = roles.iter().any(|role| role.name == favorite.role);
            let resolved =
                resolve_engine(config, sources, favorite.tier, favorite.engine.as_deref());
            let shown = match &resolved {
                Ok(info) => Some(info.clone()),
                Err(_) => favorite
                    .engine
                    .as_deref()
                    .and_then(|id| engines::find(config, sources, id)),
            };
            let unavailable = if !role_known {
                Some(format!("the role “{}” is gone", favorite.role))
            } else {
                resolved.as_ref().err().map(|err| err.to_string())
            };
            let (name, title, subtitle) = match &shown {
                Some(info) => {
                    let model = agent_choice::model_label(info);
                    let (title, subtitle) = agent_choice::agent_lines(
                        info.engine.harness,
                        favorite.tier,
                        &favorite.role,
                        &model,
                    );
                    let name = agent_choice::agent_name(
                        info.engine.harness,
                        favorite.tier,
                        &favorite.role,
                        &model,
                    );
                    (name, title, subtitle)
                }
                None => {
                    let title = format!(
                        "{} · {}",
                        agent_choice::role_label(&favorite.role),
                        favorite.tier.label()
                    );
                    let engine = favorite
                        .engine
                        .clone()
                        .unwrap_or_else(|| "default engine".to_owned());
                    (title.clone(), title, engine)
                }
            };
            FavoriteView {
                engine_id: resolved
                    .ok()
                    .filter(|_| role_known)
                    .map(|info| info.engine.id),
                favorite: favorite.clone(),
                unavailable,
                name,
                title,
                subtitle,
            }
        })
        .collect()
}

/// Adds `favorite` or, when the owner has it already, removes it. Saved into the file config under its lock, like
/// engines (never by saving the live config). Returns whether it is a favorite now.
///
/// # Errors
///
/// A refusal for a bad favorite or a full list; a failing read or save (then `config` is unchanged).
pub fn toggle_favorite(config: &mut Config, favorite: Favorite) -> Result<bool> {
    favorite
        .validate()
        .map_err(|reason| refusal("favorite", reason))?;
    let _guard = crate::config::file_edit_lock();
    let mut disk = Config::load()?;
    let now = if let Some(at) = disk.agents.favorites.iter().position(|f| *f == favorite) {
        disk.agents.favorites.remove(at);
        false
    } else {
        if disk.agents.favorites.len() >= MAX_FAVORITES {
            return Err(coded(
                "favorite.full",
                &[("n", &MAX_FAVORITES.to_string())],
                format!("at most {MAX_FAVORITES} favorites; remove one first"),
            ));
        }
        disk.agents.favorites.push(favorite);
        true
    };
    disk.save()?;
    config.agents.favorites = disk.agents.favorites;
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::EngineOrigin;
    use crate::model_tiers::TierSource;
    use crate::paths;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use axiomata_roster::{Billing, Engine, Harness};

    fn info(id: &str, model: Option<&str>, name: Option<&str>) -> EngineInfo {
        EngineInfo {
            engine: Engine {
                id: id.into(),
                label: id.into(),
                harness: Harness::Glut,
                command: String::new(),
                model: model.map(String::from),
                env: String::new(),
                billing: Billing::Metered,
                tier: None,
            },
            origin: EngineOrigin::Auto,
            rated_tier: Some(Tier::Medium),
            tier_source: TierSource::Price,
            model_key: None,
            model_name: name.map(String::from),
            output_per_m: None,
            unusable: None,
        }
    }

    #[test]
    fn a_session_is_named_like_its_agent_and_numbered_when_the_name_is_taken() {
        let engine = info("e", Some("openrouter/a/b"), Some("Sonnet 5.5"));
        let first = session_name(&engine, Tier::Medium, "allrounder", &[]).unwrap();
        assert_eq!(first, "Glut Medium Allrounder (Sonnet 5.5)");
        let taken = vec![
            first.clone(),
            "glut medium allrounder (sonnet 5.5) #2".to_owned(),
        ];
        assert_eq!(
            session_name(&engine, Tier::Medium, "allrounder", &taken).unwrap(),
            "Glut Medium Allrounder (Sonnet 5.5) #3"
        );
    }

    #[test]
    fn a_long_model_name_is_cut_so_the_name_fits_the_store_with_its_number() {
        let long = "An Extraordinarily Long Model Name That Goes On And On";
        let engine = info("e", None, Some(long));
        let name = session_name(&engine, Tier::Heavy, "implementer", &[]).unwrap();
        assert!(
            name.chars().count() + " #99".len() <= MAX_SESSION_NAME_CHARS,
            "{name}"
        );
        assert!(
            name.starts_with("Glut Heavy Implementer (An Extra") && name.ends_with("…)"),
            "{name}"
        );
    }

    #[test]
    fn such_a_name_is_safe_as_a_branch_and_a_worktree_folder() {
        let name = "Glut Medium Allrounder (DeepSeek V4.1 Flash) #2";
        let branch = crate::ide::worktree::branch_name(name, 7);
        assert_eq!(
            branch,
            "axiomata/glut-medium-allrounder-deepseek-v4-1-flash-2-7"
        );
        assert!(crate::ide::worktree::is_safe_ref(&branch));
        let path =
            crate::ide::worktree::worktree_path(std::path::Path::new("/w"), "Project", name, 7);
        assert_eq!(
            path,
            std::path::Path::new("/w/project/glut-medium-allrounder-deepseek-v4-1-flash-2-7")
        );
    }

    fn with_temp_home(body: impl FnOnce(&std::path::Path)) {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let home = unique_temp_dir("axiomata-test-agent-start");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialised by ENV_MUTEX.
        unsafe {
            std::env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        Config {
            workspace_root: home.join("workspace"),
            ..Config::default()
        }
        .save()
        .unwrap();
        body(&home);
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_favorite_is_toggled_in_the_file_and_one_whose_engine_is_gone_is_unavailable() {
        with_temp_home(|home| {
            let mut live = Config {
                workspace_root: home.join("live"),
                ..Config::default()
            };
            let pinned = Favorite {
                role: "allrounder".into(),
                tier: Tier::Heavy,
                engine: Some("claude-code-claude-opus-5-5".into()),
            };
            let gone = Favorite {
                role: "allrounder".into(),
                tier: Tier::Light,
                engine: Some("vanished".into()),
            };
            let follows = Favorite {
                role: "allrounder".into(),
                tier: Tier::Light,
                engine: None,
            };
            for favorite in [&pinned, &gone, &follows] {
                assert!(toggle_favorite(&mut live, favorite.clone()).unwrap());
            }
            assert_eq!(Config::load().unwrap().agents.favorites.len(), 3);
            assert_eq!(
                Config::load().unwrap().workspace_root,
                home.join("workspace")
            );

            let roles = vec![axiomata_roster::default_role()];
            let views = favorite_views(&live, &ModelSources::load(), &roles);
            assert_eq!(
                views[0].engine_id.as_deref(),
                Some("claude-code-claude-opus-5-5")
            );
            assert_eq!(views[0].name, "Claude Heavy Allrounder (Opus 5.5)");
            assert!(
                views[1].unavailable.is_some() && views[1].engine_id.is_none(),
                "not retargeted"
            );
            // The tier's default: Haiku is the cheapest light engine of the built-in list.
            assert_eq!(
                views[2].engine_id.as_deref(),
                Some("claude-code-claude-haiku-5-5")
            );
            let without_role = favorite_views(&live, &ModelSources::load(), &[]);
            assert!(without_role.iter().all(|view| view.unavailable.is_some()));

            assert!(
                !toggle_favorite(&mut live, gone).unwrap(),
                "a second toggle removes it"
            );
            assert_eq!(live.agents.favorites.len(), 2);
            let bad = Favorite {
                role: "Not A Slug".into(),
                tier: Tier::Light,
                engine: None,
            };
            assert!(toggle_favorite(&mut live, bad).is_err());
        });
    }

    #[test]
    fn a_start_runs_on_the_asked_engine_or_the_tiers_default_and_refuses_one_that_is_gone() {
        with_temp_home(|home| {
            let config = Config {
                workspace_root: home.join("workspace"),
                ..Config::default()
            };
            let sources = ModelSources::load();
            let tier_default = resolve_engine(&config, &sources, Tier::Medium, None).unwrap();
            assert_eq!(tier_default.engine.id, "claude-code-claude-sonnet-5-5");
            assert!(resolve_engine(&config, &sources, Tier::Medium, Some("nowhere")).is_err());

            let dir = unique_temp_dir("axiomata-test-agent-start-db");
            std::fs::create_dir_all(&dir).unwrap();
            let db = crate::db::open_and_migrate_at(&dir.join("t.db")).unwrap();
            let project = crate::ide::store::create_project(
                &db,
                crate::ide::model::NewProject {
                    name: "P".into(),
                    repo_root: dir.clone(),
                },
            )
            .unwrap();
            let roles = vec![axiomata_roster::default_role()];
            let first = start(
                &db,
                &config,
                &sources,
                &roles,
                project.id,
                "allrounder",
                Tier::Medium,
                None,
            )
            .unwrap();
            let second = start(
                &db,
                &config,
                &sources,
                &roles,
                project.id,
                "allrounder",
                Tier::Medium,
                None,
            )
            .unwrap();
            assert_eq!(first.name, "Claude Medium Allrounder (Sonnet 5.5)");
            assert_eq!(second.name, "Claude Medium Allrounder (Sonnet 5.5) #2");
            assert_eq!(
                first.engine_id.as_deref(),
                Some("claude-code-claude-sonnet-5-5")
            );
            assert_eq!(first.agent_role, "allrounder");
            assert!(
                start(
                    &db,
                    &config,
                    &sources,
                    &roles,
                    project.id,
                    "nobody",
                    Tier::Medium,
                    None
                )
                .is_err()
            );
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}
