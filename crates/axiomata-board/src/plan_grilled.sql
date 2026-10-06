-- Plans whose goal the owner sharpened with a grilling session (docs/plans/a2a.md, "Plan bearbeiten und grillen").
--
-- Written when the owner takes a proposed goal over, read for one thing: the approval says "this plan was not grilled" for
-- the plans without a row. A hint, never a gate — grilling can be skipped. Kept beside `plans` so the plan model stays.
CREATE TABLE IF NOT EXISTS plan_grilled (
    plan_id INTEGER PRIMARY KEY REFERENCES plans (id) ON DELETE CASCADE,
    at      TEXT NOT NULL
);
