# The app's agents apart from the owner's own — and a master pane in the canvas

Status: drafted 2026-10-18, owner decisions D1–D9 taken (Q1–Q4 grilled the same day); CP0 measured, next: CP1. Background:
the owner's note `vault/Entwicklung/opencode-pi-harness-uebersicht.md` (put together with Grok) and the code as it
stands at `2377be6`.

## Problem

1. **What the app does reaches the owner's own tools.** With Axiomata running, the owner's Opencode in a plain
   terminal lost whole tools — the shell tool among them, not only rights (owner, 2026-10-18).
2. **The canvas has no way of working the way one works outside.** A Studio pane is a card session: started with the
   card's rights, the role's rules and the app's hooks. There is no pane that is simply the owner's Claude Code or
   Opencode as the owner uses it in a terminal.
3. **A reader and a writer at once.** A plan runs a worker (writes) and a reviewer (reads only) side by side. On
   Opencode both are sessions of one service, kept apart by nothing but the rule list each session was created with.

## What is coupled today (verified in the code)

- **One Opencode service for everything.** The app talks to the owner's own background service: the same database
  (`~/.local/share/opencode/opencode.db`), state (`~/.local/state/opencode/service.json`) and config
  (`~/.config/opencode`). Whatever the service keeps is shared with every client the owner runs.
- **The service may be the app's.** When it is not running, the app starts it (`Service::connect` →
  `opencode service start`) with the app's allowlisted environment (`agents::agent_child_env`); the owner's TUI then
  attaches to that process.
- **MCP state is per location on that service.** The app registers its `axiomata` server per worktree
  (`PUT /api/experimental/mcp/axiomata`) and reconnects failed servers before every unattended session
  (`Service::reconnect_failed_mcp`).
- **Rights are per session only.** Card and skill sessions get their rules when they are created
  (`create_session` `permissions`; a read-only reviewer's include shell and edit denies). Whether the service keeps
  them strictly per session — and where an `always` reply lands — has never been measured.
- **Claude Code is passed everything per process** (`--settings`, `--mcp-config`, `--permission-mode`,
  `--allowedTools`); nothing is written to `~/.claude` (`agent_entry.rs` module doc). One gap: an "always allow" in a
  pane is written by Claude Code itself to `.claude/settings.local.json` of the pane's folder — a worktree for a git
  project, but **the owner's real project folder** for a project without git (`shared_folder`).
- **Measured 2026-10-18:** Opencode 2.0.32 moves *all* its places with the `XDG_*` variables
  (`XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` → its own data, database, state, config,
  cache; `opencode debug paths`). A second, fully separate service is possible. The service is a plain detached
  process registered in its state folder (`service.json`), not a launchd agent, so two do not clash.

First guess for (1) was that a session's deny rules surface beyond that session; CP0 below disproved it and found
the couplings that are real.

## Decisions (owner, 2026-10-18)

- **D1 — the owner's own tools are never touched.** Outside the app, Claude Code and Opencode behave as if Axiomata
  did not exist: no config, rules, MCP registrations, sessions or service processes of the app in the owner's places.
- **D2 — the app's automation runs on Opencode instances of its own,** under `~/.axiomata/opencode/<instance>/` (the
  four `XDG_*` folders), started and stopped by the app. One source of truth for their credentials: the provider keys in
  `~/.axiomata/config.toml`, written into the generated config — no copy of the owner's `auth.json`.
- **D3 — reading and writing are two instances (option B).** A **read** instance whose config denies every edit and
  allows only read-only shell commands, and a **write** instance with today's defaults. A session's role rules still
  come on top per session, but a reader *cannot* write whatever a session, an `always` or a file in a worktree says.
  The boundary is a property of the process, not of a rule list. One service per session (option C) stays possible
  later for a case that needs it; it is not built now (a process per agent, MCP servers restarted every time).
- **D4 — a master pane per engine in the canvas.** "Claude Code" and "Opencode" as the owner uses them outside: their
  own settings, MCP servers, rules and modes, working in the real project folder. The app hosts the terminal and adds
  nothing to the engine's behaviour …
- **D5 — … except the team tools.** The master gets the `axiomata` MCP server, so the owner can send work from it to
  other agents (board, mailbox). Added per process (Claude Code: `--mcp-config`), never written into the owner's files.

## Design

### Instances

| Instance | Folder | Who runs on it | Rights in its config |
|---|---|---|---|
| `read` | `~/.axiomata/opencode/read/` | reviewers, planners, the grill, skills declaring `access: read` (D8) | `edit` deny, `shell` only the read-only list (git log/diff/show/status, ls, cat …), `external_directory` deny |
| `write` | `~/.axiomata/opencode/write/` | workers, chat and its instruct turns, skills that write | today's defaults; per-session rules as now |
| `master` | `~/.axiomata/opencode/master/` (data, state, cache; config the owner's, D6) | the master Opencode pane | the owner's own |

- **Launcher** (`axiomata-opencode`): `Service::connect` gets the instance's `XDG_*` folders in its child
  environment; discovery reads that instance's `service.json`. The app starts what it needs at first use and stops its
  own instances on quit — never the owner's service.
- **Generated config** per instance (`<config>/opencode/opencode.json`, rewritten at every start, mode `0600`):
  providers and keys from `config.toml` (D2), the MCP servers (D7), the instance's rights (D3). The owner edits the
  app's settings, never these files.
- **Routing**: `agents::opencode` picks the instance from the request — a card session from its role
  (`read_only()` → `read`), a skill from its declaration (D8), chat → `write`.
- **Migration**: session ids stored in the database (`agents.opencode_session`, chat sessions) belong to the owner's
  service. They are dropped on the first start after the switch; a card session starts a new session the way a
  restart does today.

### Master panes

- **Claude Code**: `claude` in the project folder, nothing but `--mcp-config <channel>/mcp.json` for the team tools
  (D5) — no `--settings`, no `--permission-mode`, no allowed or denied tools, no hooks. "Always allow" lands where it
  lands outside too: that is the owner's own doing.
- **Opencode**: a TUI on the `master` instance (D6) with the `axiomata` server registered there — the registration
  lives on an instance of the app's, so the owner's own service never sees it.
- The master has an identity on the board like any agent (it sends as itself, D5) but no card and no limits (D9).

### Claude Code in the automation

Unchanged per process, with one rule added: a card session never runs in the owner's real project folder. A project
without git gets no Claude card session (today only the reviewer is refused), (D9).

## CP0 — measured (2026-10-18, Opencode 2.0.32)

On a throwaway service of its own (`XDG_*` in a scratch folder, its own port) and read-only on the owner's database:

- **Session rules do not cross.** Session A (`shell *` and `edit *` deny, in a worktree of the repo) and session B
  (agent `build`, no rules, in the main checkout): B is allowed `shell`, `edit`, even `rm -rf x` (evaluated with
  `POST /api/session/{id}/permission`, no model needed). The guess above is wrong.
- **A session without an agent has no shell.** Created with no `agent`, a session gets `shell` deny and `edit` allow;
  with `build` it gets `shell` allow. Almost every session the app creates has no agent (`session_v2.agent` null):
  card sessions, chat, skill runs. A TUI opened on such a session (`opencode --session <id>`, a Studio pane) has no
  shell tool — the closest match to the owner's report found so far.
- **"Always" is saved per project, not per session.** Table `permission` (`project_id`, `action`, `resource`): a
  project is the git repository, so the app's worktrees and the owner's checkout share one list — 100+ entries for
  Axiomata-OS, among them `shell " *"` (2026-10-15). An "always" answered for an app agent (#88) holds for the owner's
  sessions, and the owner's for every app agent, in both directions. D3 has to cover this: the `read` and `write`
  instances each keep their own list, and the owner's is never touched.
- **The session list is shared too**: the owner's TUI lists the app's card, reviewer and skill sessions of the same
  project (hourly skill runs in the vault included) and can resume one, with that session's rules and missing agent.
- **The port is fixed per config.** `opencode service set port` writes `<config>/opencode/service.json`; a second
  instance on the default port 49374 fails to start. Every app instance needs its own port in its generated config.
  For D6 (the master on the owner's `~/.config`) that file is the owner's, so the master's config folder becomes one
  of the app's that links the owner's `opencode.json`, `agents/`, `commands/` and `AGENTS.md` and has a
  `service.json` of its own — decided in CP3.
- **Not the cause:** the service's environment (the running one was started by the owner from Ghostty, full env;
  the app adds no config patch — it never calls `PATCH /api/experimental/config`) and the owner's config (a sane
  `permissions` list, `build`/`plan` with models).

Still open: the exact action that made the owner's TUI lose its shell. Next time it happens: the session's title
(an app session?) and whether the app had started the service.

## Checkpoints

- **CP0 — measure.** Reproduce (1) live: which app action makes the owner's TUI lose its shell tool. Then two
  sessions on one service with different rules, an `always` in one: does anything cross? The result confirms D3 or
  moves a case to option C. Written into this plan.
- **CP1 — the app's own instances.** Launcher with `XDG_*`, discovery, generated config, start/stop, routing
  everything to `write` first; migration of stored session ids. The owner's service is no longer started or touched.
- **CP2 — read and write apart.** The `read` instance with its config rights; routing of reviewers, planners and
  read-only skills.
- **CP3 — master panes.** Claude Code and Opencode in the Studio's pane menu, team tools wired, the `master`
  instance (D6).
- **CP4 — Claude Code hygiene.** No card session in a real project folder (D9); docs, `AGENTS.md` traps (the
  Opencode one changes: "the app's own instances, never the owner's service").

## Grilled (owner, 2026-10-18)

- **D6 (Q1) — the master Opencode runs on an app instance with the owner's config.** Own data and state folders,
  `XDG_CONFIG_HOME` the owner's `~/.config`: the owner's agents, commands, MCP servers and rules exactly, sessions and
  database apart. Credentials come from `config.toml` (D2) — providers the owner only logged into with
  `opencode auth login` are not there, by choice (one source).
- **D7 (Q2) — the app instances' MCP servers come from `~/.axh/mcp.json`,** the own harness's list, so both engines
  of the app read one list. The owner's `opencode.json` is no longer read for them.
- **D8 (Q3) — a skill declares `access: read` in its frontmatter** to run on the `read` instance; without it a skill
  runs on `write` with its rules as today.
- **D9 (Q4) — no Claude Code card session in a project without git** (the worker joins the reviewer's refusal), so the
  app never writes into the owner's real project folder. The master takes no cards: it sends work (board, mailbox) as
  itself, with no card and no limits.
