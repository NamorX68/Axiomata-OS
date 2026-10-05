-- What a plan is for, in the owner's words (docs/plans/a2a.md, CP-A7).
--
-- The planner reads it with `get_plan`; it is never put into a shell line (a plan's text, like a card's, reaches an
-- agent through a tool, not through a command). Empty for a plan that was made without one.
ALTER TABLE plans ADD COLUMN goal TEXT NOT NULL DEFAULT '';
