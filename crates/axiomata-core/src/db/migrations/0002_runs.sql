-- Skill run history. One row per skill invocation (via the CLI, a Tauri
-- command, or later a routine). The filesystem stays the source of truth for
-- the skills themselves; this table only records what happened when one ran,
-- so the UI and CLI can show a history without re-reading log files.
--
-- Mirrors the JSONL lines appended to `~/.axiomata/logs/runs.log`.
CREATE TABLE IF NOT EXISTS runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Skill identity as resolved at run time.
    skill_name   TEXT    NOT NULL,
    -- Agent backend used: 'claude-code' or 'ollama'.
    backend      TEXT    NOT NULL,
    -- Outcome: 'success' or 'failed'.
    status       TEXT    NOT NULL,
    -- Process exit code (Claude Code) or synthetic code (Ollama). NULL when the
    -- agent never produced a result at all (spawn failure, timeout, API error).
    exit_code    INTEGER,
    -- Wall-clock duration of the run in milliseconds.
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    -- Captured agent output. Empty strings rather than NULL for easy display.
    stdout       TEXT    NOT NULL DEFAULT '',
    stderr       TEXT    NOT NULL DEFAULT '',
    -- Set only for failures that produced no agent result (e.g. the timeout or
    -- spawn error message); NULL on success and on non-zero-exit failures.
    error        TEXT,
    -- RFC 3339 timestamps.
    started_at   TEXT    NOT NULL,
    finished_at  TEXT    NOT NULL
);

-- The common query is "the N most recent runs", optionally filtered by skill.
CREATE INDEX IF NOT EXISTS idx_runs_started_at ON runs (started_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_skill_name ON runs (skill_name);
