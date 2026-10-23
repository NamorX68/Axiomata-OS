-- Opencode leaves (docs/plans/opencode-exit.md, O3): a Studio session on it starts as a Glut session from now on. Its
-- worktree, branch and card stay; its Opencode session id names nothing any more. Its engine became a Glut engine with
-- the same id and model at the config's load. A master stays (O1: the plain Opencode tile), and so does a session with
-- a command of the owner's own making, which runs as written (E13).
UPDATE ide_agents
SET harness = 'glut', opencode_session = NULL
WHERE harness = 'opencode' AND agent_role <> 'master' AND trim(command) = '';
