//! The agents migration against a scratch home built like the owner's real one (2026-10-30): its 22 role names with
//! their kinds, tiers and engines, its 16 engines — instruction texts made up, the same ones where the real files share
//! theirs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axiomata_roster::{Billing, Engine, Harness, Limits, Role, Source, Tier};
use chrono::NaiveDate;
use rusqlite::Connection;

use super::*;
use crate::config::ModelCost;
use crate::ide::model::{AgentFields, NewAgent, NewProject};
use crate::model_catalog::{CatalogModel, ModelCatalog, ProviderCatalog};
use crate::test_support::{ENV_MUTEX, unique_temp_dir};

/// The day the backup is named after.
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()
}

/// One role of the fixture: name, kind, tier, engine, fallback engines, text letter.
type RoleRow = (
    &'static str,
    &'static str,
    Tier,
    &'static str,
    &'static [&'static str],
    &'static str,
);

/// The owner's roles: name, kind, tier, engine, fallbacks, and which made-up text it carries — the same letter where
/// the real files carry the same text.
const ROLES: [RoleRow; 22] = [
    (
        "allrounder",
        "implement",
        Tier::Medium,
        "claude-code-claude-sonnet-5-5",
        &[],
        "A",
    ),
    (
        "allrounder-glut",
        "implement",
        Tier::Medium,
        "glut-flash",
        &[],
        "A",
    ),
    (
        "allrounder-heavy",
        "implement",
        Tier::Heavy,
        "claude-code-claude-opus-5-5",
        &[],
        "A2",
    ),
    (
        "documenter",
        "doc",
        Tier::Light,
        "opencode-openrouter-ling-3-1-flash",
        &["opencode-openrouter-xiaomi-mimo-v2-6-flash"],
        "D",
    ),
    (
        "documenter-haiku",
        "doc",
        Tier::Light,
        "claude-code-claude-haiku-5-5",
        &[],
        "D",
    ),
    (
        "grill",
        "grill",
        Tier::Heavy,
        "claude-code-claude-opus-5-5",
        &[],
        "G",
    ),
    (
        "grill-haiku",
        "grill",
        Tier::Light,
        "claude-code-claude-haiku-5-5",
        &[],
        "G2",
    ),
    (
        "grill-mimo",
        "grill",
        Tier::Light,
        "opencode-openrouter-xiaomi-mimo-v2-6-flash",
        &[],
        "G2",
    ),
    (
        "grill-sonnet",
        "grill",
        Tier::Medium,
        "claude-code-claude-sonnet-5-5",
        &[],
        "G2",
    ),
    (
        "implementer-haiku",
        "implement",
        Tier::Medium,
        "claude-code-claude-haiku-5-5",
        &[],
        "I1",
    ),
    (
        "implementer-heavy",
        "implement",
        Tier::Heavy,
        "claude-code-claude-opus-5-5",
        &["claude-code-claude-sonnet-5-5"],
        "I2",
    ),
    (
        "implementer-light",
        "implement",
        Tier::Light,
        "opencode-openrouter-xiaomi-mimo-v2-6-flash",
        &[
            "opencode-openrouter-qwen3-8-flash",
            "opencode-openrouter-z-ai-glm-5-3-flash",
        ],
        "I3",
    ),
    (
        "planner",
        "plan",
        Tier::Heavy,
        "claude-code-claude-opus-5-5",
        &[],
        "P",
    ),
    (
        "planner-haiku",
        "plan",
        Tier::Light,
        "claude-code-claude-haiku-5-5",
        &[],
        "P2",
    ),
    (
        "planner-mimo",
        "plan",
        Tier::Light,
        "opencode-openrouter-xiaomi-mimo-v2-6-flash",
        &[],
        "P2",
    ),
    (
        "planner-sonnet",
        "plan",
        Tier::Medium,
        "claude-code-claude-sonnet-5-5",
        &[],
        "P2",
    ),
    (
        "reviewer",
        "review",
        Tier::Medium,
        "opencode-openrouter-deepseek-deepseek-v4-1-f",
        &["opencode-openrouter-gpt-6-luna"],
        "R",
    ),
    (
        "reviewer-haiku",
        "review",
        Tier::Light,
        "claude-code-claude-haiku-5-5",
        &[],
        "R2",
    ),
    (
        "reviewer-heavy",
        "review",
        Tier::Heavy,
        "claude-code-claude-sonnet-5-5",
        &["opencode-openrouter-deepseek-deepseek-v4-1-f"],
        "R3",
    ),
    (
        "reviewer-light",
        "review",
        Tier::Light,
        "opencode-openrouter-z-ai-glm-5-3-flash",
        &[
            "opencode-openrouter-qwen3-8-flash",
            "opencode-openrouter-gpt-6-luna",
        ],
        "R4",
    ),
    (
        "tester",
        "test",
        Tier::Medium,
        "claude-code-claude-sonnet-5-5",
        &[],
        "T",
    ),
    (
        "tester-haiku",
        "test",
        Tier::Light,
        "claude-code-claude-haiku-5-5",
        &[],
        "T",
    ),
];

/// The owner's engines: id, harness, model.
const ENGINES: [(&str, Harness, Option<&str>); 16] = [
    ("claude-code", Harness::ClaudeCode, None),
    (
        "claude-code-claude-haiku-5-5",
        Harness::ClaudeCode,
        Some("claude-haiku-5-5"),
    ),
    (
        "claude-code-claude-opus-5-5",
        Harness::ClaudeCode,
        Some("claude-opus-5-5"),
    ),
    (
        "claude-code-claude-sonnet-5-5",
        Harness::ClaudeCode,
        Some("claude-sonnet-5-5"),
    ),
    (
        "glut-flash",
        Harness::Glut,
        Some("deepseek/deepseek-v4.1-flash"),
    ),
    ("opencode", Harness::Glut, None),
    (
        "opencode-ollama-qwen3-8-27b-mlx",
        Harness::Glut,
        Some("ollama/qwen3.8:27b-mlx"),
    ),
    (
        "opencode-ollama-qwen3-8-27b-mxfp8",
        Harness::Glut,
        Some("ollama/qwen3.8:27b-mxfp8"),
    ),
    (
        "opencode-ollama-xiaomi-mimo-v2-6-flash",
        Harness::Glut,
        Some("ollama/xiaomi/mimo-v2.6-flash"),
    ),
    (
        "opencode-openrouter-deepseek-deepseek-v4-1-f",
        Harness::Glut,
        Some("openrouter/deepseek/deepseek-v4.1-flash"),
    ),
    (
        "opencode-openrouter-gpt-6-luna",
        Harness::Glut,
        Some("openrouter/openai/gpt-6-luna"),
    ),
    (
        "opencode-openrouter-ling-3-0-flash",
        Harness::Glut,
        Some("openrouter/inclusionai/ling-3.0-flash"),
    ),
    (
        "opencode-openrouter-ling-3-1-flash",
        Harness::Glut,
        Some("openrouter/inclusionai/ling-3.1-flash"),
    ),
    (
        "opencode-openrouter-qwen3-8-flash",
        Harness::Glut,
        Some("openrouter/qwen/qwen3.8-flash"),
    ),
    (
        "opencode-openrouter-xiaomi-mimo-v2-6-flash",
        Harness::Glut,
        Some("openrouter/xiaomi/mimo-v2.6-flash"),
    ),
    (
        "opencode-openrouter-z-ai-glm-5-3-flash",
        Harness::Glut,
        Some("openrouter/z-ai/glm-5.3-flash"),
    ),
];

/// A scratch home with the owner's roles, engines and a catalog that lists their models.
struct Home {
    dir: PathBuf,
    db: Connection,
    config: Config,
}

impl Drop for Home {
    fn drop(&mut self) {
        // SAFETY: the test holding ENV_MUTEX set it.
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn role(name: &str, kind: &str, tier: Tier, engine: &str, fallbacks: &[&str], text: &str) -> Role {
    let builds = kind != "doc" && kind != "plan" && kind != "grill";
    Role {
        name: name.into(),
        description: format!("the {name} role"),
        kind: kind.into(),
        tier,
        engine: Some(engine.into()),
        fallback_engines: fallbacks.iter().map(|id| (*id).to_owned()).collect(),
        permissions: if builds {
            vec!["Bash(cargo test:*)".into(), "Bash(git diff:*)".into()]
        } else {
            vec!["Bash(git diff:*)".into()]
        },
        limits: Limits::default(),
        creates: Vec::new(),
        instructions: format!("Instructions {text}."),
        source: Source::User,
    }
}

fn listed(id: &str, output: Option<f64>) -> CatalogModel {
    CatalogModel {
        id: id.into(),
        name: id.into(),
        context_length: None,
        price: output.map(|output| ModelCost {
            input_per_m: output / 4.0,
            cached_input_per_m: None,
            output_per_m: output,
        }),
        reasoning: None,
    }
}

fn home() -> Home {
    let dir = unique_temp_dir("axiomata-test-agents-migration");
    fs::create_dir_all(&dir).unwrap();
    // SAFETY: serialised by ENV_MUTEX, held by every test here.
    unsafe {
        std::env::set_var(paths::AXIOMATA_HOME_ENV, &dir);
    }
    let roles_dir = paths::agent_roles_dir();
    for (name, kind, tier, engine, fallbacks, text) in ROLES {
        axiomata_roster::save_role(&roles_dir, &role(name, kind, tier, engine, fallbacks, text))
            .unwrap();
    }
    let mut config = Config {
        workspace_root: dir.join("workspace"),
        ..Config::default()
    };
    config.agents.engines = ENGINES
        .iter()
        .map(|(id, harness, model)| {
            (
                (*id).to_owned(),
                Engine {
                    id: (*id).into(),
                    label: (*id).into(),
                    harness: *harness,
                    command: String::new(),
                    model: model.map(String::from),
                    env: String::new(),
                    billing: if *harness == Harness::ClaudeCode {
                        Billing::Subscription
                    } else {
                        Billing::Metered
                    },
                    tier: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    config.save().unwrap();
    let mut catalog = ModelCatalog::default();
    let router = [
        ("deepseek/deepseek-v4.1-flash", 0.4),
        ("openai/gpt-6-luna", 2.0),
        ("inclusionai/ling-3.0-flash", 0.3),
        ("inclusionai/ling-3.1-flash", 0.3),
        ("qwen/qwen3.8-flash", 0.4),
        ("xiaomi/mimo-v2.6-flash", 0.3),
        ("z-ai/glm-5.3-flash", 0.5),
    ];
    catalog.providers.insert(
        ProviderId::OpenRouter,
        ProviderCatalog {
            models: router
                .iter()
                .map(|(id, out)| listed(id, Some(*out)))
                .collect(),
            fetched_at: None,
            error: None,
        },
    );
    catalog.providers.insert(
        ProviderId::Ollama,
        ProviderCatalog {
            models: [
                "qwen3.8:27b-mlx",
                "qwen3.8:27b-mxfp8",
                "xiaomi/mimo-v2.6-flash",
            ]
            .iter()
            .map(|id| listed(id, None))
            .collect(),
            fetched_at: None,
            error: None,
        },
    );
    fs::write(
        paths::model_catalog_path(),
        serde_json::to_string(&catalog).unwrap(),
    )
    .unwrap();
    let db = crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
    Home { dir, db, config }
}

fn add_session(db: &Connection, project: i64, name: &str, role: &str, engine: &str) -> i64 {
    let agent = agent_store::create_agent(
        db,
        NewAgent {
            project_id: project,
            fields: AgentFields {
                name: name.into(),
                harness: Harness::Glut,
                command: String::new(),
                model: None,
                env: String::new(),
            },
        },
    )
    .unwrap();
    agent_store::set_role(db, agent.id, role).unwrap();
    agent_store::set_engine(db, agent.id, Some(engine)).unwrap();
    agent.id
}

/// Every file under `dir` with its content, for "nothing changed".
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_none_or(|ext| ext != "db" && ext != "db-wal" && ext != "db-shm")
            {
                out.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    out
}

#[test]
fn the_owners_layout_folds_into_seven_roles_and_automatic_engines() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
    let mut h = home();
    let project = crate::ide::store::create_project(
        &h.db,
        NewProject {
            name: "P".into(),
            repo_root: h.dir.clone(),
        },
    )
    .unwrap()
    .id;
    let tester = add_session(
        &h.db,
        project,
        "old tester",
        "tester-haiku",
        "opencode-openrouter-z-ai-glm-5-3-flash",
    );
    let griller = add_session(
        &h.db,
        project,
        "old grill",
        "grill-sonnet",
        "claude-code-claude-sonnet-5-5",
    );
    let master = add_session(
        &h.db,
        project,
        "master-glut",
        "master",
        "opencode-openrouter-gpt-6-luna",
    );

    // A dry run says everything and changes nothing.
    let before = snapshot(&h.dir);
    let dry = run(&h.db, &mut h.config, true, today()).unwrap();
    assert_eq!(snapshot(&h.dir), before, "a dry run writes nothing");
    assert!(dry.dry_run);

    let report = run(&h.db, &mut h.config, false, today()).unwrap();
    assert_eq!(
        Report {
            dry_run: true,
            ..report.clone()
        },
        dry,
        "the dry run said what the run did"
    );
    // The missing base is made from the bundled text.
    assert_eq!(report.created, ["implementer"]);
    // Variants equal to their base go into the backup; the others stay as own roles.
    assert_eq!(
        report.moved,
        ["allrounder-glut", "documenter-haiku", "tester-haiku"]
    );
    let kept: Vec<&str> = report.kept.iter().map(|k| k.name.as_str()).collect();
    for name in [
        "allrounder-heavy",
        "grill-sonnet",
        "implementer-light",
        "planner-haiku",
        "reviewer-heavy",
    ] {
        assert!(kept.contains(&name), "{name} differs and stays: {kept:?}");
    }
    let backup = report.backup.clone().unwrap();
    assert_eq!(backup, h.dir.join("agents-backup-2026-10-30"));
    assert!(
        backup.join("tester-haiku/AGENT.md").exists(),
        "moved, never deleted"
    );
    assert!(!paths::agent_roles_dir().join("tester-haiku").exists());

    // The roles that stay lose their engines; their originals are in the backup.
    let roles = axiomata_roster::load_roles(&paths::agent_roles_dir(), Source::User)
        .unwrap()
        .roles;
    assert!(
        roles
            .iter()
            .all(|r| r.engine.is_none() && r.fallback_engines.is_empty())
    );
    let original = fs::read_to_string(backup.join("planner/AGENT.md")).unwrap();
    assert!(
        original.contains("claude-code-claude-opus-5-5"),
        "the backup keeps the original"
    );
    assert_eq!(roles.len(), 22 - 3 + 1);

    // Engines: an own copy of an automatic one goes and maps to it; one with a model no catalog lists stays.
    let mapped = &report.engines_mapped;
    assert_eq!(
        mapped["claude-code-claude-opus-5-5"],
        "claude-code-claude-opus-5-5"
    );
    assert_eq!(
        mapped["opencode-openrouter-deepseek-deepseek-v4-1-f"],
        "glut-openrouter-deepseek-deepseek-v4-1-flash"
    );
    assert_eq!(
        mapped["opencode-ollama-qwen3-8-27b-mlx"],
        "glut-ollama-qwen3-8-27b-mlx"
    );
    let own: Vec<&str> = h.config.agents.engines.keys().map(String::as_str).collect();
    assert_eq!(
        own,
        ["claude-code", "glut-flash", "opencode"],
        "only the engines that are no model's stay"
    );
    assert!(
        fs::read_to_string(backup.join("engines.toml"))
            .unwrap()
            .contains("opencode-openrouter-gpt-6-luna")
    );
    assert_eq!(
        Config::load().unwrap().agents.engines.len(),
        3,
        "the file config too"
    );

    // The default engine per tier: what the old roles of that tier used most.
    let defaults = &h.config.agents.models.default_engines;
    assert_eq!(
        defaults.get(Tier::Light),
        Some("claude-code-claude-haiku-5-5")
    );
    assert_eq!(
        defaults.get(Tier::Medium),
        Some("claude-code-claude-sonnet-5-5")
    );
    assert_eq!(
        defaults.get(Tier::Heavy),
        Some("claude-code-claude-opus-5-5")
    );

    // Sessions: a moved variant's plays its base, on the automatic engine; a kept one's role stays; a master is left.
    let get = |id| agent_store::get_agent(&h.db, id).unwrap().unwrap();
    assert_eq!(get(tester).agent_role, "tester");
    assert_eq!(
        get(tester).engine_id.as_deref(),
        Some("glut-openrouter-z-ai-glm-5-3-flash")
    );
    assert_eq!(get(tester).name, "old tester", "names stay");
    assert_eq!(get(griller).agent_role, "grill-sonnet");
    assert_eq!(
        get(master).engine_id.as_deref(),
        Some("opencode-openrouter-gpt-6-luna")
    );

    // Idempotent: a second run finds nothing to do and touches nothing.
    let after = snapshot(&h.dir);
    let again = run(&h.db, &mut h.config, false, today()).unwrap();
    assert!(again.is_empty(), "{again:?}");
    assert_eq!(snapshot(&h.dir), after);
}

#[test]
fn it_runs_once_and_a_kept_variant_is_kept_or_folded_on_the_owners_word() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
    let mut h = home();
    let project = crate::ide::store::create_project(
        &h.db,
        NewProject {
            name: "P".into(),
            repo_root: h.dir.clone(),
        },
    )
    .unwrap()
    .id;
    let planner = add_session(
        &h.db,
        project,
        "old planner",
        "planner-haiku",
        "claude-code-claude-haiku-5-5",
    );
    assert!(run_once(&h.db, &mut h.config, today()).is_some());
    assert!(
        run_once(&h.db, &mut h.config, today()).is_none(),
        "recorded: once"
    );
    let state = load_state();
    assert!(!state.notice_seen, "the owner is told what happened");

    let waiting = pending().unwrap();
    let haiku = waiting
        .iter()
        .find(|d| d.variant.name == "planner-haiku")
        .unwrap();
    assert_eq!(haiku.base.name, "planner");
    assert_eq!(haiku.differs, ["instructions"]);

    keep_variant("planner-mimo").unwrap();
    fold_variant(&h.db, "planner-haiku", today()).unwrap();
    let names: Vec<String> = pending()
        .unwrap()
        .into_iter()
        .map(|d| d.variant.name)
        .collect();
    assert!(
        !names.contains(&"planner-mimo".to_owned()) && !names.contains(&"planner-haiku".to_owned())
    );
    assert!(
        paths::agent_roles_dir().join("planner-mimo").exists(),
        "kept stays"
    );
    assert!(
        !paths::agent_roles_dir().join("planner-haiku").exists(),
        "folded goes to the backup"
    );
    assert!(
        h.dir
            .join("agents-backup-2026-10-30/planner-haiku")
            .exists()
    );
    assert_eq!(
        agent_store::get_agent(&h.db, planner)
            .unwrap()
            .unwrap()
            .agent_role,
        "planner"
    );
    assert!(
        fold_variant(&h.db, "tester", today()).is_err(),
        "only a kept variant folds"
    );

    dismiss_notice().unwrap();
    assert!(load_state().notice_seen);

    // The backup is listed and deleted only on the owner's word, and only a backup.
    let listed = backups();
    assert_eq!(listed.len(), 1);
    assert!(delete_backup(&h.dir.join("agents")).is_err());
    delete_backup(&listed[0].path).unwrap();
    assert!(backups().is_empty());
}

#[test]
fn defaults_the_owner_set_are_kept() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
    let mut h = home();
    let mut disk = Config::load().unwrap();
    disk.agents
        .models
        .default_engines
        .set(Tier::Medium, Some("claude-code".into()));
    disk.save().unwrap();
    let report = run(&h.db, &mut h.config, false, today()).unwrap();
    assert!(report.defaults_seeded.is_empty());
    assert_eq!(
        h.config.agents.models.default_engines.get(Tier::Medium),
        Some("claude-code")
    );
    assert_eq!(
        h.config.agents.models.default_engines.get(Tier::Light),
        None
    );
}
