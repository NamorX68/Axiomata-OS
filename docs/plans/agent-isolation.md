# The app's agents apart from the owner's own — and a master pane in the canvas

Status: drafted 2026-10-18, owner decisions D1–D9 taken (Q1–Q4 grilled the same day); CP0 measured, CP1–CP4 built (2026-10-18); open: the owner's Mac test. Background:
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
  environment; discovery reads that instance's `service.json`. The app starts what it needs at first use — never the
  owner's service — and leaves it running when it quits (see CP1 below).
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
  **Built 2026-10-18** (`agents/opencode_instance.rs`), checked end to end with a scratch `AXIOMATA_HOME` and a local
  model (cold start, a config change restarting the instance, the owner's database untouched). What building it
  settled:
  - **Port:** chosen free once, kept in the instance's `config/opencode/service.json` beside the password the service
    writes there.
  - **Model catalog:** the `providers` of the owner's `opencode.json` (models, variants, routing preferences) are
    copied without any credential field, URL with a user part or `headers`; keys and the Ollama endpoint then come
    from `config.toml`. OpenRouter with the app's key drops the catalog's `baseURL`/`package`, so the key only ever
    goes to Opencode's own OpenRouter endpoint. A provider the owner reaches only through `opencode auth login`
    (Anthropic's subscription) is not available to the app (D6, by choice).
  - **Shell environment:** a service with the instance's `XDG_*` folders hands them to every command it runs, so `gh`
    or `git` would look for their configuration there. Every session gets the app's allowlisted environment back
    (`PUT /api/session/{id}/environment` — measured: it *replaces* the environment), at creation and whenever it is
    continued.
  - **Panes:** the TUI is typed with `env XDG_STATE_HOME=<instance state>` in front — the discovery file is all it
    needs (measured), so its own look and keys stay the owner's.
  - **Restart:** the configuration is regenerated at every connect; a running service started with another one (marker
    `started-with.json`, written only after a start that worked) is restarted only while no session executes, else
    later. A restart drops the per-location `axiomata` MCP registrations of idle card sessions — they come back when
    the pane restarts.
  - **Not stopped on quit:** stopping it would cut off every running card worker on each `cargo tauri dev` rebuild; it
    is the app's own, so leaving it running costs the owner nothing.
  - **Guard:** before starting or restarting, Opencode's own `debug paths state` must name the instance's folder; the
    instance folder must be an absolute, real folder (mode `0700`).
  - **Cold start:** a fresh service lists Ollama's models a moment after it answers; a new session waits (up to 15 s)
    until its model is listed.
  - **Migration:** a stored card session id is unknown to the new service and replaced at the next start; a chat whose
    session is unknown goes on in a new one. `~/.axh/mcp.json` was created with the owner's two Apple servers.
  - **Locked:** card sessions may not write `~/.axh/` (Opencode auto mode shell, Claude `Edit`/`Bash` denies).
- **CP2 — read and write apart.** The `read` instance with its config rights; routing of reviewers, planners and
  read-only skills. **Built 2026-10-18.** Measured first on a throwaway service (2.0.32, evaluating with
  `POST /api/session/{id}/permission`, the answers steady only a few seconds after a session is made):
  - **The configuration's rules alone do not hold.** A project's `opencode.json` or `.opencode/opencode.json` with
    `shell *`/`edit *` allow beats them; so does a saved "always" (`touch *` answered in one session allows it in a
    session without rules). `OPENCODE_DISABLE_PROJECT_CONFIG` stops the project file but also every project
    `AGENTS.md` (an agent no longer knows the repository's guidance) — not used.
  - **A session's own rules beat both**: a session whose list starts with `shell *`/`edit *` deny refuses what the
    project file and the saved "always" allow. `policies` (`tool.use` deny) did not take a tool away.
  - So the read instance's rights ([`read_rules`]: edit, shell and outside folders refused, read-only git and `ls`
    allowed, `--no-index`/`--output` refused) are in its generated config — for a pane opened on it without rules —
    **and** in front of every session the app makes there: a reviewer's or planner's role and the owner's auto mode
    lift them one rule at a time behind it, never the project; a skill with `access: read` gets them after its
    run rules, so its auto approval cannot lift them. Its database keeps its own "always" answers.
  - **Routing:** a card session from its launch (`Launch::read_only`: a reviewer, a planner) — the session, its MCP
    registration, its first prompt and the pane's `XDG_STATE_HOME` all on the same instance; a skill from
    `access: read` (D8, `read`/`write`, anything else refuses the skill); chat and workers on `write`. The status
    watcher follows both instances; the limit meter, the activity look, the permission answer and an escalation's
    interrupt find a session on whichever instance knows it.
  - **Checked end to end** with a scratch `AXIOMATA_HOME` and a local model: a skill with `access: read` started
    only the read instance, its `touch` was refused by the service, its `ls` ran, nothing was written.
  - **Review (2026-10-18):** a redirect writes through an allowed command — measured, `git log > f` and `ls > f`
    made their files on the read rules, while a pipe (`| tee f`) and `$(…)` are split into commands of their own and
    refused. So `*>*` is refused last, in the config and in every read session (a reader's `2>&1` goes with it);
    git's long options are matched in their shortest forms (`--ou*`, `--no-i*`); `ls` takes no path.
  - **What `read` does not mean:** a reviewer in the owner's auto mode keeps `shell *` (card #95, the owner's
    choice) — only edits, other folders, redirects and the auto mode's denies stay refused; and a skill with
    `access: read` still has every tool of the MCP servers (apple-mail can send). The boundary is the workspace and
    the checkout, not the outside world.
  - Not done: the bundled skills declare no `access` yet — the digests only read, but on `read` they would start a
    second set of MCP servers (apple-mail opening Mail.app from both instances); left to the owner.
- **CP3 — master panes.** Claude Code and Opencode in the Studio's pane menu, team tools wired, the `master`
  instance (D6). **Built 2026-10-18:**
  - **A master is an agent row with the reserved role `master`** (`axiomata_ide::model::MASTER_ROLE`), one per project
    and harness, made the first time it is asked for (`roster::master_session`; Agents panel "Master · Claude Code" /
    "Master · Opencode", `ide_master_session`, `axiomata-cli ide agents master <project> <harness>`). No role file
    may take the name. Its command is the harness's own default — not an engine of the catalog, whose flags are the
    studio's.
  - **The real folder, nothing added:** `provision::prepare` gives it the project folder (no worktree, no branch — so
    git's actions on an agent, which need both, never reach that folder) and no status hooks on its command line.
    Claude Code starts as `claude --mcp-config <channel>/mcp.json`; Opencode as `opencode` with the master instance's
    `XDG_STATE_HOME` in front, no session made for it and no rules added.
  - **The `master` instance** (`~/.axiomata/opencode/master/`): its config is the owner's `opencode.json` as it is —
    agents, rules, default model, MCP servers — with the providers under `config.toml`'s keys (D2/D6); the owner's
    `AGENTS.md`, `agents/`, `commands/` and the like are **linked** into its config folder (a fixed list: never
    `.gitignore`, `package.json` and the other files Opencode keeps up there itself, which it would write through a
    link). Its own port, database and sessions. Not watched or limited — nothing of the app's runs there. Started
    before the pane attaches, so a terminal UI never starts a service of its own with the owner's folders.
  - **Team tools (D5, D9):** the `axiomata` server registered at the project folder (Opencode: on the master
    instance), with the master's own capabilities — mail, the board to read, proposals of any kind — and no
    `claim_task`, `report_done`, `review_verdict`: it sends work, it takes none.
  - Checked end to end with a scratch home: both masters start in the project folder, the Opencode one sees the
    owner's agents (`architecture-reviewer` …), rules and model, the owner's folder is only read.
  - **Review (2026-10-18), changed:** the pane's prefix carries the instance's data and cache folders too (a UI that
    starts a service itself never does so on the owner's database); the master's `XDG_CONFIG_HOME` links the owner's
    other `~/.config` entries (`gh`, `git` …), since the TUI's sessions get no environment from the app; a master gets
    no channel variables (`CLAUDE_CODE_TASK_LIST_ID`), reads as not status-connected, cannot be edited into another
    role, its registration is healed and taken off the master instance on delete; a role file named `master` is
    refused where it is saved.
  - **As outside, on purpose:** the Opencode master's shell has no agent identity, so `axiomata-cli` there acts as the
    owner — exactly as in the owner's own Opencode; the Claude master has its identity from the pane. The owner's
    `opencode.json` is copied with its MCP servers as they are (secrets included) into the master's 0600 config. A
    changed config restarts the master's service only while no session executes — an idle UI attached to it
    reconnects. Only `opencode.json` is read (not `.jsonc`). The Claude master gets the team tools but no mailbox
    instructions (nothing is added to its prompt).
- **CP4 — Claude Code hygiene.** No card session in a real project folder (D9); docs, `AGENTS.md` traps (the
  Opencode one changes: "the app's own instances, never the owner's service"). **Done 2026-10-18:** D9 already held —
  `card_session` refuses a card session, a reviewer, a plan's steps and a planner for a project that is no git
  repository, whatever the engine (since CP-A6; tested). The only session in a real folder is the master, the
  owner's own tool. The traps are in `AGENTS.md` (Opencode's instances, the master).

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
