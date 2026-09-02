-- Scheduled routines and their firing history (milestone M3).
--
-- A routine is a cron schedule bound to a target: either a named skill from
-- `~/.axiomata/skills/`, or a raw prompt sent straight to an agent backend.
-- A single background poll loop (`routines::scheduler`) checks `next_fire_at`
-- roughly every 30 seconds and fires whatever is due.
--
-- `next_fire_at` is authoritative and persisted: it is loaded from this table
-- on startup and never recomputed from scratch there, so restarting the app
-- can neither double-fire nor lose a routine. A `next_fire_at` already in the
-- past at startup is rolled forward without firing (a 'missed' history row is
-- written instead) -- routines do not catch up on time they were offline for.

CREATE TABLE IF NOT EXISTS routines (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Human-facing label, also the handle used to disambiguate in the UI.
    name          TEXT    NOT NULL UNIQUE,
    -- Cron expression exactly as the user entered it. 6-7 fields, seconds
    -- required (the `cron` crate's native format), e.g. '0 */2 * * * *'.
    cron_expr     TEXT    NOT NULL,
    -- What to run: 'skill' (then `target` is a skill name) or
    -- 'prompt' (then `target` is the literal prompt text).
    target_type   TEXT    NOT NULL,
    target        TEXT    NOT NULL,
    -- Backend override: 'claude-code' or 'ollama'. NULL means "use the skill's
    -- own declared backend" for a skill target, or the config default for a
    -- prompt target.
    backend       TEXT,
    -- 0/1. A disabled routine keeps its `next_fire_at` but is skipped by the
    -- poll loop.
    enabled       INTEGER NOT NULL DEFAULT 1,
    -- RFC 3339. The next instant this routine should fire. NULL only if the
    -- cron expression has no further occurrence at all.
    next_fire_at  TEXT,
    -- RFC 3339 of the most recent successful or failed fire; NULL if never
    -- fired. Convenience for the derived status and the UI.
    last_fired_at TEXT,
    -- RFC 3339 bookkeeping.
    created_at    TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL
);

-- The poll loop's hot query is "enabled routines due at or before now".
CREATE INDEX IF NOT EXISTS idx_routines_due ON routines (enabled, next_fire_at);

-- One row per firing attempt (including ones skipped as 'missed' at startup).
-- The actual agent execution is recorded in `runs`; this table only adds the
-- scheduling context and links to it.
CREATE TABLE IF NOT EXISTS routine_runs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    routine_id    INTEGER NOT NULL REFERENCES routines (id) ON DELETE CASCADE,
    -- The `runs.id` produced by executing the target. NULL when no execution
    -- happened: a 'missed' catch-up row, or a fire that failed before the
    -- runner produced a record (e.g. the target skill no longer exists).
    run_id        INTEGER REFERENCES runs (id),
    -- RFC 3339. The `next_fire_at` value this attempt was satisfying.
    scheduled_for TEXT    NOT NULL,
    -- RFC 3339. When the scheduler actually acted on it.
    fired_at      TEXT    NOT NULL,
    -- 'success', 'failed', or 'missed'.
    status        TEXT    NOT NULL,
    -- Error message, or a short note for 'missed' rows; NULL otherwise.
    detail        TEXT
);

-- The common query is "recent firings of routine X".
CREATE INDEX IF NOT EXISTS idx_routine_runs_routine
    ON routine_runs (routine_id, fired_at DESC);
