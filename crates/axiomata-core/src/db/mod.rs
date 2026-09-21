//! SQLite connection setup and schema migrations for `~/.axiomata/axiomata.db`.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::error::AxiomataError;
use crate::paths;

/// Ordered list of schema migrations as `(version, sql)`. `version` must be
/// unique and strictly increasing. Once released, a migration is never
/// edited or reordered — only new ones are appended.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("migrations/0001_init.sql")),
    (2, include_str!("migrations/0002_runs.sql")),
    (3, include_str!("migrations/0003_routines.sql")),
    (4, include_str!("migrations/0004_runs_source.sql")),
    (5, include_str!("migrations/0005_runs_cost.sql")),
    (6, include_str!("migrations/0006_chat_turns.sql")),
    (7, include_str!("migrations/0007_runs_model.sql")),
    // Owned by the `axiomata-board` crate rather than a local .sql file: the
    // board is embeddable elsewhere (the agentic IDE's task board, M7.5) and
    // has to carry its own initial schema. Frozen from here on — a later board
    // schema change arrives as its own constant and its own migration number.
    (8, axiomata_board::SCHEMA_SQL_V1),
    // Owned by `axiomata-ide` for the same reason as 0008 above: the agentic
    // IDE is meant to be extractable, so it carries its own initial schema.
    // Frozen from here on.
    (9, axiomata_ide::SCHEMA_SQL_V1),
    // Agent profiles (M7.2 CP4). A new constant rather than an edit to
    // migration 9: that one has already run everywhere and never runs again.
    (10, axiomata_ide::SCHEMA_SQL_V2),
    // Worktree, branch and port per agent (M7.2 CP5). An ALTER, which is
    // exactly why the runner's one-transaction-per-migration matters.
    (11, axiomata_ide::SCHEMA_SQL_V3),
];

/// Opens (creating if necessary) the SQLite database at
/// `~/.axiomata/axiomata.db` and applies any migrations that haven't run yet.
pub fn open_and_migrate() -> Result<Connection, AxiomataError> {
    open_and_migrate_at(&paths::db_path())
}

/// Same as [`open_and_migrate`], but against an explicit path — used by
/// tests so they don't touch the real `~/.axiomata/axiomata.db`.
pub fn open_and_migrate_at(path: &Path) -> Result<Connection, AxiomataError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut conn = Connection::open(path)?;

    // SQLite ignores every `REFERENCES` clause unless this is switched on per
    // connection. Enabling it is process-wide for this one shared connection;
    // today only `routine_runs` (migration 0003) declares foreign keys — it
    // relies on this for the cascade on routine deletion and the SET NULL on
    // `run_id`. Set before any migration transaction (a pragma inside a
    // transaction is silently ignored).
    conn.pragma_update(None, "foreign_keys", true)?;

    // Write-ahead logging, set before anything else touches the file.
    //
    // The app holds one shared `Mutex<Connection>`, which serialises *this
    // process* and nothing else. `axiomata-cli` is a second process on the
    // same file, so in the default rollback-journal mode a CLI write while the
    // app is running fails with SQLITE_BUSY immediately — and rusqlite's
    // default DEFERRED transaction can hit that on write-upgrade halfway
    // through, which is not safely retryable. That was already true of
    // `routines add`; nobody had happened to run one at the wrong moment.
    // WAL lets a writer and readers coexist, and the busy timeout makes two
    // writers queue instead of one failing outright.
    //
    // Visible consequence: `axiomata.db-wal` and `axiomata.db-shm` appear
    // alongside the database. `journal_mode` is persistent — it is stored in
    // the file header, so this is a one-time conversion, not a per-connection
    // setting. It is queried rather than `pragma_update`d because SQLite
    // answers with the resulting mode.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;

    // Bookkeeping table for applied migration versions. Created
    // unconditionally (idempotent) rather than as migration 0001 itself,
    // since a migration can't track whether it already ran without it.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
        [],
    )?;

    // Each migration runs inside its own IMMEDIATE transaction, and the
    // version check happens *inside* it rather than once up front.
    //
    // This matters because two processes routinely share this file — the app
    // and `axiomata-cli` — and never more so than right after a new migration
    // is added, when both may still be at the old version. Reading the version
    // outside a write lock let both processes decide to apply the same
    // migration, then both run it. `CREATE TABLE IF NOT EXISTS` survives that;
    // the `ALTER TABLE ADD COLUMN` and backfill statements in migrations 4, 5
    // and 7 do not, and without a transaction a half-applied one leaves no way
    // back. With the lock taken first, the loser blocks on `busy_timeout`,
    // then sees the version already recorded and skips.
    for &(version, sql) in MIGRATIONS {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let applied: u32 = tx.query_row(
            "SELECT COUNT(*) FROM schema_version WHERE version = ?1",
            [version],
            |row| row.get(0),
        )?;
        if applied == 0 {
            tx.execute_batch(sql)
                .map_err(|source| AxiomataError::Migration { version, source })?;
            tx.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                [version],
            )?;
        }
        tx.commit()?;
    }

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    /// The one property the whole scheme rests on: a version number, once
    /// shipped, always means the same migration. A duplicate or a number that
    /// goes backwards would let a database record a version as applied and
    /// then never run the statement that actually belongs to it.
    #[test]
    fn migrations_are_unique_and_increasing() {
        let mut previous = 0;
        for &(version, _) in MIGRATIONS {
            assert!(
                version > previous,
                "migration {version} does not come after {previous} — the list must be strictly increasing"
            );
            previous = version;
        }
    }

    #[test]
    fn open_and_migrate_applies_once_and_is_idempotent_on_reopen() {
        let temp_db = unique_temp_dir("axiomata-test-db").with_extension("db");

        {
            let conn = open_and_migrate_at(&temp_db).expect("first open should succeed");
            let version: u32 = conn
                .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                    row.get(0)
                })
                .unwrap();
            // Derived from the list rather than hard-coded: what this asserts
            // is "every migration ran", and spelling the number out again here
            // only meant editing two places each time one was added.
            // `migrations_are_unique_and_increasing` below is what guards the
            // list itself.
            assert_eq!(
                version,
                MIGRATIONS.last().expect("migrations must not be empty").0
            );

            // Migration 0001's DDL actually ran, not just the bookkeeping.
            conn.execute(
                "INSERT INTO app_meta (key, value) VALUES ('probe', 'ok')",
                [],
            )
            .expect("app_meta table should exist");

            // Migration 0002's DDL ran too.
            conn.execute(
                "INSERT INTO runs \
                 (skill_name, backend, status, exit_code, duration_ms, \
                  started_at, finished_at) \
                 VALUES ('probe', 'ollama', 'success', 0, 12, \
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("runs table should exist");

            // Migration 0003's DDL ran too.
            conn.execute(
                "INSERT INTO routines \
                 (name, cron_expr, target_type, target, enabled, \
                  next_fire_at, created_at, updated_at) \
                 VALUES ('probe', '0 */2 * * * *', 'skill', 'example-skill', 1, \
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', \
                         '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("routines table should exist");
            conn.execute(
                "INSERT INTO routine_runs \
                 (routine_id, run_id, scheduled_for, fired_at, status) \
                 VALUES (1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', \
                         'success')",
                [],
            )
            .expect("routine_runs table should exist");

            // Migration 0004's DDL ran too, and backfills existing rows to
            // 'manual' rather than leaving them NULL.
            let source: String = conn
                .query_row(
                    "SELECT source FROM runs WHERE skill_name = 'probe'",
                    [],
                    |row| row.get(0),
                )
                .expect("runs.source column should exist");
            assert_eq!(source, "manual");

            // Migration 0005's columns exist and default to NULL for a row
            // inserted without them.
            let cost: Option<f64> = conn
                .query_row(
                    "SELECT cost_usd FROM runs WHERE skill_name = 'probe'",
                    [],
                    |row| row.get(0),
                )
                .expect("runs.cost_usd column should exist");
            assert_eq!(cost, None);

            // Migration 0006's table exists.
            conn.execute(
                "INSERT INTO chat_turns \
                 (session_id, mode, provider, model, is_error, created_at) \
                 VALUES ('s-1', 'chat', 'anthropic', NULL, 0, \
                         '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("chat_turns table should exist");

            // Migration 0007's column exists.
            conn.execute(
                "UPDATE runs SET model = 'deepseek/deepseek-v4-flash-0731' \
                 WHERE skill_name = 'probe'",
                [],
            )
            .expect("runs.model column should exist");

            // Migration 0008 (the board crate's schema) ran too. Inserting a
            // card exercises the composite foreign key as well as the tables.
            conn.execute(
                "INSERT INTO boards (id, name, created_at, updated_at) \
                 VALUES (1, 'probe', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("boards table should exist");
            conn.execute(
                "INSERT INTO board_columns (id, board_id, name, position, maps_to_status) \
                 VALUES (1, 1, 'Offen', 1.0, 'open')",
                [],
            )
            .expect("board_columns table should exist");
            conn.execute(
                "INSERT INTO cards \
                 (board_id, column_id, position, title, created_at, updated_at) \
                 VALUES (1, 1, 1.0, 'probe', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("cards table should exist");

            // Migration 0009 (the IDE crate's schema) ran too.
            conn.execute(
                "INSERT INTO projects (name, repo_root, created_at) \
                 VALUES ('probe', '/tmp/probe', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("projects table should exist");
        }

        {
            // Re-opening must not re-apply any migration or error out.
            let conn = open_and_migrate_at(&temp_db).expect("second open should succeed");
            let applied_count: u32 = conn
                .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
                .unwrap();
            // One row per migration and not one more: a second open must not
            // re-apply anything. Derived from the list for the same reason as
            // the version assertion above.
            assert_eq!(applied_count as usize, MIGRATIONS.len());

            let probe_value: String = conn
                .query_row(
                    "SELECT value FROM app_meta WHERE key = 'probe'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(probe_value, "ok");
        }

        let _ = fs::remove_file(&temp_db);
    }
}
