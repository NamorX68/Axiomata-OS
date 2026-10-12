# A plan's auto mode — workers may do everything but push (board card #89)

Status: decided 2026-10-12 (owner), being built.

## Problem

A card session asks before every command its role does not list (`BUILD_AND_TEST`, `READ_GIT`) — right for a plan the
owner has not looked at, a brake for one whose cards the owner has read and found clean: the run stands still on
questions nobody is there to answer.

## Decisions

- **A1 — where.** A switch "Auto mode" beside the plan's release, shown exactly when the release is (the owner has just
  read the cards). Off by default. Also on a plan made from a card.
- **A2 — Claude Code.** `--permission-mode auto` instead of `acceptEdits`: Claude Code's classifier lets safe actions
  through and refuses risky ones; the denied list (`CLAUDE_CARD_DENIED` plus `CLAUDE_AUTO_DENIED`: push, git outside
  the checkout, the configuration files, the CLI, the database, the git hooks) still holds, and project settings and
  MCP servers are not read (`--setting-sources user --strict-mcp-config`). Never `bypassPermissions`.
- **A3 — Opencode.** No classifier, only rules (last match wins): `shell *` and `edit *` allowed; refused instead of
  asked — a question would stop an unattended run — `external_directory`, `.env` reads (`.env.example` stays readable),
  edits and shell commands touching the configuration a later session would obey (`opencode.json`, `.opencode/`,
  `.claude/`, `.mcp.json`, `.git/hooks`), the CLI, `sqlite3` and `.axiomata/` (`AUTO_DENIED`).
  The git-outside and push denies come last and win.
- **A4 — who.** Workers, and since card #95 (owner, 2026-10-12) reviewers too: a reviewer gets the free shell under the
  same denies — it hung on `npm ci` / `git check-ignore` otherwise — and still no edit (Claude Code: `Edit` denied on
  top; Opencode: no `edit *` allow, the read-only `edit * deny` wins). Planners and grilling sessions are done before
  the release.
- **A5 — when.** Read at a session's start (`ide_start::card_launch`), and only while the plan is released (approved,
  not closed). The switch stays on the running plan; a change
  applies to sessions started afterwards, a running one keeps what it started with (Claude Code cannot change its mode
  mid-run; a continued Opencode session keeps the rules it was created with).
- **A6 — who switches.** Only the owner (`human:` actor, `flow::set_plan_auto_rights`); taking the plan over stays the
  owner's in any case.

## Not a sandbox

The sessions run as the owner's user, and a free shell reaches whatever that user reaches. The deny patterns stop the
plain forms only (security audit 2026-10-12):

- **Push.** `git push *`, `git * push *`, `*git push*` are refused; an alias (`git config alias.p push`),
  `git send-pack`, `gh` or a script are not. Claude Code's classifier is the second lock; Opencode has none.
- **Acting as the owner.** The owner's identity in the CLI is the absence of `AXIOMATA_AGENT_ID` — a shell can unset
  it, and `~/.axiomata/axiomata.db` is a file the user can write. `axiomata-cli …`, `sqlite3` and paths under
  `.axiomata/` are refused as patterns; `cargo run -p axiomata-cli` or a copied binary are not.
- **The next session's rights.** The `Edit` denies cover the edit tools; a shell write to `.claude/settings.json`,
  `.mcp.json`, `opencode.json`, `.opencode/` or `.git/hooks` is refused only as a pattern. Claude Code workers in
  auto mode read no project settings and no project MCP servers (`--setting-sources user --strict-mcp-config`), so a
  written `.claude/settings.json` does not reach them; Opencode reads its project config from the worktree.
- **Outside the worktree.** `external_directory` covers Opencode's file tools, not its shell.
- **Switching off** does not reach a session that runs; stopping it means stopping its card.

The mode is a yes to the cards the owner has read. Cards proposed later by a session join the plan only through the
owner's approval of the proposal. Follow-up (own card): a real lock — an owner proof the agent shell cannot forge, the
database out of the sessions' reach, and the worktree's configuration files checked by hash at every start.

## Design

- `plan_auto_rights` (board schema V11, migration 31): a row means "on"; `Plan::auto_rights`.
- `CardLaunch::auto_rights` (`work_freely`), `Launch::auto_rights()` (never for a reviewer or a plan session).
- `agent_entry::claude_card_options(role, review, auto_rights)`, `opencode::CardRights::auto_rights` / `AUTO_DENIED`.
- Tauri: `approve_board_plan(…, autoRights)` sets it with the yes; `set_plan_auto_rights(id, on)` switches it later.
- `PlanPane.svelte`: the switch at the release, and on the approved plan.
