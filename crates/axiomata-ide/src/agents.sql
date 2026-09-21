-- IDE agents (milestone M7.2, CP4).
--
-- An agent is a *profile*: a name, which harness runs it, the command line
-- that starts it, and the model and environment it runs with. It is not a
-- running process — a PTY session belongs to whichever pane is showing the
-- agent right now and dies with the app, which is the price of owning our own
-- PTY engine rather than leaning on tmux (question F7 in the plan, answered
-- that way deliberately).
--
-- What is deliberately NOT here, by the same rule migration 9 followed: the
-- worktree path, branch and reserved port arrive in CP5, and the lifecycle
-- status in CP6, each as its own migration once its shape is known. A frozen
-- schema must not contain a hypothesis.
--
-- Two load-bearing choices:
--
--   * `UNIQUE (project_id, name)`. CP5 builds a git worktree path out of the
--     project and the agent name (`~/.axiomata/worktrees/<project>/<agent>`),
--     so two agents sharing a name inside one project would be two agents
--     quarrelling over one directory. The same reasoning that made
--     `projects.repo_root` UNIQUE, one level down.
--   * `ON DELETE CASCADE` on the project. An agent without its project is not
--     a thing that can mean anything; when CP5 adds worktrees this also
--     becomes the hook for cleaning them up, which is why the foreign key is
--     here from the start rather than left to application code.

CREATE TABLE IF NOT EXISTS ide_agents (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    -- 'claude_code' | 'opencode' | 'mini'. Stored as text rather than an
    -- integer so a human reading the database sees what it says; the crate
    -- refuses any other value on the way in and on the way out.
    harness    TEXT NOT NULL,
    -- The command line that starts this agent. Empty means "the harness's own
    -- default", resolved in Rust so the default can change without a data
    -- migration of every row that never overrode it.
    command    TEXT NOT NULL DEFAULT '',
    -- NULL = whatever the harness itself picks. Axiomata does not route a
    -- foreign harness's model choice; it only passes one on when asked to.
    model      TEXT,
    -- KEY=value per line, the same shape the Terminal module's env setting
    -- uses, parsed by the same frontend helper.
    env        TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Unlike `projects`, this one earns its index: every project view lists its
-- agents, and from M7.5 the mailbox resolves "who is in this project" on each
-- delivery. Still small, but read far more often than it is written.
CREATE INDEX IF NOT EXISTS idx_ide_agents_project ON ide_agents (project_id);

-- The uniqueness promise from the header, as an index rather than a table
-- constraint, because it needs `COLLATE NOCASE` and SQLite only takes a
-- collation on an indexed column. The case-insensitivity is not politeness:
-- CP5 turns the name into a directory under ~/.axiomata/worktrees/, and macOS
-- filesystems are case-insensitive, so "Builder" and "builder" would be two
-- rows pointing at one directory.
CREATE UNIQUE INDEX IF NOT EXISTS idx_ide_agents_name ON ide_agents (project_id, name COLLATE NOCASE);
