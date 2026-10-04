-- Engine and role of an agent session (docs/plans/a2a.md, CP-A1).
--
-- `ide_agents` has been the *session* all along (worktree, branch, port, Opencode session) while also carrying the
-- engine's fields (harness, command, model, env). The engine catalog now lives in the owner's config and the roles
-- in `~/.axiomata/agents/<name>/AGENT.md`; a session points at both by id. Neither is a foreign key, because
-- neither lives in this database.
--
-- The old harness/command/model/env columns stay as the fallback until CP-A6 moves starting over to the engine —
-- a migration that deletes data it cannot get back is not worth a tidier table.

-- NULL = not assigned yet. The application derives engines from the existing rows once at start and fills this.
ALTER TABLE ide_agents ADD COLUMN engine_id TEXT;

-- Every existing agent becomes an `allrounder`, the role seeded on first start.
ALTER TABLE ide_agents ADD COLUMN agent_role TEXT NOT NULL DEFAULT 'allrounder';
