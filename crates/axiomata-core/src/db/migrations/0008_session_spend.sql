-- What the studio's agent sessions used (a2a.md, CP-A8c: the limit of a plan and of a day).
--
-- A session's usage is read live from its harness (a transcript, the Opencode database) and disappears with the session
-- when its card is integrated, so a limit over a *plan* or a *day* has nothing to add up unless every look writes what it
-- found down. Each row is the **increase** since the previous look of that session; the sum over a session is its usage,
-- over a plan or over today is what the limit is held against. Append-only: nothing here is ever updated.
CREATE TABLE IF NOT EXISTS session_spend (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    -- The IDE agent session (AUTOINCREMENT ids are never reused, so a deleted session's rows stay its own).
    agent_id   INTEGER NOT NULL,
    -- The plan the session worked for; NULL for a card that belongs to none.
    plan_id    INTEGER,
    -- Tokens (input, output, cache writes), steps (tool calls) and, for an engine paid per token with a priced model,
    -- dollars. NULL cost is "not metered", never "free".
    tokens     INTEGER NOT NULL DEFAULT 0,
    steps      INTEGER NOT NULL DEFAULT 0,
    cost_usd   REAL,
    -- RFC 3339 timestamp of the look.
    at         TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_session_spend_plan  ON session_spend (plan_id);
CREATE INDEX IF NOT EXISTS idx_session_spend_agent ON session_spend (agent_id);
CREATE INDEX IF NOT EXISTS idx_session_spend_at    ON session_spend (at);
