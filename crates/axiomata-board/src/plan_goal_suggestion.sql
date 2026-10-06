-- A sharper goal for a plan, proposed by a session that grilled it (docs/plans/a2a.md, "Plan bearbeiten und grillen", Q4).
--
-- The goal of a plan is the owner's own words and no agent writes it (A7). A grilling session therefore only *proposes*
-- one: it lands here, the owner reads it in the planning panel and either takes it over (it becomes the goal and the row
-- goes) or discards it. One row per plan; a second proposal replaces the first. Kept beside `plans` rather than in it, so
-- the plan model stays as it was.
CREATE TABLE IF NOT EXISTS plan_goal_suggestions (
    plan_id INTEGER PRIMARY KEY REFERENCES plans (id) ON DELETE CASCADE,
    goal    TEXT NOT NULL,
    at      TEXT NOT NULL
);
