-- IDE projects (milestone M7.1).
--
-- A project is a name and a folder, plus the dock layout the user left behind
-- in it. Everything else the agentic IDE will need — agents, worktrees, plan
-- steps, the mailbox — is deliberately NOT here: those tables arrive with
-- their own migrations in M7.2 and M7.4, once their shape is known rather
-- than guessed. A frozen schema must not contain a hypothesis.
--
-- Two load-bearing choices:
--
--   * `repo_root` is UNIQUE. From M7.2 a project owns a set of git worktrees
--     under ~/.axiomata/worktrees/<project>/; two projects on one folder would
--     be two layouts quarrelling over the same worktrees.
--   * `layout_json` is opaque to Rust and nullable. The dock tree belongs to
--     the frontend (`apps/dashboard/src/ide/layout.ts`), exactly as a tile's
--     config belongs to it in dashboard.json — Rust stores the text and has no
--     opinion about it. NULL means "never opened", which is what triggers the
--     starting layout.

CREATE TABLE IF NOT EXISTS projects (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL,
    -- Absolute and canonicalised by the store before it ever gets here, so
    -- the UNIQUE constraint compares like with like: `~/x`, `./x` and
    -- `/Users/me/x` must not become three different projects.
    repo_root TEXT NOT NULL UNIQUE,
    -- The frontend's serialised dock tree. NULL = never opened.
    layout_json TEXT,
    -- RFC 3339 bookkeeping, like every other timestamp in this database.
    created_at     TEXT NOT NULL,
    last_opened_at TEXT
);

-- No index on purpose. The project list is sorted by `last_opened_at`, but a
-- person has a dozen projects, not a million; SQLite scans that faster than it
-- would walk an index, and an index here would be cargo cult rather than a
-- measurement. (Contrast `board_columns`, where a board's columns are read on
-- every single render.)
