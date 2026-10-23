# Glut on cards — the own harness works a card

Status: **approved** by the owner 2026-10-21 ("Ja leg los"; C1: `mini` was the placeholder, it becomes `glut`); CP1–CP4
**built** 2026-10-21 on branch `glut-cards`, CP5 (the Mac) open. Follows the owner's decision of 2026-10-21 ("Glut soll
Karten abarbeiten dürfen") and revisits D9 of `docs/plans/durable.md`. Next on the road: Glut's own TUI, then Opencode
leaves Axiomata.

## Why

Glut is the only one of the three harnesses with an OS sandbox, a network gate, LSP errors after an edit, refusal
limits and its own compaction — yet a card can only be worked by Claude Code or Opencode. As long as that holds,
Opencode cannot leave, and the flow (worker → reviewer → take-over, a plan running by itself) runs on the two engines
Glut was built to replace.

## What exists (measured in the code, 2026-10-21)

- **Engines** (`axiomata-roster`): `Harness` is `claude_code | opencode | mini`; `mini` is the M7.4 placeholder
  (`axiomata-miniagent`), refused or empty at every step (`ide_start.rs:224`, `lifecycle.rs:325`,
  `session_limits.rs:316`, `session_activity.rs:115`, `ide_status.rs:396`). Nothing in roster or IDE knows Glut.
- **A card session** is made by `card_session::start_card_session` (claim, role, engine) and started by
  `ide_start::start_agent` when the pane opens: worktree, channel, the team server `axiomata-cli mcp-serve` with the
  session's secret in its own environment, the card's rights, the start prompt (`agent_entry::start_prompt`), and a
  launch command the pane types into its terminal. Reviewer, planner and a plan running by itself all go through it.
- **Seen from outside:** status from the channel files, "waiting" from the `permission_asks` table (Claude Code's
  `PermissionRequest` hook, #88), limits by reading the harness's own records (`session_limits.rs`), the Flow tiles
  likewise (`session_activity.rs`). The terminal is the only live view.
- **Glut today:** `axiomata-cli harness run` prints readable steps and asks in the terminal when stdin is a TTY
  (`Terminal` asker); `axh run` headless; MCP from `~/.axh/mcp.json` + `.mcp.json`, no `--mcp-config`; one prompt per
  run, no follow-up in a terminal. In the app Glut never asks anyone (`Nobody`, `agents/axh.rs:1201`).

## Decisions to confirm

- **C1 — the engine kind:** `mini` becomes **`glut`** (`Harness::Glut`, spelled `glut`; a stored `mini` reads as
  `glut` — the mini agent is what Glut became). Its engine's `model` is the model id; the provider is the **chat
  provider** (must have a `base_url`: OpenRouter or Ollama, else the start fails with a clear message, as a skill run
  does). `billing` stays the engine's own field (metered for OpenRouter).
- **C2 — where it runs: in the pane, like the others.** The launch command is a new `axiomata-cli harness card`
  (the session named by `AXIOMATA_AGENT_ID`), typed into the pane's terminal as Claude Code's is. It prints the steps
  as `harness run` does, so the pane shows what Glut does live. Nothing else of the pane machinery changes, and the
  TUI later replaces only the printing. *(Rejected: a background run in the app with its own view — a second live
  view to build now and throw away with the TUI.)*
  **Done 2026-10-23 (Glut TUI8b):** the pane runs Glut's own TUI, `harness_card::CardHost` its host — the model
  fixed, the mode locked to the role's, no `!cmd`, `/compact` and `/handoff write` refused at the session's limit and
  metered in its event log, a closed pane leaves the turn to its journal.
- **C3 — the team server and the secret:** `axh` and `harness card` gain **`--mcp-config <file>`** (Glut's own flag,
  Claude's shape; feedback "Glut standalone"), and the card's `mcp.json` (0600, in the channel) names only the team
  server, as for Claude Code cards. The secret lives in that server's environment, never in Glut's shell
  environment. New on Glut: its sandbox **hides the channel folder** from the agent's commands, so the session cannot
  read its own secret — stronger than on Claude Code (a2a.md A39).
- **C4 — rights:** the card's rights as Glut rules — the same lists `wire_claude` builds (worker: edits in the
  worktree, `Edit(~/.axh/**)` denied; reviewer and planner: read only — built as mode `default` with `Edit` denied,
  as Claude Code's reviewer, so a command is asked rather than refused), cwd and sandbox the worktree.
  The owner's `~/.axh/settings.json` `deny`/`ask` still beat them, its `allow` never widens them (as for skills).
- **C5 — asks: in the app and in the terminal, whoever answers first.** A new asker writes the ask into
  `permission_asks` (the badge, the waiting tile and the app's answer dialog of #88 work unchanged) and also prompts
  in the pane's terminal; the first answer wins, the other is withdrawn. A plan running by itself has nobody at its
  pane: its asks wait for the app like Claude Code's. In the terminal it is the prompt `harness run` already shows
  (`? bash cargo test — allow? [y]es / always / [n]o:`, `j`/`ja`/`immer` too; anything else is no). One reader owns
  the pane's input for the whole session, so a line read for an ask the app answered first is not left pending: the
  terminal prints "answered in the app: …" and the next line goes to whatever asks next.
- **C6 — a budget per run, Glut's own** (also the roadmap item "budget per run"): `axh` / `RunConfig` gain
  `--max-tokens` and `--max-usd` beside `--max-turns`, ending the run with a new `FinishReason::Budget`; prices come
  from the run config (Axiomata passes its `agents.costs`; `axh` reads `"prices"` in `~/.axh/settings.json`). The card
  passes what is **left** of its session limits (CP-A6c), so a follow-up run cannot spend them twice. The meter
  (`session_limits.rs`) and the Flow tiles (`session_activity.rs`) read Glut's event log, which `harness card` writes
  as JSON lines into the channel (the `Usage` and `Command` events already exist). A stop from the limit watch is the
  run's `Cancel` (a stop file the run checks between steps), not a hook.
- **C7 — one conversation per card session:** after an answer the pane shows a prompt line; what the owner types goes
  on in the same conversation (kept in the channel, compacted by Glut robust). A card the reviewer returns starts the
  worker's next run **in the same conversation** with the review's notes as the new user message — Claude Code starts
  fresh there today.
- **C8 — D9 revisited, resume:** a card run on Glut keeps a journal like a skill run; when the app or the pane was
  lost mid-run, reopening the pane offers *Resume / Start fresh* (`harness card --resume`). The journal lives in the
  channel, the 7-day sweep of D8 applies.
- **C9 — not in this round:** the **master pane** on Glut (it is "the engine as the owner uses it outside" — that is
  the TUI's job), an Anthropic client for Glut, images.
  *The master pane is done 2026-10-23 (Glut TUI8c, `axiomata-cli harness master`).*

## Checkpoints

- **CP1 — engine and start:** C1, C2, C3, C4: `Harness::Glut` through roster, IDE, `start_agent` (`wire_glut`),
  `rosterLook.ts`; `harness card` with the start prompt and readable output; `--mcp-config`; sandbox hides the
  channel. Test: a scripted model claims nothing, reads the card, edits a file and calls `report_done`.
- **CP2 — asks and status:** C5; channel status (working / waiting / done) and the waiting tile; `session_activity`
  for Glut. Tests for the race of the two answers.
- **CP3 — budget and limits:** C6 in the harness (with tests in `run.rs`), the event log, the meter and the stop.
- **CP4 — conversation and resume:** C7, C8; a returned card continues the worker's conversation.
- **CP5 — live on the Mac:** one card end to end — worker Glut on DeepSeek V4.1 Flash, reviewer Claude Code, one
  return, take-over — and one plan of two cards running by itself on Glut; measure steps, tokens, cost.

## Built (2026-10-21)

- **CP1:** `Harness::Glut` (`glut`, a stored `mini` reads as it) through roster, IDE, status and the Studio's labels;
  `agent_entry::wire_glut` writes `mcp.json` (the team server with the secret in its own environment) and the spec
  `glut-card.json` (`glut_card::CardSpec`: folder, model, mode, rules, instructions, first message) into the channel;
  the pane types `axiomata-cli harness card`, which finds the spec by `AXIOMATA_AGENT_ID`. `axh --mcp-config` and
  `mcp::config::named`. The project's `AGENTS.md` joins the instructions (not for a reviewer). A Glut engine's model is
  the provider's bare id (the picker offers them; an Opencode spelling `openrouter/<model>` is taken too).
- **CP2:** `PaneAsker` — the question goes into `permission_asks` (badge, waiting tile, the app's dialog of #88
  unchanged) and into the pane; `permission_ask::answer_in_pane`, `withdraw`. The channel's state (working, waiting,
  idle, ended) is written by the CLI (`Channel::set_state`); the Flow tiles read `glut-events.jsonl`
  (`activity::glut_recent`, each line with its time).
- **CP3:** `Budget` in the harness (tokens less cache reads plus output, money at `Prices`), checked before each model
  call, `FinishReason::Budget`; a subagent gets what is left. `usage::GlutTally` meters the session; before each turn
  the CLI passes what is left (`session_limits::session_usage`, `left_of`) and ends the turn under way on the studio's
  stop file. `Event::Usage` carries `cached_input_tokens`.
- **CP4:** the conversation is kept in the channel (`glut-conversation.json`); a returned card reaches the idle pane
  through the existing nudge, so the worker goes on in the same conversation. Each turn journals into
  `glut-runs/`; a pane started over a lost turn **resumes it by itself** (not "offers Resume / Start fresh": a session
  nobody watches must not stand still on that question — Ctrl-C stops it), without a budget of its own (the journal
  counts on from what the lost turn used, which the session's reading counts already; the studio's stop still holds).
- **After the reviews:** one pane per session (a lock in the channel); the spec's folder and team-server file are
  checked against the database and the channel; a start on another harness forgets the spec, conversation and journals;
  the auto mode's deny list carries all of Claude Code's (the sandbox hides `~/.axiomata` and `~/.axh` as well, but not
  on every machine) and `git send-pack`; the app's question shows head and tail of a long command; a question nobody
  will answer is withdrawn; SIGTERM/SIGHUP write `ended`; a budget that cannot be read gives the turn nothing; a stop
  keeps the waiting message for when the limit is raised.
- **Left open:** Auto mode allows any network host (Claude Code's classifier weighs that; Glut's gate does not — an
  ask on `Network(*)` would stand an unattended session still); `harness card` cannot tell its pane from another
  process of the owner's that sets `AXIOMATA_AGENT_ID` (a2a.md A39 — not a sandbox); the meter prices Glut's tokens
  without the cache reads' cost; the event log grows without a bound; the app's question does not mark a call asked
  to run outside the sandbox (as the pane's does not either).

## Measured

- CP5 on the Mac: (to be filled in after the owner's test)

