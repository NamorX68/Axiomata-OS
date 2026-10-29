//! The maintenance against a scratch home: aged rows and aged files, never the owner's `~/.axiomata`.

use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use super::*;
use crate::board;
use crate::ide;
use crate::test_support::unique_temp_dir;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// A scratch home with a migrated database and nothing else.
struct Scratch {
    home: PathBuf,
    db: Mutex<Connection>,
    now: DateTime<Utc>,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let home = unique_temp_dir(&format!("axiomata-test-maintenance-{label}"));
        fs::create_dir_all(&home).unwrap();
        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
        Self {
            home,
            db: Mutex::new(db),
            now: Utc::now(),
        }
    }

    fn locations(&self) -> Locations {
        Locations {
            home: self.home.clone(),
            db: self.home.join("axiomata.db"),
        }
    }

    fn run(&self, settings: &MaintenanceSettings, dry_run: bool) -> Report {
        run(&self.db, settings, &self.locations(), self.now, dry_run)
    }

    fn ago(&self, days: i64) -> String {
        (self.now - chrono::Duration::days(days)).to_rfc3339()
    }

    /// Writes `path` (and its folders) and sets its modification `days` back.
    fn file(&self, rel: &str, bytes: usize, days: u64) -> PathBuf {
        let path = self.home.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; bytes]).unwrap();
        age(&path, days);
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

/// Sets a file's or folder's modification `days` back.
fn age(path: &Path, days: u64) {
    let when = SystemTime::now() - DAY * u32::try_from(days).unwrap();
    File::open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(when))
        .unwrap();
}

fn item<'a>(report: &'a Report, key: &str) -> &'a Item {
    report
        .items
        .iter()
        .find(|item| item.key == key)
        .unwrap_or_else(|| panic!("no item {key}"))
}

fn count(db: &Mutex<Connection>, table: &str) -> i64 {
    lock(db)
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

/// A project in `folder` (it must exist).
fn add_project(db: &Connection, folder: &Path) -> i64 {
    ide::store::create_project(
        db,
        ide::NewProject {
            name: "Project".into(),
            repo_root: folder.to_path_buf(),
        },
    )
    .unwrap()
    .id
}

fn add_agent(db: &Connection, project_id: i64, name: &str) -> ide::Agent {
    ide::agent_store::create_agent(
        db,
        ide::NewAgent {
            project_id,
            fields: ide::AgentFields {
                name: name.into(),
                harness: ide::Harness::Glut,
                command: String::new(),
                model: None,
                env: String::new(),
            },
        },
    )
    .unwrap()
}

fn insert_run(db: &Connection, started_at: &str, status: &str) {
    db.execute(
        "INSERT INTO runs (skill_name, backend, status, duration_ms, stdout, stderr, started_at, finished_at, source) \
         VALUES ('s', 'glut', ?1, 0, '', '', ?2, ?2, 'manual')",
        params![status, started_at],
    )
    .unwrap();
}

#[test]
fn old_rows_go_by_their_rule_and_the_rest_stay() {
    let s = Scratch::new("rows");
    {
        let mut db = lock(&s.db);
        // Runs: three old, one new; the newest two stay whatever their age, and an old `running` one stays too.
        insert_run(&db, &s.ago(100), "success");
        insert_run(&db, &s.ago(120), "failed");
        insert_run(&db, &s.ago(130), "success");
        insert_run(&db, &s.ago(1), "success");
        insert_run(&db, &s.ago(200), "running");

        db.execute(
            "INSERT INTO routines (name, cron_expr, target_type, target, enabled, created_at, updated_at) \
             VALUES ('r', '0 * * * * *', 'prompt', 'hi', 1, ?1, ?1)",
            [s.ago(400)],
        )
        .unwrap();
        for days in [40, 5] {
            db.execute(
                "INSERT INTO routine_runs (routine_id, scheduled_for, fired_at, status) VALUES (1, ?1, ?1, 'success')",
                [s.ago(days)],
            )
            .unwrap();
        }
        for days in [91, 89] {
            db.execute(
                "INSERT INTO chat_turns (session_id, mode, created_at) VALUES ('x', 'chat', ?1)",
                [s.ago(days)],
            )
            .unwrap();
            db.execute(
                "INSERT INTO web_searches (provider, searched_at) VALUES ('exa', ?1)",
                [s.ago(days)],
            )
            .unwrap();
        }
        for days in [400, 300] {
            db.execute(
                "INSERT INTO session_spend (agent_id, tokens, steps, at) VALUES (1, 1, 1, ?1)",
                [s.ago(days)],
            )
            .unwrap();
        }
        // A stamp SQLite cannot read stays: neither older nor newer.
        db.execute(
            "INSERT INTO chat_turns (session_id, mode, created_at) VALUES ('x', 'chat', 'garbage')",
            [],
        )
        .unwrap();

        // Card history: an old event of a living card stays; the events of a card archived long ago go, those of
        // one archived lately stay.
        let board = board::store::create_board(&mut db, "B").unwrap();
        let column = board::store::list_columns(&db, board.id)
            .unwrap()
            .into_iter()
            .find(|c| c.stage.is_none())
            .unwrap();
        let mut cards = Vec::new();
        for title in ["living", "archived long ago", "archived lately"] {
            let card = board::store::create_card(
                &db,
                &board::NewCard {
                    column_id: column.id,
                    fields: board::CardFields {
                        title: title.to_owned(),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
            cards.push(card.id);
        }
        db.execute(
            "UPDATE cards SET archived_at = ?2 WHERE id = ?1",
            params![cards[1], s.ago(100)],
        )
        .unwrap();
        db.execute(
            "UPDATE cards SET archived_at = ?2 WHERE id = ?1",
            params![cards[2], s.ago(10)],
        )
        .unwrap();
        db.execute("DELETE FROM card_events", []).unwrap();
        for card in &cards {
            db.execute(
                "INSERT INTO card_events (card_id, at, actor, kind) VALUES (?1, ?2, 'human:o', 'note')",
                params![card, s.ago(500)],
            )
            .unwrap();
        }
    }
    let settings = MaintenanceSettings {
        runs_keep: 2,
        ..MaintenanceSettings::default()
    };

    let dry = s.run(&settings, true);
    assert!(dry.dry_run);
    assert_eq!(item(&dry, "runs").count, 5);
    assert_eq!(item(&dry, "runs").freeable_count, 2);
    assert_eq!(item(&dry, "routine_runs").freeable_count, 1);
    assert_eq!(item(&dry, "chat_turns").freeable_count, 1);
    assert_eq!(item(&dry, "web_searches").freeable_count, 1);
    assert_eq!(item(&dry, "session_spend").freeable_count, 1);
    assert_eq!(item(&dry, "card_events").freeable_count, 1);
    assert_eq!(count(&s.db, "runs"), 5, "a dry run deletes nothing");
    assert_eq!(dry.last_run, None, "nor does it count as a run");

    let done = s.run(&settings, false);
    assert!(!done.dry_run);
    assert_eq!(item(&done, "runs").freeable_count, 2);
    assert_eq!(count(&s.db, "runs"), 3);
    assert_eq!(count(&s.db, "routine_runs"), 1);
    assert_eq!(count(&s.db, "chat_turns"), 2);
    assert_eq!(count(&s.db, "web_searches"), 1);
    assert_eq!(count(&s.db, "session_spend"), 1);
    assert_eq!(count(&s.db, "card_events"), 2);
    let still_running: i64 = lock(&s.db)
        .query_row(
            "SELECT COUNT(*) FROM runs WHERE status = 'running'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(still_running, 1);
    assert!(item(&done, "database").error.is_none());

    // The run is recorded, and a second pass finds nothing left.
    assert_eq!(done.last_run, Some(s.now));
    assert_eq!(last_run(&lock(&s.db)), Some(s.now));
    let again = s.run(&settings, true);
    assert_eq!(item(&again, "runs").freeable_count, 0);
    assert_eq!(again.last_run, Some(s.now));
}

#[test]
fn old_app_logs_extra_backups_and_orphaned_status_folders_go() {
    let s = Scratch::new("files");
    s.file("logs/app.log.2026-01-01", 100, 20);
    s.file("logs/app.log.2026-10-28", 50, 1);
    // The run log is never the app log's rule's, however old.
    s.file("logs/runs.log", 70, 60);

    // Three copies of the database (with side files) and one of the config: the two newest of each stay.
    s.file("axiomata.db.backup-a", 10, 30);
    s.file("axiomata.db.backup-a-wal", 1, 30);
    s.file("axiomata.db.backup-a-shm", 1, 30);
    s.file("axiomata.db.backup-b", 10, 20);
    s.file("axiomata.db.backup-c", 10, 10);
    s.file("config.toml.backup-x", 5, 90);
    s.file("config.toml", 5, 90);

    {
        let db = lock(&s.db);
        let project = add_project(&db, &s.home);
        for name in ["one", "two", "three"] {
            add_agent(&db, project, name);
        }
        ide::agent_store::delete_agent(&db, 2).unwrap();
    }
    s.file("agent-events/1/state", 3, 0);
    s.file("agent-events/2/state", 3, 0);
    // Newer than anything the database has handed out: an agent being created right now.
    s.file("agent-events/9/state", 3, 0);
    s.file("agent-events/notes/x", 3, 0);

    let settings = MaintenanceSettings::default();
    let dry = s.run(&settings, true);
    assert_eq!(item(&dry, "app_log").count, 2);
    assert_eq!(item(&dry, "app_log").freeable_bytes, Some(100));
    assert_eq!(item(&dry, "backups").count, 6);
    assert_eq!(item(&dry, "backups").freeable_count, 3);
    assert_eq!(item(&dry, "backups").freeable_bytes, Some(12));
    assert_eq!(item(&dry, "agent_events").freeable_count, 1);
    assert_eq!(item(&dry, "runs_log").bytes, Some(70));
    assert!(dry.freeable_bytes >= 100 + 12 + 3);
    assert!(s.home.join("axiomata.db.backup-a").exists());

    s.run(&settings, false);
    assert!(!s.home.join("logs/app.log.2026-01-01").exists());
    assert!(s.home.join("logs/app.log.2026-10-28").exists());
    assert!(s.home.join("logs/runs.log").exists());
    for gone in [
        "axiomata.db.backup-a",
        "axiomata.db.backup-a-wal",
        "axiomata.db.backup-a-shm",
    ] {
        assert!(!s.home.join(gone).exists(), "{gone}");
    }
    for kept in [
        "axiomata.db.backup-b",
        "axiomata.db.backup-c",
        "config.toml.backup-x",
        "config.toml",
        "axiomata.db",
    ] {
        assert!(s.home.join(kept).exists(), "{kept}");
    }
    assert!(s.home.join("agent-events/1").exists());
    assert!(!s.home.join("agent-events/2").exists());
    assert!(s.home.join("agent-events/9").exists());
    assert!(s.home.join("agent-events/notes").exists());
}

#[test]
fn only_old_claude_sessions_go_and_the_login_folder_stays_whole() {
    let s = Scratch::new("claude");
    // A project folder with one old and one fresh session keeps the fresh one; one with only old sessions goes.
    s.file("claude/projects/-a/old.jsonl", 10, 40);
    s.file("claude/projects/-a/new.jsonl", 10, 2);
    s.file("claude/projects/-b/sub/old.jsonl", 10, 40);
    age(&s.home.join("claude/projects/-b/sub"), 40);
    s.file("claude/projects/-b/old.jsonl", 10, 40);
    s.file("claude/runs/old", 10, 40);
    s.file("claude/shell-snapshots/new", 10, 1);
    s.file("claude/session-env/old/env", 10, 40);
    age(&s.home.join("claude/session-env/old"), 40);
    // Anything else in the login folder is never touched, however old.
    s.file("claude/.credentials.json", 10, 400);
    s.file("claude/backups/old", 10, 400);
    s.file("claude/sessions/old", 10, 400);

    let settings = MaintenanceSettings::default();
    let dry = s.run(&settings, true);
    let claude = item(&dry, "claude_sessions");
    assert_eq!(claude.freeable_count, 5, "{claude:?}");
    assert_eq!(claude.freeable_bytes, Some(50));

    s.run(&settings, false);
    assert!(!s.home.join("claude/projects/-a/old.jsonl").exists());
    assert!(s.home.join("claude/projects/-a/new.jsonl").exists());
    assert!(
        !s.home.join("claude/projects/-b").exists(),
        "an emptied project folder goes"
    );
    assert!(!s.home.join("claude/runs/old").exists());
    assert!(
        s.home.join("claude/runs").is_dir(),
        "the folder itself stays"
    );
    assert!(s.home.join("claude/shell-snapshots/new").exists());
    assert!(!s.home.join("claude/session-env/old").exists());
    for kept in [
        "claude/.credentials.json",
        "claude/backups/old",
        "claude/sessions/old",
    ] {
        assert!(s.home.join(kept).exists(), "{kept}");
    }
}

#[test]
fn a_link_in_place_of_a_cleaned_folder_is_never_followed() {
    let s = Scratch::new("links");
    let outside = unique_temp_dir("axiomata-test-maintenance-outside");
    fs::create_dir_all(&outside).unwrap();
    for name in ["old.jsonl", "app.log.2020-01-01"] {
        let path = outside.join(name);
        fs::write(&path, "the owner's").unwrap();
        age(&path, 400);
    }
    fs::create_dir_all(s.home.join("claude")).unwrap();
    std::os::unix::fs::symlink(&outside, s.home.join("claude/runs")).unwrap();
    std::os::unix::fs::symlink(&outside, s.home.join("logs")).unwrap();
    // A backup that is a link is not a backup.
    std::os::unix::fs::symlink(outside.join("old.jsonl"), s.home.join("x.backup-1")).unwrap();
    std::os::unix::fs::symlink(outside.join("old.jsonl"), s.home.join("x.backup-2")).unwrap();
    std::os::unix::fs::symlink(outside.join("old.jsonl"), s.home.join("x.backup-3")).unwrap();

    let settings = MaintenanceSettings {
        backups_keep: 1,
        ..MaintenanceSettings::default()
    };
    let done = s.run(&settings, false);
    assert!(outside.join("old.jsonl").exists());
    assert!(outside.join("app.log.2020-01-01").exists());
    assert_eq!(item(&done, "backups").count, 0);
    assert!(s.home.join("x.backup-1").symlink_metadata().is_ok());
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn rules_out_of_range_or_a_foreign_home_refuse_the_whole_pass() {
    let s = Scratch::new("refused");
    s.file("logs/app.log.2020-01-01", 10, 400);
    let zero = MaintenanceSettings {
        app_log_days: 0,
        ..MaintenanceSettings::default()
    };
    let refused = s.run(&zero, false);
    assert!(refused.error.is_some());
    assert!(refused.items.is_empty());
    assert!(s.home.join("logs/app.log.2020-01-01").exists());
    assert_eq!(last_run(&lock(&s.db)), None);

    // A home without Axiomata's database is not Axiomata's.
    let foreign = Locations {
        home: s.home.join("logs"),
        db: s.home.join("logs").join("axiomata.db"),
    };
    let refused = run(
        &s.db,
        &MaintenanceSettings::default(),
        &foreign,
        s.now,
        false,
    );
    assert!(refused.error.is_some());
    assert!(s.home.join("logs/app.log.2020-01-01").exists());
}
