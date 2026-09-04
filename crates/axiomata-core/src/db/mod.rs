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

    let conn = Connection::open(path)?;

    // SQLite ignores every `REFERENCES` clause unless this is switched on per
    // connection. Enabling it is process-wide for this one shared connection;
    // today only `routine_runs` (migration 0003) declares foreign keys — it
    // relies on this for the cascade on routine deletion and the SET NULL on
    // `run_id`. Set before any migration transaction (a pragma inside a
    // transaction is silently ignored).
    conn.pragma_update(None, "foreign_keys", true)?;

    // Bookkeeping table for applied migration versions. Created
    // unconditionally (idempotent) rather than as migration 0001 itself,
    // since a migration can't track whether it already ran without it.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
        [],
    )?;

    let current_version: u32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            conn.execute_batch(sql)
                .map_err(|source| AxiomataError::Migration { version, source })?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                [version],
            )?;
        }
    }

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

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
            assert_eq!(version, 4);

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
        }

        {
            // Re-opening must not re-apply any migration or error out.
            let conn = open_and_migrate_at(&temp_db).expect("second open should succeed");
            let applied_count: u32 = conn
                .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(applied_count, 4);

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
