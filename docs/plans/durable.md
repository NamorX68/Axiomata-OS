# Durable Glut runs (after Pi Durable)

Status: grilled 2026-10-20 (D1–D8), **built 2026-10-20** (CP1–CP6). Live: an `axh run` on Ollama killed with
`kill -9` after two tool calls went on with `axh resume` — the read under way ran again, the steps counted on (2 → 6),
the journal was gone afterwards. Deviations while building: D9 below. Follows `docs/plans/glut-2.md` (G5: Pi Durable is a plan
of its own).

## Where we stand

- **Pi Durable** (`@earendil-works/pi-durable`, with Pi 1.0, October 2026, experimental) is a runtime beside Pi's coding
  CLI: every step of a conversation is committed atomically (SQLite or JSONL), a new process opens the same storage
  and unfinished tasks go on from their last checkpoint; an interrupted tool call is rerun only when the tool declares
  `replay: "safe"`, otherwise the model gets an "interrupted" result. It also has forks, background tasks and several
  clients on one conversation — not taken here. Sources: [stacktoheap](https://stacktoheap.com/blog/2026/10/05/pi-durable/),
  [mindstudio](https://www.mindstudio.ai/blog/pi-durable-long-running-agents),
  [the monorepo](https://github.com/earendil-works/pi).
- **Here today**: a Glut run lives in the app's process; the app quitting, crashing or `cargo tauri dev` rebuilding
  ends it, and nothing is left — `runs` gets its row only at the end. Glut's chat writes its session file only after
  an answered turn. Opencode's turns go on in its own service, but the app loses sight of them; Claude Code is a child
  process and dies with the app.

## Decisions

- **D1 — scope**: Glut runs (skills, routines, the chat, card sessions on Glut) become resumable. For every harness a
  run is visible from its start: its `runs` row is written as `running` when it starts and finished at the end; a row
  still `running` when the app starts is marked `interrupted` — never gone without a trace. Opencode and Claude Code
  runs are not resumed (Opencode's service may still finish its turn; the app does not collect it).
- **D2 — in the app**: Glut keeps running in the app's process. Every step is saved; after a restart a run goes on from
  its last saved step. No daemon.
- **D3 — resume on the owner's click**: an interrupted run shows as `interrupted` with a Resume button (the run list,
  the chat panel for a chat turn), and `axh resume <run>` in the terminal. Nothing goes on by itself and costs money.
- **D4 — replay as Pi**: a tool call that was under way when the run stopped is run again only when it only reads
  (`read_file`, `grep`, `glob`, `list`, `web_fetch`, `web_search`, `read_full_result`, `todo_write`); every other call
  (`bash`, the edit tools, `codemode`, `task`, an MCP tool) gets the result "interrupted while running — check the state
  before you go on", and the model decides. The snapshots (Glut 2) show what changed.
- **D5 — a journal per run**: `~/.axiomata/harness/runs/<run>.jsonl`, one line appended and synced per step; the harness
  stays self-contained, so `axh` resumes as the app does.
- **D6 — limits on resume**: steps and money used so far count on; the time limit starts afresh for the rest (the pause
  was no work).
- **D7 — MCP replay by the server's word**: an MCP call is rerun only when its tool declares `readOnlyHint: true`
  (the MCP tool annotations); everything else is interrupted as in D4.
- **D8 — retention**: a finished run deletes its journal (its result is in `runs` or the chat's session file); an
  interrupted one stays resumable for 7 days, then the sweep removes it — as the snapshots.

- **D9 — found while building** (revisited 2026-10-21 by `docs/plans/glut-cards.md`, C8: card sessions run on Glut
  now, and a lost turn of one resumes from a journal in its channel): card sessions never run on Glut (they are Opencode and Claude Code panes), so there is
  none to resume. A prompt routine's run is recorded from its start like a skill's, but only a skill's run resumes
  (it is rebuilt from its `SKILL.md`). `axh` keeps its own journals in `~/.axh/runs` and resumes only its own runs
  (`axh resume [run] [--list] [--discard]`, its flags kept in the journal); the app's are in `~/.axiomata/harness/runs`.
  A run's row is `running` from its start for every harness; the start-up sweep checks the row's process id, so a
  CLI run going on beside the app is not marked. A chat turn the app lost is offered in the chat panel (Resume / Drop)
  and said once by a toast at start. The connector tiles pass a `running` row by and hear its end (their watch keys
  on id and status).

- **D10 — from the reviews (2026-10-20)**: the journal is written in one write and one sync per step (a chat turn
  opens with its whole earlier conversation), after the SessionStart context; a torn tail is cut off before a resume
  appends; a journal is only a regular file of the owner's, opened without following a link and read to a bound; a
  journal of another run, or one lost before its task was saved, is not resumed. Only a skill run on Glut gets a
  journal; a run whose journal is gone is no longer offered (start-up tidy); a resume claims its row once (a second
  click goes no further) and checks the journal is that skill's. A chat turn resumes only on Glut, not as a command,
  and only while its conversation is as the turn found it. `axh resume` needs the run named, prints the flags it goes
  on with, and `--base-url` may hold no user or password (the journal keeps it).

## Design

- **Journal lines** (JSON, one per line): `start` (run id, kind `skill|routine|chat|card`, name, cwd, model id — never a
  key — the rights as written, limits, the session id of a chat turn), then per step `message` (an assistant reply with
  its tool calls, or a tool's result), `call` (a tool call about to run) and `usage` (the step's tokens and cost, the
  step count), and `end` (finished, why). A line is written with `O_APPEND` and synced before the step goes on; a torn
  last line is ignored on reading. One process per journal: a file lock (as the snapshot store's).
- **Resume** (`axiomata_harness::resume`): the journal gives the conversation and what was used; each `call` without
  its result is replayed or answered "interrupted" (D4/D7); the loop goes on with the remaining steps and money
  (D6). The caller rebuilds the toolbox and gate from the same skill or chat settings as a fresh run — the journal
  names them, it does not carry them (a changed `~/.axh` rule applies to the rest).
- **Subagents**: only the parent's steps are journaled; a `task` under way is interrupted (D4).
- **`runs`**: the row is inserted as `running` at the start (all harnesses), updated at the end; at app start every
  `running` row becomes `interrupted` with "the app stopped during the run", plus `resumable` when its journal is there.
- **Chat**: an interrupted chat turn leaves the conversation as it was before the turn; Resume finishes the turn and
  writes it to the session file as an answered one would.

## Checkpoints

- **CP1 — journal** (`axiomata-harness`): writer and reader, per-step sync, torn-line tolerance, lock; the loop writes
  it when `RunConfig.journal` is set. Tests.
- **CP2 — resume** (`axiomata-harness`): `resume()` with the replay rules (D4/D7, `readOnlyHint` read from the MCP tool
  list), the limits (D6); `axh resume <run>` and `axh resume --list`. Tests with a scripted model.
- **CP3 — `runs` from the start** (core, all harnesses): `running` → `success|failed`, the start-up sweep to
  `interrupted`; migration if a column is needed (`resumable`, `journal`).
- **CP4 — resume in the app** (core + Tauri): skills, routines, card sessions and chat turns on Glut; a Tauri
  `resume_run`; the retention sweep (D8).
- **CP5 — UI**: `interrupted` in the run lists with Resume, the chat panel's "turn interrupted — resume".
- **CP6 — docs + review**: `architecture.md` §5/§7, AGENTS.md; the combined review and the security-auditor (new Tauri
  command, files under the app's home).
