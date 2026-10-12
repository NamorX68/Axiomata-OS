# The agents' permission requests, answered in the app (board card #88)

Status: built 2026-10-12. The owner asked for it in one go and to test it afterwards, so the decisions below were made
without the usual grilling; each one can be turned over after the test.

## Problem

A card session that wants a right it was not given stops on a prompt inside its terminal pane. The waiting badge
(#87) says that it waits, but the owner has to find the pane to answer. And a session that waits for a *text* answer
(the grill in its interview, a planner done with its draft) reports `idle`, so no badge appears at all.

## Decisions

- **R1 — where the owner answers.** In the app: in the waiting badge's list (Dock, Orbit, Kanban) and in a strip on top
  of the agent's own pane in the Studio. Each shows what is asked (tool and its argument) and **Allow** / **Deny**;
  Claude Code's also **In terminal**, which hands the question back to the pane's own prompt. No "always": a lasting
  rule widens the session's rights, and that stays the role file's job.
- **R2 — Claude Code.** A `PermissionRequest` hook (Claude Code runs it before it shows a prompt) marks the channel
  `waiting` and runs `axiomata-cli permission-hook --agent <id>`. That stores the request in the database and waits for
  the owner's answer, polling it twice a second. Allow or Deny goes back to Claude Code as the hook's decision; **In
  terminal**, no answer within 25 minutes, or any error, ends the hook without a decision, and Claude Code shows its own
  prompt as before. The hook's own timeout is set above the wait.
- **R3 — Opencode.** The service already lists a session's open requests and takes a reply. The status tracker keeps the
  request it saw in `permission.asked` (or found when seeding) and the app answers it with `once` or `reject`. The
  prompt in the pane stays usable; whichever answer comes first counts.
- **R4 — who may answer.** Only the owner, through the app's own command (`answer_permission`, actor `human:owner`).
  There is no CLI command that answers, and the hook never takes an answer it did not read from the database. Not a
  sandbox: the session runs as the owner's user and could write the database file itself — the same limit as #89/#90
  (`docs/plans/plan-auto-rights.md`, "Not a sandbox"). The hook asks only in its own session's name
  (`AXIOMATA_AGENT_ID` must be set and match); a shell that sets the variable itself can file a request in another
  agent's name, which closes that agent's open request (its hook hands the prompt back to the pane) — it can never
  answer one. A session's open requests are closed when it starts again; answered rows are kept 30 days.
- **R5 — what is shown.** The tool and its main argument (the command, the file, the URL), cut to one line of at most
  300 characters. It is written by the agent, so it is plain text everywhere, never markup.
- **R6 — waiting for a text answer.** A session made for a plan (the planner, the grill) that is `idle` while its plan
  is still a draft counts as waiting: it has asked or proposed, and the next step is the owner's.
- **R7 — later.** Questions with forms (Opencode `form.created`, Claude Code's `AskUserQuestion`) are still answered in
  the pane; the badge shows that they wait.

## Design

- **Database** (migration 32, `permission_asks`): one row per request — agent, tool, detail, when asked, the decision
  (`allow` / `deny` / `terminal`), who decided and when. `axiomata_core::permission_ask` holds the store, the hook's
  wait and its output.
- **Waiting list** (`ide_status::waiting_agents`): each waiting agent carries its open request, if any (`ask`): from the
  database for Claude Code, from the tracker for Opencode. R6 adds the idle planners.
- **Command** `answer_permission(agent_id, request_id, decision)`; the watcher's next tick brings the changed list.
- **Frontend**: `ui/WaitingBadge.svelte` (the list) and the agent pane's strip, both over `core/agentWaiting.ts`.
