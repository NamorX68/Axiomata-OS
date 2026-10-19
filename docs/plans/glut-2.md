# Glut 2 — fewer tokens by code, tools on demand, and what Opencode does well

Status: **grilled 2026-10-19** — owner decisions G1–G12 taken; ready to build on the owner's go. Follows `docs/plans/glut-round.md` ("Glut rund"); Pi Durable (runs that survive a crash) is its own plan
afterwards (G5), `LocalBrain` (summaries, a local classifier) too, together with point 4 of the owner's list.

## Why

Glut rund made Glut a harness to keep a role on, and measured where it still wastes: **news-web used 1.7 M input
tokens** (run #1712) because every fetched page rides along in every later step, and a chat turn offers **43 tool
schemas (~17k tokens) on every step**, 32 of them MCP tools the turn rarely needs. Pi 1.0 answers both with *code mode*
(`pi.dev/docs/latest/codemode`): the model writes a short script that calls tools as functions, and only what the script
prints comes back. And Opencode does four things for code work Glut does not yet: a todo list, the language server's
diagnostics after an edit, snapshots with undo, and a plan/build switch.

What Pi's code mode is (read 2026-10-19): a `codemode` tool taking raw JavaScript, run as an async function body in
**QuickJS** — no Node, no file system, no network, no timers; outside access only through `tools.<name>(args)` (the
session's tools, with the session's permissions) and `models` (classifiers, image models). `text()`, `console.*` and
`return` make the output; output over a budget (10k tokens) keeps head and tail and puts the whole into a temp file;
`store()`/`load()` keep small JSON across calls; deferred tools (MCP) are found with `searchTools()` /
`describeTool()`. 256 MB per VM, two workers per process, no nested scripts. On by default since 1.0 when an MCP server
is configured.

## Decisions (owner, 2026-10-19)

- **G1 — Glut 2 is B, A, E and D**: shorten old tool results (B), code mode (A), tools on demand (E), and from Opencode
  the todo tool, LSP diagnostics after edits, snapshots + undo and the plan/build switch (D).
- **G2 — QuickJS through `rquickjs`**, as Pi: complete modern JavaScript (models write it far more reliably than Rhai
  or Lua), memory and time limits and an interrupt built in. The C source is compiled in (no system package); the
  dependency gets the audit.
- **G3 — the core tools stay direct, MCP tools only through code**: `read_file`, `edit_file`, `write_file`, `bash`,
  `web_fetch`, `web_search`, `task`, `skill` are offered as today; MCP tools are no longer offered one by one but
  reached inside `codemode` and found there with `searchTools()` — E in the same step. Not a copy of Pi (Pi keeps them
  direct by default); Glut's own choice, because the MCP outputs are what rides along.
- **G4 — all four Opencode pieces.**
- **G5 — Pi Durable is a plan of its own** after Glut 2.

## Design

### B — old tool results shrink (CP1)

Once the model has answered after a tool result (the turn after it), a result longer than a threshold is replaced in
the history by its head, a note how much was cut, and an id. Relocation, never loss: the whole text stays in the run's
memory and `read_full_result { id, offset }` gives it back in parts. **Built 2026-10-20 (`history.rs`) — memory, not a
file** as first sketched: the file tools are confined to the work folder, and a run folder outside it would have needed
an exception in that guard; the store needs none and serves code mode's over-long outputs too. An id carries the run's
own tag, so a note left from an earlier chat turn answers "no longer available", never another run's text. Each message is shortened **once** and stays stable after, so the provider's prompt cache
breaks once at that point, not on every turn. Measured before and after on `news-web` and `calendar-digest`.

### A — code mode (CP2, CP3)

- **The tool** `codemode { code }`: the script is the body of an async function; top-level `await` and `return` work.
  Its description lists the offered core tools as TypeScript declarations and, for MCP, only the servers' names and a
  line how to find their tools (`searchTools(query)`, `describeTool(name)`), within a token budget.
- **The runtime**: one QuickJS context per call on a blocking worker, 256 MB, the run's stop flag as the interrupt, no
  time or call limit of its own (G9: the run's limits catch it); no `fetch`, no `require`, no timers. Globals: `tools.<name>(args)` → a promise of the tool's
  result (a string; JSON-shaped MCP results parsed), `text()`, `console.log/warn/error`, `return`, `store()`/`load()`
  (JSON, kept for the run), `searchTools()`, `describeTool()`.
- **Every inner call is a real call through the run's toolbox and gate** — the same rules, the same asks (headless an
  unanswered ask refuses, the promise rejects with the reason), the same events (marked as coming from the script), the
  same output cap per call. A tool the run does not offer cannot be called from a script either, except MCP tools that
  the rules allow (G3: they live behind the script). No nested `codemode`; `task` from a script is refused.
- **Output**: what the script prints and returns, in order; over the budget head and tail, the whole into the run's
  folder (as B). A failed script returns its partial output and the error.

### E — tools on demand (CP3)

Follows from G3: a chat turn's list shrinks from 43 schemas to the ~9 core ones plus `codemode`. MCP tools are found by
search inside the script. A skill that declares MCP tools gets them through code as well (Q2).

### D — from Opencode (CP4–CP6)

- **todo**: `todo_write { items: [{ text, status }] }` replaces the run's list; the list goes out as an event (the
  Studio can show it) and, when the run is long, a short reminder of open items rides in the system prompt.
- **plan/build**: `Mode::Plan` exists (Glut rund, `access: read`). A run in Plan mode can ask to switch with
  `exit_plan { plan }`: the plan goes to the owner as an ask (#88's way in the app); yes → Build mode, no → stays.
  Headless (a skill run) the ask is refused, the run stays in Plan.
- **Snapshots + undo**: before the first change of a run and before each edit/write/bash step, a snapshot of the work
  folder into a shadow git store outside it (`~/.axh/snapshots/<folder-hash>`, `--git-dir` + `--work-tree`, the
  user's `.gitignore` honoured, nothing written into the folder's own `.git`). `axh undo <run> [--step N]` restores; the
  app gets a button later. Large or binary files above a limit are skipped and named.
- **LSP diagnostics after edits**: a small LSP client in the harness (stdio, `initialize` → `didOpen`/`didChange` →
  wait for `publishDiagnostics` up to a short limit), the server per language from one table shared with the editor's;
  only servers that are installed, never installed on the fly; errors (not warnings) of the edited file appended to the
  edit's result. The server stays up for the run.

## Decisions from the grilling (owner, 2026-10-19)

- **G6 — B's threshold**: a result over 2,000 characters shrinks to its first 1,200 + a note + the path, once the model
  answered on it; the newest result stays whole.
- **G7 — a skill's declared MCP tools go through code too**: the digests are where the outputs ride along. (The
  declaration still decides what the gate allows inside the script.)
- **G8 — code mode is on for every Glut run**: `[agents.glut] codemode = true`; off, the MCP tools are offered directly
  as today.
- **G9 — Pi's limits**: 256 MB per script, no time or call limit of its own — the run's own limits (time, steps, spend)
  catch a runaway; 10k tokens of output, head and tail kept, the rest in the run's folder; two scripts at a time per
  process.
- **G10 — `models.classify()` not now**: it needs `LocalBrain` (point 4); Glut 2 leaves the hook.
- **G11 — snapshots kept 7 days**, then swept like the run logs.
- **G12 — a lean LSP client of the harness's own** (start the server, open/change the file, wait for diagnostics within
  a short limit, append the errors), the server table shared with the editor in one crate; **plan/build in the app
  chat by a switch**, and the model may ask to switch with `exit_plan` (an ask, as #88).

## Checkpoints

| CP | What |
|---|---|
| **CP0** | Measure before: `news-web` and `calendar-digest` on Glut (input tokens per step, tools offered); a chat turn's prompt size. |
| **CP1** | B: old tool results shrink, the full text in the run's folder; measured again. |
| **CP2** | A: QuickJS runtime (`rquickjs`, dependency audit), `codemode` tool, globals, limits, inner calls through toolbox and gate, events, output budget. |
| **CP3** | E + G3: MCP behind code (`searchTools`/`describeTool`), the declarations in the tool description, config switch; skills and chat measured. |
| **CP4** | D: `todo_write` and the plan/build switch (`exit_plan` as an ask). |
| **CP5** | D: snapshots before changes, `axh undo`. |
| **CP6** | D: LSP diagnostics after edits (client, language table, limits). |
| **CP7** | Docs (`harness.md`, `architecture.md`, `AGENTS.md`), live runs of the bundled skills on Glut, numbers before/after. |

Work happens in a git worktree and reaches `main` only while no skill, routine or chat turn runs (a Rust edit rebuilds
the owner's `cargo tauri dev` and cuts a running agent off). Glut stays a harness of its own (harness.md, standalone
constraint): everything here lives in `axiomata-harness` behind its config, reaching neither `~/.axiomata` nor the core.
