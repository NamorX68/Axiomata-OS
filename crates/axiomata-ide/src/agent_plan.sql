-- The plan a planner session was started for (docs/plans/a2a.md, CP-A7).
--
-- Like `card_id`, written by the studio when it makes the session, and no foreign key: the plan lives in the board's
-- tables, which this crate does not know (A11). A planner's worktree is a detached checkout of the project's state when
-- it started (`start_ref`, already there for the reviewer), so it can read the code and write to no branch.

-- NULL = not a planner.
ALTER TABLE ide_agents ADD COLUMN plan_id INTEGER;
