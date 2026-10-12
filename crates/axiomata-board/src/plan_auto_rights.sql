-- Plans whose working sessions may do everything but push (docs/plans/plan-auto-rights.md, board card #89): the owner
-- switches it on with the plan's approval, after reading its cards, and may switch it off again; a session started
-- afterwards gets the rights as they are then. Beside `plans` like `plan_origin`: a row here means "on".
CREATE TABLE IF NOT EXISTS plan_auto_rights (
    plan_id INTEGER PRIMARY KEY REFERENCES plans (id) ON DELETE CASCADE,
    at TEXT NOT NULL
);
