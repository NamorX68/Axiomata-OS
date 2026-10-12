-- The agents' permission requests the owner answers in the app (docs/plans/permission-asks.md, board card #88).
--
-- A Claude Code session's PermissionRequest hook writes one row and waits for its decision; only the app's own
-- command, acting as the owner, writes the decision. `decision` is NULL while the request is open, otherwise 'allow',
-- 'deny' or 'terminal' (handed back to the pane's own prompt, also when the hook gave up waiting).
CREATE TABLE permission_asks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id    INTEGER NOT NULL,
    tool        TEXT    NOT NULL,
    detail      TEXT    NOT NULL,
    asked_at    TEXT    NOT NULL,
    decision    TEXT,
    decided_by  TEXT,
    decided_at  TEXT
);
CREATE INDEX permission_asks_open ON permission_asks (agent_id, decision);
