-- The integration line of a plan (docs/plans/a2a.md, CP-A8).
--
-- A plan that runs by itself has its own branch next to the project's: the reviewer's yes integrates a card into it,
-- and the cards that need that card start from it. `project_id` is the project (repository) the plan's sessions run
-- in — the IDE's table, so no foreign key (A11) —, `base_branch` the branch the line was cut from, set when the line is
-- made. `integrated_at` says a card's work is on the line: not yet in the main line (that is `taken_over_at`), but what
-- a card that builds on it starts from.
ALTER TABLE plans ADD COLUMN project_id INTEGER;
ALTER TABLE plans ADD COLUMN base_branch TEXT;
ALTER TABLE cards ADD COLUMN integrated_at TEXT;
