# Opencode's exit (Glut TUI plan T22)

Status: **grilled 2026-10-23 (owner)**; OX1 built and tested by the owner, OX2 built 2026-10-23; the folder
`~/.axiomata/opencode` waits for the owner's yes.

Opencode came in as the one harness for skills, the chat and card sessions (`opencode2.md`). Since then Glut does all
three (`harness-choice.md`, `glut-cards.md`) and has a terminal UI of its own (`../Axiomata-Glut/docs/plans/glut-tui.md`,
TUI0–TUI8), Claude Code stays beside it. What is left of Opencode is weight: three service instances of the app's own
(`write`, `read`, `master`), their generated configurations, MCP registration and reconnection, a status tracker, its
usage and activity readers — about 8,000 lines, and 904 MB under `~/.axiomata/opencode`.

## Measured before the grill (2026-10-23)

- **The owner's harnesses** are Glut already: `chat_harness = skill_harness = "axh"` in `~/.axiomata/config.toml`.
  The built-in default is still Opencode (`HarnessId`'s `#[default]`).
- **The connector digests run on Glut**: 41 runs of `calendar-digest`, `mail-digest`, `reminders-digest` on `axh`
  since 2026-10-19, all `success` — Glut reads `~/.axh/mcp.json` like the app's Opencode instances did.
- **11 engines** of the catalog are `harness = "opencode"` (one without a model, three `ollama/…`, seven
  `openrouter/…`); **7 roles** name one of them as `engine` or fallback (implementer-light, documenter, reviewer,
  planner-mimo, grill-mimo, reviewer-heavy, reviewer-light).
- **14 Studio sessions** (`ide_agents`) are on Opencode, one of them the master.
- A Glut engine's model always goes to the chat provider (`glut_card::endpoint`): an `ollama/…` model would be sent to
  OpenRouter.
- A plain `opencode` attaches to the owner's long-running service (`opencode serve --service`); a configuration given
  by `OPENCODE_CONFIG` / `OPENCODE_CONFIG_CONTENT` does not reach it (measured with a real MCP server under a new
  name: not listed). A master with the team tools therefore needs a service of the app's own.

## Decisions

- **O1 — Opencode leaves, except as a plain master tile.** The owner starts Opencode now and then for its free models:
  the Agents panel keeps an Opencode master that runs the owner's own `opencode` in the project folder — their config,
  their service, **no team tools** (that would keep the app's own service, ~2–3k lines, alive for one tile). Everything
  else goes: skill, chat and card sessions on Opencode, the app's instances, the crate `axiomata-opencode`.
- **O2 — Engines become Glut engines with the same model** (`openrouter/x` stays as it is written — Glut reads the
  Opencode spelling —, `ollama/x` goes to Ollama). The roles keep their engine ids. The owner sees the list before it
  is written. An engine without a model takes the chat provider's model, as a Glut engine without one does.
- **O3 — Old values are read, not refused**: a stored `opencode` harness (config, `runs.backend`, a routine's
  `backend`, a skill's `backend:`) means the role's harness of today, as the retired `claude-code` / `ollama-agent`
  ids do already; a non-master Studio session on Opencode becomes a Glut session at its next start (its worktree and
  branch stay), its Opencode session id is forgotten.
- **O4 — The app's own Opencode services are stopped** at the first start without them (they were left running when
  the app quit). `~/.axiomata/opencode` is **not** deleted by the app: it is deleted after the owner's test and yes.
- **O5 — In two steps**: OX1 switches (everything runs without Opencode, its code still there), a Mac test, OX2
  removes the code.

## Steps

- **OX1 — switch.**
  1. The built-in default harness is Glut (`HarnessId`'s default; the CLI's `--harness`; the engine template in
     `core/roster.ts`); Opencode is no longer offered for chat, skills, routines or a new engine.
  2. A Glut engine's model names its provider: `ollama/…` → Ollama, `openrouter/…` → OpenRouter, otherwise the chat
     provider (`glut_card::endpoint`).
  3. A config migration on load turns every `harness = "opencode"` engine into `glut` (O2) and saves it through
     `roster::persist_engines`; the list is shown to the owner before the first start that writes it.
  4. Reading old values (O3): `HarnessId` / `AgentBackend` / roster `Harness` take `opencode` as before but resolve it
     to today's harness; a non-master session row on Opencode starts as Glut.
  5. The app's Opencode services are stopped at start (O4); none is started any more except by OX2's plain master.
  6. Tests for each; docs (`AGENTS.md`'s Opencode traps marked as leaving).
- **Mac test** of OX1: chat, a digest, a card on a former Opencode engine (reviewer, planner-mimo), the Opencode master.
- **OX2 — remove.** The crate `axiomata-opencode`, `agents/opencode.rs`, `opencode_instance.rs`, `ide_status.rs`'s
  tracker, the Opencode parts of `session_limits`, `usage`, `activity`, `session_activity`, `studio_spend`,
  `card_session`, `plan_session`, `ide_start` (`reach`, `Registration`), the Tauri `ide_agent_new_session`, the UI's
  Opencode choices and its panes' "New session"; the Opencode master becomes the plain `opencode` command (no
  instance, no env prefix, no MCP). The frozen migration and the `opencode_session` column stay in the schema history.
  `AGENTS.md`, `architecture.md` and the plans that describe Opencode as a part get their closing note.
- **OX2 — built 2026-10-23.** Gone: the crate `axiomata-opencode`, `agents/opencode.rs`, `opencode_instance.rs`, the
  service tracker of `ide_status.rs` (what is left are the waiting agents and the answers from the app), the Opencode
  parts of `session_limits` (no interrupt, no service metering), `session_activity`, `card_session` and `plan_session`
  (no MCP registration to take back), `ide_start` (`reach`, `Registration`, `new_session`), `axiomata-ide`'s Opencode
  readers, the Tauri `ide_session_reach` / `ide_agent_new_session`, the panes' reach and "New session", the
  `HarnessId::Opencode` variant (an `opencode` in a config is Glut's serde alias, and the load gives an Anthropic role
  Claude Code). Kept apart and moved: the JSON reply helpers (`agents/json_reply.rs`) and the read-only command lists
  (`agents/read_access.rs`) that Glut and Claude Code share. The app no longer stops Opencode services at start: OX1's
  start already did on the owner's machine.
- **After the owner's yes:** `~/.axiomata/opencode` deleted.
