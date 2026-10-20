# Slash commands in the app chat (card #108)

Status: grilled 2026-10-20 (S1–S12), waiting for the owner's go.

## Where we stand

- The assistant bar already routes `/` (`core/commands.ts`): 8 shell commands (`/add`, `/remove`, `/theme`, `/open`,
  `/newfile`, `/skill run`, `/brain`, `/help`) plus `/<module> <action>` for 13 module types — 21 names on one level.
  Any other `/text` becomes a one-shot `instruct` turn (billed). ↑/↓ walk the input history; there is no suggestion list.
- Glut's slash commands (HS4, `axiomata-harness::commands`) exist only in the terminal (`axh`, `axiomata-cli harness
  run`): files under `~/.axh/commands/` and a trusted project's `.axh/commands/`, built-ins `/help`, `/permissions`;
  `/clear`, `/model`, `/compact` only answer "not available in a single run yet". The app chat (`agents/axh.rs`) does
  not use the registry.
- The engines can tell what they have (measured 2026-10-20):
  - **Opencode**: `GET /api/command` lists the server's commands with descriptions (today `init`, `review`, the MCP
    prompts as `apple-reminders:…`); `POST /api/session/{id}/command` runs one; `…/compact`, `…/revert/stage|commit`,
    `DELETE …/revert` are endpoints of their own. TUI-only commands (`/new`, `/models`, `/themes`) are not in the API.
  - **Claude Code**: the first `stream-json` event (`system`/`init`) carries `slash_commands` and `skills` — ~150 with
    the app's login, many of them interactive only (`color`, `config`, `fast`).
- `ChatRequest.model` already exists, so a per-conversation model needs no new plumbing.

## Decisions

- **S1 — three levels.**
  1. **Conversation commands**, the same name on all three engines, the app maps each to the engine: `/help`, `/clear`,
     `/compact`, `/model`, `/plan`, `/build`, `/skills`, `/cost`, `/resume`, `/undo`, `/do`.
  2. **App control**: the module commands move under `/tile` (`/tile add <type>`, `/tile remove <type|id>`,
     `/tile <module> <action> [json]`), freeing 13 names; `/theme`, `/open`, `/newfile`, `/brain` stay.
  3. **Engine commands** keep their own name (`/review`, `/init`, the owner's files, skills).
- **S2 — clashes**: levels 1 and 2 win; a clashing engine command is reached as `/cc:<name>`, `/oc:<name>` or
  `/glut:<name>` and listed that way.
- **S3 — unknown `/x` is refused** with "unknown command" and the nearest names; the one-shot instruction becomes
  explicit: `/do <text>`. A typo never costs a turn. The old `/<module> …` form answers with the hint "now
  `/tile <module> …`" (one release, then it goes).
- **S4 — `/skill run <name>` → `/skills run <name>`**; `/skills` alone lists the skills (name, description, harness).
- **S5 — `/model <id>` holds for this conversation only** (until `/clear` / New session); `chat_model` is untouched.
  `/model` alone shows the current model and the allowed ones (the model catalog, #17).
- **S6 — Claude Code's list shows only checked commands**: CP0 measures which work in `-p` with the app's flags
  (`--setting-sources ""`, `--strict-mcp-config`); a fixed allow-list of built-ins (e.g. `compact`) plus every skill
  and command file — interactive built-ins are filtered out.
- **S7 — where the lists come from**: Glut from its registry (user files; project files only where trusted, as in the
  terminal); Opencode from `GET /api/command` of the chat's instance; Claude Code from the `init` event of the last
  chat turn, cached per login folder (no extra call; before the first turn the list holds only level 1/2).
- **S8 — running an engine command**: Glut expands it through the registry into the turn's prompt; Opencode
  `POST …/command`; Claude Code sends `/name args` as the prompt (CP0 confirms per kind).
- **S9 — the per-engine meaning of level 1** (where an engine cannot, the command says so plainly, never pretends):

  | | Glut | Opencode | Claude Code |
  |---|---|---|---|
  | `/clear` | new session | new session | new session |
  | `/compact` | **new**: the model summarises the history, the session file keeps the summary + the last turns | `POST …/compact` | `/compact` as prompt (CP0) |
  | `/model` | `ChatRequest.model` | same | same |
  | `/plan` `/build` | the switch (`planMode`) | same | same |
  | `/cost` | tokens + money of this session from `runs` | same | same |
  | `/resume` | pick an earlier session file | `GET` its sessions | its sessions of the app's login |
  | `/undo` | the last turn's snapshot (`snapshot::restore`) | `…/revert` of the last message | not available (no rewind in `-p`) |

- **S10 — the suggestion list**: typing `/` opens a list above the bar (name, source chip `app`/`glut`/`oc`/`cc`,
  argument hint, description), filtered by prefix; ↑/↓ move in it while it is open (history otherwise), Tab/Enter
  takes, Esc closes. `ui/` family and tokens, no new look.
- **S11 — only the chat bar.** Card sessions and terminal panes keep the engines' own TUIs.
- **S12 — nothing a list says starts a process**: names and descriptions from an engine are data (shown, never
  `{@html}`); running one goes through the same chat path and gate as a typed message.

## Checkpoints

- **CP0 — measure** (no product code): which Claude Code commands work in `-p` with the app's flags (a skill, a command
  file, `compact`, `context`); the list with those flags; Opencode `POST …/command` and `…/compact` on the chat
  instance; what `/compact` leaves in each engine's session.
- **CP1 — router** (`core/commands.ts`): the three levels, `/tile`, `/skills [run]`, `/do`, unknown refused with nearest
  names, the old-form hint, `/help` listing every level; Vitest.
- **CP2 — command catalog** (backend): a Tauri command `chat_commands` → `[{name, source, hint, description}]` per the
  chat's harness (S7), the clash prefixes (S2); running an engine command (S8). Security review (new Tauri command).
- **CP3 — suggestion list** (`AssistantBar.svelte`, pure filtering logic in a `.ts` with tests).
- **CP4 — conversation commands I**: `/clear`, `/model`, `/plan`, `/build`, `/skills`, `/cost`.
- **CP5 — conversation commands II**: `/compact` (Glut's own compaction in `axiomata-harness`, Opencode endpoint, Claude
  prompt), `/resume`, `/undo`; Glut's built-ins `/clear`, `/model`, `/compact` then also work in the terminal where a
  conversation exists.
- **CP6 — docs + review**: `architecture.md` §5/§7, AGENTS.md, the combined review, security-auditor.
