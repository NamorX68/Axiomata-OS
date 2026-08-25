-- Minimal application-metadata key/value table. Not consumed by anything in
-- M0 itself; its purpose here is twofold: prove the migration runner applies
-- arbitrary DDL (not just the bookkeeping `schema_version` table), and give
-- later milestones a place to persist small app-level facts (e.g. a
-- first-run timestamp) without a schema change.
CREATE TABLE IF NOT EXISTS app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
