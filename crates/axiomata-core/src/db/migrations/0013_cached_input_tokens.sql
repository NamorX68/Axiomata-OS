-- The part of a run's or a chat turn's input tokens the provider read from its prompt cache
-- (docs/plans/glut-round.md, CP4): `input_tokens` now counts every prompt token processed, as Glut and Claude Code
-- already did, and the cached ones are metered at their own, lower price. NULL for rows from before, and for a
-- backend that does not say.
ALTER TABLE runs ADD COLUMN cached_input_tokens INTEGER;
ALTER TABLE chat_turns ADD COLUMN cached_input_tokens INTEGER;
