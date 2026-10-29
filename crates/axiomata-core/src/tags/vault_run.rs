//! The one run over the whole vault (`docs/plans/tags-links.md` Q9, TL6), in two steps the owner starts from
//! Settings → Workspace → Tags, each after seeing what it costs:
//!
//! 1. **The list** — the bundled skill `vault-tagging` reads the notes and proposes a tag list
//!    (`axiomata-cli notes tags propose`); the proposals wait for the owner as any other.
//! 2. **The notes** — the same skill plans tags and related links for every note with
//!    `axiomata-cli notes tag --dry-run`, which records the plan here instead of writing; the owner sees the preview
//!    (what each note would get) and only then writes it ([`apply`]) — or throws it away.
//!
//! Which step a run is in is stored here ([`set_phase`]): the skill asks `axiomata-cli notes tags todo` and is told,
//! so one skill serves both steps and a run that stopped half-way goes on where it stopped.

use std::collections::HashSet;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::note::{self, TagOutcome, TagRequest};
use super::{Vault, note_path};
use crate::config::{Config, HarnessId, ProviderId};
use crate::error::AxiomataError;

/// The bundled skill that does the run.
pub const SKILL: &str = "vault-tagging";

/// Notes the skill is given per `todo` call in the notes step: a batch it can keep in mind; it asks again for the next.
/// The list step gets every note at once — it only needs their paths to choose what to read, and nothing marks a note
/// as read there.
pub const TODO_BATCH: usize = 40;

/// Which step a vault run is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Propose the tag list.
    List,
    /// Plan tags and related links for every note.
    Notes,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::List => "list",
            Phase::Notes => "notes",
        }
    }

    /// Reads a phase as the app sends it.
    ///
    /// # Errors
    ///
    /// Anything but `list` or `notes`.
    pub fn parse(text: &str) -> Result<Self, AxiomataError> {
        match text {
            "list" => Ok(Phase::List),
            "notes" => Ok(Phase::Notes),
            other => Err(super::refused(format!("unknown step {other:?}"))),
        }
    }
}

/// Marks the step a run is about to do; `None` ends it.
///
/// # Errors
///
/// The database fails.
pub fn set_phase(db: &Connection, phase: Option<Phase>) -> Result<(), AxiomataError> {
    match phase {
        Some(phase) => db.execute(
            "INSERT INTO tag_run (id, phase, started_at) VALUES (1, ?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET phase = excluded.phase, started_at = excluded.started_at",
            params![phase.as_str(), Utc::now().to_rfc3339()],
        )?,
        None => db.execute("DELETE FROM tag_run", [])?,
    };
    Ok(())
}

/// The step a run is in, if one runs.
///
/// # Errors
///
/// The database fails.
pub fn phase(db: &Connection) -> Result<Option<Phase>, AxiomataError> {
    let text: Option<String> = db
        .query_row("SELECT phase FROM tag_run WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(text.and_then(|text| Phase::parse(&text).ok()))
}

/// One note as the run sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteSize {
    pub path: String,
    pub bytes: u64,
}

/// The notes a vault run may tag: every Markdown note an agent may tag ([`note_path`]), sorted, with its size.
///
/// # Errors
///
/// The vault cannot be walked.
pub fn taggable_notes(config: &Config) -> Result<Vec<NoteSize>, AxiomataError> {
    let vault = Vault::load(config)?;
    let mut notes: Vec<NoteSize> = vault
        .notes()
        .filter(|rel| note_path(rel).is_ok())
        .map(|rel| NoteSize {
            bytes: std::fs::symlink_metadata(config.workspace_root.join(rel))
                .map(|meta| meta.len())
                .unwrap_or(0),
            path: rel.clone(),
        })
        .collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(notes)
}

/// What `notes tags todo` tells the skill: the step, and the notes still to do in it (for the list, all of them at
/// once; for the notes, those without a plan or a skip yet, at most [`TODO_BATCH`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Todo {
    pub phase: Option<Phase>,
    pub notes: Vec<String>,
    /// Notes still to do beyond this batch.
    pub more: usize,
}

/// The work left for the run.
///
/// # Errors
///
/// The vault cannot be walked; the database fails.
pub fn todo(config: &Config, db: &Mutex<Connection>) -> Result<Todo, AxiomataError> {
    let (phase, planned) = {
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        (phase(&db)?, planned_notes(&db)?)
    };
    let left: Vec<String> = match phase {
        None => Vec::new(),
        Some(Phase::List) => taggable_notes(config)?
            .into_iter()
            .map(|n| n.path)
            .collect(),
        Some(Phase::Notes) => taggable_notes(config)?
            .into_iter()
            .map(|n| n.path)
            .filter(|path| !planned.contains(&path.to_lowercase()))
            .collect(),
    };
    let batch = if phase == Some(Phase::List) {
        left.len()
    } else {
        TODO_BATCH
    };
    let more = left.len().saturating_sub(batch);
    Ok(Todo {
        phase,
        notes: left.into_iter().take(batch).collect(),
        more,
    })
}

fn planned_notes(db: &Connection) -> Result<HashSet<String>, AxiomataError> {
    let mut stmt = db.prepare("SELECT note FROM tag_plan")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|note| note.to_lowercase())
        .collect())
}

/// What the run plans for one note: the preview's row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedNote {
    pub note: String,
    /// Tags from the list it would get.
    pub tags: Vec<String>,
    /// Link names it would get under `## Related`.
    pub related: Vec<String>,
    /// Tags it asked for that are not on the list: proposed to the owner when the plan is written, and set on the note
    /// once the owner accepts one.
    pub proposed: Vec<String>,
    /// Why the run left the note out (a name not safe on a command line, a note it could not read), if it did.
    pub skipped: Option<String>,
}

/// Records what a dry run of `notes tag` would set, while the run is in its notes step; outside of it a dry run
/// records nothing. `proposals` are the dry run's own `--propose` pairs, so a proposed tag keeps its description until
/// the plan is written. Returns whether it recorded.
///
/// # Errors
///
/// The database fails.
pub fn record(
    db: &Connection,
    outcome: &TagOutcome,
    proposals: &[(String, String)],
) -> Result<bool, AxiomataError> {
    if phase(db)? != Some(Phase::Notes) {
        return Ok(false);
    }
    let pairs: Vec<(String, String)> = outcome
        .proposed
        .iter()
        .map(|tag| {
            let description = proposals
                .iter()
                .find(|(raw, _)| super::detect::normalise(raw).as_deref() == Some(tag.as_str()))
                .map(|(_, description)| description.clone())
                .unwrap_or_default();
            (tag.clone(), description)
        })
        .collect();
    let json = |list: &[String]| serde_json::to_string(list).unwrap_or_else(|_| "[]".into());
    db.execute(
        "INSERT INTO tag_plan (note, tags, related, proposed, proposals, skipped, planned_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6) \
         ON CONFLICT(note) DO UPDATE SET tags = excluded.tags, related = excluded.related, \
         proposed = excluded.proposed, proposals = excluded.proposals, skipped = NULL, \
         planned_at = excluded.planned_at",
        params![
            outcome.note,
            json(&outcome.tagged),
            json(&outcome.related),
            json(&outcome.proposed),
            serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".into()),
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(true)
}

/// Records that the run left `note` out, and why, so the notes step can finish without it (the preview shows it).
/// Only in the notes step; returns whether it recorded.
///
/// # Errors
///
/// The database fails.
pub fn record_skip(db: &Connection, note: &str, reason: &str) -> Result<bool, AxiomataError> {
    if phase(db)? != Some(Phase::Notes) {
        return Ok(false);
    }
    let reason: String = super::list::one_line(reason);
    db.execute(
        "INSERT INTO tag_plan (note, skipped, planned_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(note) DO UPDATE SET tags = '[]', related = '[]', proposed = '[]', proposals = '[]', \
         skipped = excluded.skipped, planned_at = excluded.planned_at",
        params![note, reason, Utc::now().to_rfc3339()],
    )?;
    Ok(true)
}

/// The plan as it stands, by note.
///
/// # Errors
///
/// The database fails.
pub fn plan(db: &Connection) -> Result<Vec<PlannedNote>, AxiomataError> {
    let mut stmt =
        db.prepare("SELECT note, tags, related, proposed, skipped FROM tag_plan ORDER BY note")?;
    let parse = |text: String| serde_json::from_str::<Vec<String>>(&text).unwrap_or_default();
    let rows = stmt.query_map([], |row| {
        Ok(PlannedNote {
            note: row.get(0)?,
            tags: parse(row.get(1)?),
            related: parse(row.get(2)?),
            proposed: parse(row.get(3)?),
            skipped: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Throws the plan away and ends the run.
///
/// # Errors
///
/// The database fails.
pub fn discard(db: &Connection) -> Result<(), AxiomataError> {
    db.execute("DELETE FROM tag_plan", [])?;
    set_phase(db, None)
}

/// What writing the plan did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Applied {
    /// Notes written.
    pub written: usize,
    /// Notes the plan changed nothing on any more (already tagged and linked so).
    pub unchanged: usize,
    /// Notes that could not be written, one line each.
    pub failed: Vec<String>,
}

/// Writes the plan into the notes — the owner's step after the preview — each through `notes tag`'s own narrow path
/// (tags from the list as it is now, related links that still resolve, the planned proposals made now with the note,
/// so an accepted one is set there). A written, unchanged or skipped note leaves the plan; one that failed stays in it
/// for another try. The run's step ends either way.
///
/// # Errors
///
/// The database fails.
pub fn apply(config: &Config, db: &Mutex<Connection>) -> Result<Applied, AxiomataError> {
    let lock = || db.lock().unwrap_or_else(|poison| poison.into_inner());
    let planned: Vec<(PlannedNote, Vec<(String, String)>)> = {
        let db = lock();
        let proposals = planned_proposals(&db)?;
        plan(&db)?
            .into_iter()
            .map(|entry| {
                let pairs = proposals.get(&entry.note).cloned().unwrap_or_default();
                (entry, pairs)
            })
            .collect()
    };
    let mut applied = Applied::default();
    for (entry, proposals) in planned {
        let done = if entry.skipped.is_some() {
            true
        } else {
            let request = TagRequest {
                note: entry.note.clone(),
                tags: entry.tags,
                related: entry.related,
                proposals,
                dry_run: false,
            };
            match note::tag_note(config, db, "the vault run", &request) {
                Ok(outcome) => {
                    if outcome.written {
                        applied.written += 1;
                    } else {
                        applied.unchanged += 1;
                    }
                    true
                }
                Err(err) => {
                    applied.failed.push(format!("{}: {err}", entry.note));
                    false
                }
            }
        };
        if done {
            lock().execute("DELETE FROM tag_plan WHERE note = ?1", [&entry.note])?;
        }
    }
    set_phase(&lock(), None)?;
    Ok(applied)
}

/// The planned proposals as `[tag, description]` pairs, by note.
fn planned_proposals(
    db: &Connection,
) -> Result<std::collections::HashMap<String, Vec<(String, String)>>, AxiomataError> {
    let mut stmt = db.prepare("SELECT note, proposals FROM tag_plan")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(note, json)| (note, serde_json::from_str(&json).unwrap_or_default()))
        .collect())
}

/// What a step is expected to cost, before the owner starts it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Estimate {
    pub phase: Phase,
    pub notes: usize,
    pub bytes: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// The model the skill runs on, as the price table names it.
    pub model: Option<String>,
    /// In US dollars from the owner's price table; `None` when the model has no price there (Anthropic through the
    /// subscription, a local Ollama).
    pub cost_usd: Option<f64>,
}

/// Bytes per token of a note's text: the usual rule of thumb for German and English prose.
const BYTES_PER_TOKEN: u64 = 4;

/// How often a run's context sends a note's text again: read once, then carried through the later steps until the
/// run compacts. Deliberately on the high side — the estimate is to warn, the meter counts what it really cost.
const RESEND_FACTOR: u64 = 3;

/// Tokens a run spends per note besides its text: the instructions and the tool calls around each read.
const TOKENS_PER_NOTE: u64 = 1_500;

/// Tokens a run writes per note: one `notes tag` (or a share of the proposals) and a line of report.
const OUTPUT_PER_NOTE: u64 = 150;

/// The estimate of `phase` over `notes`, priced with the skill's model (`spend::metered_cost_usd`).
pub fn estimate(config: &Config, phase: Phase, notes: &[NoteSize]) -> Estimate {
    let bytes: u64 = notes.iter().map(|n| n.bytes).sum();
    let count = notes.len() as u64;
    let text_tokens = bytes / BYTES_PER_TOKEN;
    let (input_tokens, output_tokens) = match phase {
        // The list reads the notes once, without a call per note, and writes 30–60 proposals.
        Phase::List => (text_tokens * 2 + 20_000, 60 * OUTPUT_PER_NOTE),
        Phase::Notes => (
            text_tokens * RESEND_FACTOR + count * TOKENS_PER_NOTE,
            count * OUTPUT_PER_NOTE,
        ),
    };
    let model = skill_model(config);
    let cost_usd = crate::spend::metered_cost_usd(
        config,
        model.as_deref(),
        Some(input_tokens),
        None,
        Some(output_tokens),
    );
    Estimate {
        phase,
        notes: notes.len(),
        bytes,
        input_tokens,
        output_tokens,
        model,
        cost_usd,
    }
}

/// The model a skill without its own `model:` runs on: Anthropic's skill model on Claude Code, else the skill
/// provider's (as `skills::runner` resolves it).
fn skill_model(config: &Config) -> Option<String> {
    let provider = match config.agents.skill_harness {
        HarnessId::ClaudeCode => ProviderId::Anthropic,
        HarnessId::Axh => config.agents.skill_provider,
    };
    config
        .agents
        .providers
        .get(&provider)
        .map(|settings| settings.skill_model.trim().to_owned())
        .filter(|model| !model.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn fixture() -> (std::path::PathBuf, Config, Mutex<Connection>) {
        let root = unique_temp_dir("axiomata-tags-vault-run");
        fs::create_dir_all(root.join("A")).unwrap();
        fs::create_dir_all(root.join("Kanban")).unwrap();
        fs::write(root.join("TAGS.md"), "- #rust — Rust\n").unwrap();
        fs::write(root.join("A/one.md"), "# One\n").unwrap();
        fs::write(root.join("A/two.md"), "# Two\n").unwrap();
        fs::write(root.join("Kanban/1-board.md"), "# Board\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        let db = crate::db::open_and_migrate_at(&root.join(".t.db")).unwrap();
        (root, config, Mutex::new(db))
    }

    #[test]
    fn the_tag_list_and_the_routers_are_never_a_related_note() {
        let (root, config, db) = fixture();
        fs::write(root.join("AGENTS.md"), "# Router\n").unwrap();
        let request = TagRequest {
            note: "A/one.md".into(),
            related: vec!["TAGS".into(), "AGENTS".into(), "two".into()],
            dry_run: true,
            ..TagRequest::default()
        };
        let outcome = note::tag_note(&config, &db, "t", &request).unwrap();
        assert_eq!(outcome.related, vec!["two"]);
        assert_eq!(outcome.refused.len(), 2, "{:?}", outcome.refused);
        let names = crate::notes::note_names(&config, "tags agents two");
        assert!(
            names.iter().all(|n| n != "TAGS" && n != "AGENTS"),
            "{names:?}"
        );
    }

    #[test]
    fn a_dry_run_records_only_in_the_notes_step_and_apply_writes_the_plan() {
        let (root, config, db) = fixture();
        let dry = TagRequest {
            note: "A/one.md".into(),
            tags: vec!["rust".into(), "neu".into()],
            related: vec!["two".into()],
            dry_run: true,
            ..TagRequest::default()
        };
        let outcome = note::tag_note(&config, &db, "t", &dry).unwrap();
        let pairs = vec![("neu".to_string(), "New things".to_string())];
        assert!(
            !record(&db.lock().unwrap(), &outcome, &pairs).unwrap(),
            "no run, no plan"
        );

        set_phase(&db.lock().unwrap(), Some(Phase::Notes)).unwrap();
        let todo_before = todo(&config, &db).unwrap();
        assert_eq!(
            todo_before.notes,
            vec!["A/one.md", "A/two.md"],
            "Kanban/ and TAGS.md are no notes to tag"
        );
        assert!(record(&db.lock().unwrap(), &outcome, &pairs).unwrap());
        assert!(record_skip(&db.lock().unwrap(), "A/two.md", "name not safe").unwrap());
        assert!(
            todo(&config, &db).unwrap().notes.is_empty(),
            "a skipped note counts as done"
        );
        let planned = plan(&db.lock().unwrap()).unwrap();
        assert_eq!(planned[0].tags, vec!["rust"]);
        assert_eq!(planned[0].related, vec!["two"]);
        assert_eq!(planned[0].proposed, vec!["neu"]);
        assert_eq!(
            fs::read_to_string(root.join("A/one.md")).unwrap(),
            "# One\n",
            "the preview wrote nothing"
        );

        assert!(
            super::super::proposals::open(&db.lock().unwrap())
                .unwrap()
                .is_empty(),
            "the preview proposed nothing"
        );
        let applied = apply(&config, &db).unwrap();
        assert_eq!(applied.written, 1);
        assert!(applied.failed.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("A/one.md")).unwrap(),
            "---\ntags:\n  - rust\n---\n# One\n\n## Related\n\n- [[two]]\n"
        );
        let db = db.lock().unwrap();
        assert!(plan(&db).unwrap().is_empty() && phase(&db).unwrap().is_none());
        let proposed = super::super::proposals::open(&db).unwrap();
        assert_eq!(proposed.len(), 1, "writing the plan makes its proposals");
        assert_eq!(proposed[0].description, "New things");
        assert_eq!(proposed[0].notes, vec!["A/one.md".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_list_step_gets_every_note_and_a_failed_note_stays_planned() {
        let (root, config, db) = fixture();
        for n in 0..(TODO_BATCH + 5) {
            fs::write(root.join(format!("A/n{n:02}.md")), "# N\n").unwrap();
        }
        set_phase(&db.lock().unwrap(), Some(Phase::List)).unwrap();
        let all = todo(&config, &db).unwrap();
        assert_eq!((all.notes.len(), all.more), (TODO_BATCH + 7, 0));
        set_phase(&db.lock().unwrap(), Some(Phase::Notes)).unwrap();
        assert_eq!(todo(&config, &db).unwrap().notes.len(), TODO_BATCH);
        let outcome = TagOutcome {
            note: "A/gone.md".into(),
            tagged: vec!["rust".into()],
            ..TagOutcome::default()
        };
        record(&db.lock().unwrap(), &outcome, &[]).unwrap();
        let applied = apply(&config, &db).unwrap();
        assert_eq!(applied.failed.len(), 1);
        assert_eq!(
            plan(&db.lock().unwrap()).unwrap().len(),
            1,
            "kept for another try"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_estimate_prices_the_notes_with_the_skill_model() {
        let mut config = Config::default();
        config.agents.skill_harness = HarnessId::Axh;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap()
            .skill_model = "m".into();
        config.agents.costs.insert(
            "m".into(),
            crate::config::ModelCost {
                input_per_m: 1.0,
                cached_input_per_m: None,
                output_per_m: 2.0,
            },
        );
        let notes = vec![
            NoteSize {
                path: "a.md".into(),
                bytes: 4_000,
            };
            10
        ];
        let estimate = estimate(&config, Phase::Notes, &notes);
        assert_eq!(estimate.input_tokens, 10_000 * 3 + 15_000);
        assert_eq!(estimate.output_tokens, 1_500);
        let cost = estimate.cost_usd.unwrap();
        assert!((cost - (45_000.0 / 1e6 + 3_000.0 / 1e6)).abs() < 1e-9);
        config.agents.costs.clear();
        assert_eq!(super::estimate(&config, Phase::List, &notes).cost_usd, None);
        assert!(Phase::parse("x").is_err());
    }
}
