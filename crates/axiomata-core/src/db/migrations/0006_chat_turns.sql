-- Dashboard-assistant turns (provider-hardening checkpoint 4). Chat turns were
-- never logged before ("the reply is not recorded in the run log"), so a paid
-- provider's chat spend was completely invisible in-app.
--
-- Kept separate from `runs` rather than folded in: a chat turn has no skill
-- name, carries a `session_id` / resume chain and a mode, and its lifecycle
-- (multi-turn conversation) is genuinely different from a one-shot skill run.
-- The spend rollup (checkpoint 5) UNIONs the `cost_usd` of both tables.
CREATE TABLE IF NOT EXISTS chat_turns (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    -- The `session_id` claude returned for this turn; the resume key for the
    -- next turn of the same conversation.
    session_id    TEXT    NOT NULL,
    -- 'chat' (read-mostly) or 'instruct' (may edit workspace files).
    mode          TEXT    NOT NULL,
    -- Model-routing provider active when the turn ran ('anthropic' /
    -- 'open_router' / 'ollama'); NULL only if it could not be determined.
    provider      TEXT,
    -- The resolved `claude --model`, or NULL when the CLI default was used.
    model         TEXT,
    -- 1 when claude reported `is_error` for the turn.
    is_error      INTEGER NOT NULL DEFAULT 0,
    -- `total_cost_usd` from the JSON envelope; NULL for the subscription path.
    cost_usd      REAL,
    input_tokens  INTEGER,
    output_tokens INTEGER,
    num_turns     INTEGER,
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    -- RFC 3339 timestamp of when the turn completed.
    created_at    TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_turns_created_at ON chat_turns (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_turns_provider_created_at ON chat_turns (provider, created_at DESC);
