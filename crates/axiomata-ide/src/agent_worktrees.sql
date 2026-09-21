-- Per-agent worktrees and ports (milestone M7.2, CP5).
--
-- Three columns on `ide_agents`, all nullable, because all three are optional
-- in a way that matters:
--
--   * A project folder that is not a git repository has no worktrees at all.
--     Its agents run in the folder itself, sharing it, exactly as they did
--     before this migration. That is an ordinary situation, not a degraded
--     one, and `NULL` is how it is spelled.
--   * A worktree is created when an agent is created, and an agent created
--     before this migration does not have one yet. It gets one the next time
--     it is started, rather than in a migration that would have to run `git`.
--
-- Unlike migration 10 this is an ALTER, not a CREATE: the table shipped and
-- rows exist. `ALTER TABLE ADD COLUMN` is not idempotent the way
-- `CREATE TABLE IF NOT EXISTS` is, which is fine because the migration runner
-- applies each version exactly once, inside a transaction (see
-- `axiomata_core::db`, whose comment explains why that transaction matters
-- for precisely this kind of statement).

ALTER TABLE ide_agents ADD COLUMN worktree_path TEXT;

-- The branch checked out in that worktree, e.g. `axiomata/builder-3`. Stored
-- rather than derived: the name is built from the agent's name, and renaming
-- an agent must not silently rename the branch its work is on.
ALTER TABLE ide_agents ADD COLUMN branch TEXT;

-- A port reserved for this agent, so two agents that both start a dev server
-- do not fight over one number. Reserved by being written down, not by being
-- held open — the agent's own process binds it.
ALTER TABLE ide_agents ADD COLUMN port INTEGER;

-- A port belongs to at most one agent across all projects: two projects'
-- agents run on the same machine and would collide just as surely as two
-- agents in one project. The index is partial so that the many agents without
-- a port (a non-repository project, an older row) do not collide on NULL.
CREATE UNIQUE INDEX IF NOT EXISTS idx_ide_agents_port ON ide_agents (port) WHERE port IS NOT NULL;
