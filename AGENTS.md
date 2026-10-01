# AGENTS.md

Guidance for coding agents working in this repository — Opencode and Claude Code alike
(Claude Code reads it through `CLAUDE.md`, which imports this file and adds what is specific
to it). It is intentionally short and pointer-heavy — it is loaded in full on **every** turn,
so it only carries build/run commands, a compact current-state summary, and the handful of
non-obvious traps that would otherwise cost a whole debug session to rediscover. Everything
else (design rationale, full module-by-module walkthrough, milestone history) lives in
**[`docs/architecture.md`](docs/architecture.md)**, which is *not* auto-loaded — read it
before starting substantial new work, and **update it** (not just the paragraph below) when a
milestone or major feature lands. It went stale between M3 and M6 for exactly the opposite
reason once already; see its own maintenance note.

## Project status

Axiomata-OS is an early-stage Rust + Tauri desktop app: a personal "Agentic OS" command
centre / second brain, built around the **ARMS framework** (Applications, Routines, Memory,
Skills — see `ARMS-Agentic-OS-Guide.pdf`, though the actual design has since diverged from it
in several places; `docs/architecture.md` §1 explains how).

Milestones **M0–M3, M5, M6** are complete (scaffold, skills runner, memory router, routines
scheduler w/ full CRUD, the Svelte module-canvas dashboard, the particle-graph Second Brain),
plus ongoing post-M6 feature work (ToDo, Calendar/Reminders/Mail connector modules, themes,
the `srcdoc` HTML viewer, the Terminal module). **M4 (always-on/background scheduling) was
dropped outright** (owner, 2026-09-20 — the app being open, or tiles refreshing on open, is
enough); do not plan around it. Full detail: `docs/architecture.md` §5 (what exists) and §7
(milestone history). Detailed step-by-step milestone plans live outside this repo, in the
owner's local Claude Code planning notes — read them before starting new M0–M6-scale work if
available.

**M7 — the agentic IDE is under way** (`docs/plans/agentic-ide.md`): M7.1 is complete (the
`axiomata-ide` crate with projects, the dock-layout model, the full-screen IDE view, and
per-project layouts), and M7.2 is under way: agent profiles (CP4), a git worktree plus reserved port per agent
(CP5, the repo's first git integration — `git` as a subprocess, not `git2`), and a live status plus Plan tab
per agent (CP6/CP6b, `docs/plans/agent-lifecycle.md` — a file channel under `~/.axiomata/agent-events/`, no DB
column; nothing is ever written into a worktree). M7.2 is done; **M7.3 (git layer, `docs/plans/git-layer.md`)** is
under way: CP7's git engine (diff against the recorded base branch, discard, commit, take-over into the project
folder — squash by default, never a push), CP8's Diffs tab (drawn on the editor, H1–H16) and CP9 (file and
agent-diff dock panes, discard per file/hunk, commit, take-over dialogs) are built — M7.3 is complete. The plan below
describes the whole chain: an own full-screen IDE
view with a dock/split/tab layout, foreign agent harnesses (Claude Code, Opencode) hosted as
PTY tiles, A2A over an own MCP server rather than screen-scraping, one git worktree per
agent, a "Plan" tab per agent showing what it is working on, and an own mini-harness —
designed from the start to be extractable into a standalone app the way `axiomata-terminal`
is. Seven milestones (M7.0–M7.6), and **M7.0 is a standalone Kanban module that deliberately
ships before the IDE** — it is useful on its own and is the data layer the agents' task
board later sits on. The eight load-bearing decisions are settled in §3 of that plan.
**Under way:** the file app / own AAA editor (`docs/plans/editor.md`, decisions D1–D19 —
TS engine + `axiomata-files` crate, tree-sitter WASM, Vi mode, LSP, one App-Ring icon each for Editor and
IDE); it slots in before M7.3 CP8 (ED0–ED2 first, CP8's diff view is built on it). **ED0 (file service,
E1–E12) and ED1 (editor core, F1–F13) are done**: `axiomata-files`, root ids + dialog grants, `file_*`
commands, the watcher, the ring's "Ansicht öffnen" entries; the TS engine in `src/editor/` and the file
app in `src/fileapp/` (full-screen view, recovery, settings); **ED2** (tree-sitter highlighting, themes,
effects, Markdown preview) is done too. Per D15, M7.3 CP8/CP9 on the editor are done, and so is **ED3 (Vi, V1–V12)**: the machine in
`src/editor/vi/` (the `:` line lives inside it — `vi/cmdmode.ts`), wired to every surface, the Mac pasteboard
via `axiomata-macos::clipboard`, tree-sitter text objects (`editor/syntax/objects.ts`), and
`~/.axiomata/editor-vi.json` for registers, file marks and histories. **ED4 (single point of truth, W1–W17)**: ED4.1 replaced the Document tile/viewer (`md-file`) with the panel-only file panel
(`fileapp/FilePanel.svelte` around `FileEditor`, opened via `core/staging.ts` `openFilePanel`), and the agent
opens files for the owner through the shell action `openFile` (`core/registry.ts`, instance id `shell`).
ED4.2–ED4.6 are done too — **ED4 is complete**: the Second Brain shows files through `FilePeek`, tabs in the file
app (`fileapp/tabs.ts`; `src-tauri/src/menu.rs` drops the menu's ⌘W "Close Window"), the file tree on
`axiomata-files::dir` (renames and deletes are broadcast as `files:renamed`/`files:removed`), ⌘P quick open
(`axiomata-files::index`), and the IDE's Files pane (`ide/panes/FilesPane.svelte`, ⌘P over the project). **ED5
(tools, T1–T19)** is under way: ED5.1 put documents on an immutable rope (`editor/rope.ts`; `LineStore` stays as the
tests' reference) and made files up to 16 MiB editable, over 2 MiB in a "light mode" (`FileSession.light`, no
tree-sitter). ED5.2: every search pattern is first run by a worker with a 1 s limit (`editor/search/guard.ts`
`SearchGuard`, a verdict per rope version; Vi's key queue pauses on `SearchPending` like on the clipboard), plus Vi's
`:g`/`:v`/`:d`/`:normal`, `:s///c`, `gn`/`cgn`. ED5.3: multiple cursors in normal mode (`editor/multicursor.ts`:
a command runs at each cursor as one undo step; `EditorDocument.extra` + `extraGoals`). ED5.4: the find bar
(`fileapp/FindBar.svelte`, logic in `editor/search/findModel.ts`; the bar and Vi share the last search via
`fileapp/findShared.ts`). ED5.5: folding (`editor/fold/`: `ranges.ts` where the text folds, `FoldState` what is
folded; folded lines have no rows in `VisualLayout`; kept per file in `settings.editor.folds`,
`fileapp/foldMemory.ts`). ED5.6: sticky scroll (`editor/sticky.ts`) and the minimap (`editor/minimap.ts`,
`fileapp/Minimap.svelte`), off in the floating panel (`FileEditor` `compact`). ED5.7: the project search
(`axiomata-files::search`, Tauri `file_search` over an `ipc::Channel`, `fileapp/ProjectSearch.svelte` in the file
app's Files | Search column and the IDE's `search` pane, ⇧⌘F). ED5.8: installed Mac fonts
(`axiomata-macos::fonts`, CoreText through raw `unsafe extern` FFI; `core/installedFonts.ts`). ED5.9: moving in the
tree by dragging (`FileTree.svelte`, pointer events, `treeModel.moveTarget`) — **ED5 is complete**. **Opencode 2
(`docs/plans/opencode2.md`, OC1–OC4) is done**: Axiomata is a client of Opencode 2's shared background service
(crate `axiomata-opencode`) — skills and chat run as sessions on it, IDE Opencode agents start on a session the IDE
keeps (`opencode --session <id>`), and their status comes from the service's event stream. **ED6 (LSP) is under way**
(`docs/plans/editor.md` "ED6 im Detail", L0–L11): L0 refuses `$HOME` and above as a project root; ED6.1 runs language
servers from Rust (`axiomata-files::lsp` — which program, only from a built-in table or `~/.axiomata/lsp.json`; only
the methods the client speaks pass) with the protocol in `src/editor/lsp/`, and shows diagnostics; ED6.2 adds hover
and go-to-definition (a definition outside every root opens read-only on the root `lsp:<handle>`, readable only for
files the server named); ED6.3 adds implementation, type definition and uses (a list in the project search's
results); ED6.4 completion (a menu while typing, as blink.cmp in the owner's Neovim); ED6.5 formatting (own
formatter table in `axiomata-files::format`, after the owner's conform.nvim) and rename; ED6.6 signature help;
ED6.7 code actions (L18–L24: `workspace/executeCommand` only as an echo of a command the server offered in a
`textDocument/codeAction` answer, checked in Rust; `workspace/applyEdit` only while our command runs) —
**ED6 is complete** — and the owner's live test of ED6 was accepted on 2026-09-29, as were the ED5
live test and the ED2 colour sign-off. Nothing in ED0–ED6 is open.
Next in the editor: **ED7** (extraction into a standalone app, the engine already imports nothing
from the app — D1). Beyond that the editor has two follow-on plans of their own:
[`editor-look.md`](docs/plans/editor-look.md) (LK0–LK5, complete — the modern look becoming the
app's standard) and [`editor-projekt-werkzeuge.md`](docs/plans/editor-projekt-werkzeuge.md)
(project roots in the tree, outline, git panel, run/tasks, debug/DAP — grilled 2026-09-30, Q1–Q27;
**#48 git panel is built (status, stage/unstage incl. hunks, discard, commit, push, branches, `git init`; also as an IDE pane) (new crate `axiomata-git` — the only place that runs `git`, shared with `axiomata-ide`; panel in `fileapp/GitPanel.svelte`, change view `GitDiffView.svelte`, commands `src-tauri/src/git.rs`; Push / Commit & Push publish only the checked-out branch to its upstream or `origin`, never forced — the IDE's agent layer still never pushes). #49 outline is built too (`editor/syntax/outline.ts` from the syntax tree / Markdown headings, `fileapp/OutlinePanel.svelte` under the tree, breadcrumbs in the header). #47 is built as project new/open/close — one `projects` registry for editor and IDE, the folder from the native dialog (`project_open`/`project_new`), the tree shows the open project only; the rest stays parked**). `docs/plans/editor.md` lists both under „Fortschreibungen".
**Editor and IDE are becoming one workbench** (`docs/plans/workbench.md`, owner 2026-10-02: one app, a switch Editor / Agents, two layouts per project, agents keep running when switched away). Steps 1 and 2 are built — `ide/dockDrag.svelte.ts` and one shared `ide/DockNode.svelte` serve both views, and `fileapp/ProjectSidebar.svelte` (project bar, Files | Search | Git, outline) is the left column of both (own prefs per view: `settings.editor.tree` / `settings.ide.tree`; the IDE no longer starts with a Files pane, its header has no project picker). Steps 3 and 4 are built: the IDE view *is* the workbench — the editor's tab keys, unsaved-text question, rename-following, preview tab, ⌘N, file-panel handoff, and the **Editor | Agents switch** (two layouts per project in one `layout_json`, `ide/modes.ts`; the hidden mode's panes stay mounted and running). The ring's "Editor" and "IDE" entries both open it (`ide/modeRequest.ts`); the old `fileapp/FileAppView` is gone. Not yet live-tested on the Mac. Step 5 (naming, cleanup) and the path breadcrumbs remain.
Deferred meanwhile, by the same owner decision: the ⌘K spotlight search
(`docs/plans/spotlight-search.md`) and further model-provider work (the current Opencode +
OpenRouter setup is considered good enough).

## Commands

```sh
cargo build --workspace                    # build everything
cargo clippy --workspace -- -D warnings    # lint (must be warning-free)
cargo fmt                                  # format
cargo fmt --check                          # verify formatting in CI-style checks
cargo test --workspace                     # run all tests
cargo test -p axiomata-core                # run just the core engine's tests

cargo run -p axiomata-cli                  # headless: init the core, print status, exit
cargo run -p axiomata-cli -- list-skills   # discovered skills (~/.axiomata/skills/)
cargo run -p axiomata-cli -- run-skill <name>   # run a skill, print outcome, exit 1 if it failed
cargo run -p axiomata-cli -- list-runs --limit 20   # recent run history from the DB
cargo run -p axiomata-cli -- get-run <id>    # one full run record incl. captured stdout
cargo run -p axiomata-cli -- skills reseed [--force]  # re-copy bundled skills from resources/ (seed-if-absent unless --force)
cargo run -p axiomata-cli -- memory sync    # regenerate the workspace CLAUDE.md router blocks
cargo run -p axiomata-cli -- memory status  # is the router stale?
cargo run -p axiomata-cli -- routines list  # scheduled routines, soonest next-fire first
cargo run -p axiomata-cli -- routines add --name daily --cron '0 0 9 * * *' --skill <name>
cargo run -p axiomata-cli -- routines edit <id> --name … --cron … --skill|--prompt … [--backend …] [--disabled]
cargo run -p axiomata-cli -- routines delete <id>
cargo run -p axiomata-cli -- routines tick  # run one scheduler poll pass now (no 30s wait)
cargo run -p axiomata-cli -- board list [--board <id>] [--archived]   # boards, or one board's columns+cards
cargo run -p axiomata-cli -- board new <name>            # board + its three default columns
cargo run -p axiomata-cli -- board rename <id> <name>    # mirror follows, old file swept up
cargo run -p axiomata-cli -- board delete <id> [--force] # --force required once it holds cards
cargo run -p axiomata-cli -- board add --column <id> <title> [--label …]
cargo run -p axiomata-cli -- board edit <id> [--title …] [--body …] [--label … | --clear-labels]  # omitted flags keep their value; --label replaces all labels
cargo run -p axiomata-cli -- board move <id> --column <id> [--index <n>]
cargo run -p axiomata-cli -- board claim <id> [--actor human:owner]   # CAS; fails if already held
cargo run -p axiomata-cli -- board done <id>             # move into the board's done column
cargo run -p axiomata-cli -- board verify <id> [--actor …]  # refused for whoever claimed it
cargo run -p axiomata-cli -- board archive <id> [--undo]
cargo run -p axiomata-cli -- ide projects list          # IDE projects, most recently opened first
cargo run -p axiomata-cli -- ide projects new <name> <path>   # path is stored absolute + canonicalised
cargo run -p axiomata-cli -- ide projects rename <id> <name>
cargo run -p axiomata-cli -- ide projects set-root <id> <path>  # "Pfad ändern": keeps id + layout
cargo run -p axiomata-cli -- ide projects delete <id>   # removes the row only, never the folder
cargo run -p axiomata-cli -- ide agents list <project>  # a project's agent profiles
cargo run -p axiomata-cli -- ide agents new <project> <name> [--harness …] [--command …] [--model …]
cargo run -p axiomata-cli -- ide agents edit <id> [--name …] [--command …] …  # omitted flags keep their value
cargo run -p axiomata-cli -- ide agents delete <id>
cargo run -p axiomata-cli -- ide agents prepare <id>   # worktree + port (+ Opencode session), idempotent; prints where it runs
cargo run -p axiomata-cli -- ide agents new-session <id>  # an Opencode agent opens a fresh session on its next start
cargo run -p axiomata-cli -- ide agents discard-worktree <id> [--force]  # --force throws away uncommitted work
cargo run -p axiomata-cli -- ide agents status <project>  # state word + plan per agent, as the harness reported it
cargo run -p axiomata-cli -- ide agents diff <id> [--file <path>]   # what the agent changed since its base branch
cargo run -p axiomata-cli -- ide agents base <id> <path>            # a file as the agent's base has it
cargo run -p axiomata-cli -- ide agents commit <id> -m "…"          # commit what the agent left uncommitted
cargo run -p axiomata-cli -- ide agents discard <id> <paths…>        # put files back to the base (committed too)
cargo run -p axiomata-cli -- ide agents discard-hunk <id> <path> <n> # put the n-th hunk (from 0) back to the base
cargo run -p axiomata-cli -- ide agents take-over <id> -m "…" [--no-ff]  # into the project folder; squash by default, never pushes
cargo run -p axiomata-cli -- files roots          # file-service roots: workspace, project:<id>, worktree:<agent>, grant:<id>
cargo run -p axiomata-cli -- files read <root> <rel>   # through the editor's guard; version on stderr
cargo run -p axiomata-cli -- files write <root> <rel> [--expect <version>] < content   # Conflict if stale
cargo run -p axiomata-cli -- files grants list|add <path>|revoke <id>   # dialog grants (~/.axiomata/file-grants.json)
cargo run -p axiomata-cli -- files search <root> <pattern> [--regex] [--case] [--word] [--include g] [--exclude g]
cargo run -p axiomata-cli -- assistant "hi" [--resume <session_id>] [--instruct] [--allowed-tools <tools>]
cargo run -p axiomata-cli -- modules        # print the module manifest the dashboard wrote
cargo run -p axiomata-cli -- module-action <instance> <action> --json '{}'  # needs a running dashboard
cargo run -p axiomata-cli -- graph          # workspace graph summary (areas, links, skills, routines)
cargo run -p axiomata-cli -- import obsidian <folder> [--dry-run] [--skip-secrets]  # agent-sorted import

cd apps/dashboard && cargo tauri dev       # run the desktop app (hot-reloading dev mode)
cd apps/dashboard && npm run check         # svelte-check + tsc (must be clean)
cd apps/dashboard && npx vite --port 1420  # frontend alone in a browser: Tauri commands are
                                           # served by src/core/devmock.ts fixtures (DEV only)
cd apps/dashboard && npx vitest run        # frontend unit tests (pure TS logic, e.g. core/*.ts)
```

For browser-level checks (`agent-browser` against `vite --port 1420`) the mock backend
returns fixture data; anything that needs the real Rust side (persistence, file commands,
the agent) is verified by launching `cargo tauri dev` under a scratch `AXIOMATA_HOME`.

One-time setup for the Tauri app: `cargo install tauri-cli --version "^2" --locked`, and
`cd apps/dashboard && npm install`.

Dev-only: a local `apps/dashboard/.env.local` with `VITE_AXIOMATA_DISABLE_AUTO_REFRESH=true`
skips the calendar/mail/reminders modules' mount-time auto-refresh (each is a billed
`run_skill` agent turn) on repeated `cargo tauri dev` restarts — cached digests still load,
and each module's manual ↻ still runs regardless (`core/devFlags.ts`).

## Architecture — traps worth knowing before you touch things

Full walkthrough: `docs/architecture.md` §3–§5. The load-bearing facts that aren't obvious
from the code itself:

- **Runtime data lives outside the repo**, at `~/.axiomata/` (config, DB, logs, skills,
  dashboard layout — all app-owned), separate from the user's freely-chosen Second-Brain
  workspace folder (`config.workspace_root`, memory-router content only). Override with
  `AXIOMATA_HOME` in tests. New shared Cargo deps go in the root `Cargo.toml` under
  `[workspace.dependencies]`, referenced per-crate as `some_crate.workspace = true`.
- **Skills live in one place only**, `~/.axiomata/skills/<name>/SKILL.md` — there is no
  workspace-local skill location (dropped deliberately; `docs/architecture.md` §4 explains
  why).
- **Opencode 2 is a service, and Axiomata is its API client** (`docs/plans/opencode2.md`,
  crate `axiomata-opencode`; docs: https://opencode.ai/v2/docs, *not* `/docs`, that is v1).
  Skills and chat are sessions on the one shared background service (`POST /api/session`,
  prompt, wait on `/api/event`, read the messages back) — never parse a CLI's output again,
  never `--standalone` (it deadlocks on the shared database and starts its own MCP servers,
  apple-mail opening Mail.app each time). Unit tests cannot reach the real service
  (`cfg(test)` guard in `agents/opencode.rs`); live checks are `#[ignore]`d in
  `crates/axiomata-opencode/tests/live.rs`. After every Opencode update, check one real skill run.
- **Connector skills reach MCP through opencode's own config, not Axiomata's** — skill
  sessions run on the service (`AgentBackend::Opencode`), which resolves the
  MCP servers from `~/.config/opencode/opencode.json` (the same `apple-mail` /
  `apple-reminders` servers); tool use is pre-approved by the session's permission rules
  (`auto_approve_tools`, the former `--auto`). There is no
  `allowed_tools` allow-listing at runtime anymore (that was a Claude Code `--allowedTools`
  trap, §5 of `docs/architecture.md`); the frontmatter field is kept as documentation only.
- **Routines**: cron is the `cron` crate's **6–7 field, seconds-first** format
  (`0 */2 * * * *`), not 5-field crontab. `add`/`update`/`delete` all exist (`update` is a
  full replace, not a partial patch); the dashboard's Routines module UI uses a friendly
  interval picker (`core/routineInterval.ts`) over that cron, not a bare text field.
- **A connector module (Calendar/Reminders/Mail-shaped) is "provider = skill, not code"**:
  an MCP-backed `*-digest` skill, no live poll (every refresh is a real agent turn), writes
  go through a silent one-shot instruct turn, not the skill. Follow this pattern for the next
  integration rather than hand-rolling Tauri commands for it (`docs/architecture.md` §5).
  The digests run on the single agent harness — an Opencode session on whatever
  `skill_provider` routes to (`openrouter/deepseek/...`, `anthropic/claude-haiku-4-5`, or a
  local `ollama/<model>`), the same way opencode itself would run them; that harness is what
  replaced both the Claude Code CLI and the Stufe 2 `ollama-agent` tool loop.
  `mail-digest` also uses `prepend_files: ["Mail/.topics.md"]` to inline its workspace topics
  (opencode has no file tool for `mail-digest`'s workspace file).
- **The vault router is `AGENTS.md`** (was `CLAUDE.md`, 2026-09-28): `memory sync` writes the router block into
  `<workspace>/AGENTS.md` and `<area>/AGENTS.md`, renames an old `CLAUDE.md` router on the way (the owner's own text
  goes along) and leaves a `CLAUDE.md` that only imports `@AGENTS.md`, so Claude Code reads the same map. The chat's
  appended `module-context.md` now opens with a fixed guide to the app (`bridge.rs` `APP_GUIDE`: workspace, skills and
  their frontmatter, routines, the board, "ask when unclear, confirm before moving or deleting").
- **Bundled skills are seed-if-absent**: a `resources/<name>/SKILL.md` edit does **not**
  reach an install whose `~/.axiomata/skills/<name>/SKILL.md` already exists (the seed never
  overwrites). Bring it up to date with `cargo run -p axiomata-cli -- skills reseed --force`
  (re-copies only the bundled ones — the four digests/cleanup plus `inbox-sort` and `todo-to-kanban`; user skills untouched).
- **HTML/course pages render via `<iframe sandbox srcdoc=…>`, not `asset://`** — an
  `asset://` + `<iframe src=…>` design was tried first and never actually worked (silent
  WebKit sandboxing wall); don't re-attempt it without reading the postmortem in
  `docs/architecture.md` §5 first.
- **The Kanban board mirrors itself into the vault, one way only**: every board
  change rewrites `<workspace>/Kanban/<id>-<name>.md` (`core/board_mirror.rs`), so
  Obsidian, the Second-Brain search and the memory router can see a board without
  the app. **Nothing reads that file back** — the database is the board. A rename
  produces a new file name and the old one is swept up by id prefix; deleting a
  board removes its mirror. Writing it is best-effort and never fails the edit
  that triggered it.
- **File access for the editor goes through `axiomata-files`, never a raw path**: the webview
  names a file as `{ root, rel }` (`workspace` strict; `project:`/`worktree:`/`grant:` contained),
  every action walks from the root fd with `openat(O_NOFOLLOW)` (`pinned.rs`), and a new place
  on disk is reachable only through `file_pick` — the native dialog driven from Rust. Never grant
  `dialog:*` (or `fs:*`) in `capabilities/default.json`; that would let the webview forge picks.
- **The editor engine (`src/editor/`) imports nothing from the app** — no DOM, no Svelte, no
  `core/`; `src/fileapp/` depends on it, never the reverse (D1, extractable for ED7). Its
  `EditorDocument` is a mutable class: a Svelte component redraws via its own counter after
  every `doc.*` call, not via Svelte reactivity (`EditorSurface.svelte`'s header).
- **tree-sitter grammars are built, not downloaded at runtime**: `apps/dashboard/scripts/build-grammars.sh
  [name…]` (pinned tags, pinned `tree-sitter-cli`) writes `public/grammars/`, which is checked in. Never read
  a tree-sitter `node.text` — `web-tree-sitter` re-calls the parse callback with a stale position; slice the
  `TextStore` instead (`syntax/highlighter.ts`). The CSP's `'wasm-unsafe-eval'` exists for tree-sitter.
- **Themes**: every colour/size in a Svelte component goes through a `--ax-*` token
  (`themes/tokens.css`) — no literals. A user's `~/.axiomata/theme.css` is validated
  (`:root { --ax-*: … }` only) before injection.
- **UI scale** (`docs/plans/editor-look.md`, K9–K11): every UI size is `N px × var(--ax-ui-scale)`
  (tokens already are; a size that is no token is written `calc(Npx * var(--ax-ui-scale))`), set by
  `core/uiScale.ts` from the display's real density (`axiomata-macos::display`) or the "UI size"
  setting. The canvas, the floating panels and the file tree store **unscaled** sizes and draw them
  times `$uiScale` — divide pointer deltas by it. Not scaled: the editor's and terminal's fonts, and
  anything measured in editor character cells. Script that reads a size token gets the unresolved
  `calc(…)`, not a number. UI icons: `ui/Icon.svelte`/`ui/IconButton.svelte` over the Lucide subset
  vendored by `scripts/vendor-icons.sh`.
- **Backend ids + the one frontmatter bridge**: `backend:` is `opencode` (default) |
  `ollama`; the only other relevant field is `prepend_files: ["rel/path.md"]` (workspace files
  inlined into the prompt as `## Context file:` blocks; missing = skipped, `..`/absolute =
  run fails as a recorded `Failed`, per §D of the Stufe 2 plan). The `local_backend` /
  `effective_backend` mechanism and the `ollama-agent` backend are gone.
- **Model / provider**: the provider is chosen **per role** — `agents.chat_provider` for
  interactive chat, `agents.skill_provider` for skill/routine runs — resolved via
  `AgentDefaults::provider_for(ProviderRole::{Chat,Skill})`. Every Opencode session gets
  the model `provider/<model>` from *its role's* provider (`providers[chat_provider].chat_model`
  / `providers[skill_provider].skill_model`; a skill's own `model:` frontmatter still wins),
  and opencode resolves the provider's auth/keys itself from its own credential store — there
  is no `ANTHROPIC_*` env plumbing anymore. `guard_redirected_turn(db, config, role)` still
  gates the paid roles, metering them from token counts × an owner-set per-model price table
  (`config.agents.costs` / `spend::metered_cost_usd`) rather than the CLI's own estimate —
  `runs.model` (migration 0007) records the model so `spend::reconcile_recorded_costs` can
  re-meter already-recorded runs at startup. v1 providers: `Anthropic`
  (default; no base URL/key, subscription-billed via the CLI login) | `OpenRouter` | `Ollama`
  — all `ProviderSettings` fields kept per provider even while unused. Anthropic's defaults
  are `claude-sonnet-5` (chat) / `claude-haiku-4-5` (skills). The old flat `agents.claude_model`
  and single `agents.active_provider` are both migration-only now (`active_provider` folds
  into both role fields on load, then `save()` drops it). Details: `docs/architecture.md` §5
  "Model providers"; `docs/plans/per-role-provider.md`;
  `docs/plans/settings-provider-overhaul.md`.

## Sub-agents (use the Rust variants, not the Python-oriented defaults)

The owner's global `~/.claude/CLAUDE.md` defines mandatory automatic sub-agent triggers. Three of
the named agents there (`test-engineer`, `dependency-auditor`, `performance-analyzer`) are worded
for a Python/`uv` stack and **do not apply to this repo**. Global, Rust-flavored replacements exist
at `~/.claude/agents/{rust-test-engineer,rust-dependency-auditor,rust-performance-analyzer}.md`
(usable in any Rust project) — use those instead, same trigger conditions translated to Rust terms
(`cargo test`, `cargo audit`, `rusqlite`/`tokio`).

**Cadence override** (owner, 2026-09-08; rationale in `docs/architecture.md` §7): fire them **once
per plan checkpoint, and always before a commit**, not after every single changed
`fn`/`struct`/`Cargo.toml` line mid-task. Keep writing/updating tests inline as code lands
regardless — the test engineer's run is a bundled second-pass gap check over the accumulated diff,
not the first pass. The same override applies to an agent's own verification loop: batch
`cargo build`/`clippy`/`fmt`/`test` per unit of work, and run the full set only when the work is
done, before handing off to a sub-agent, and before a commit.

**Lean review cadence** (owner, 2026-09-27, to save resources; reversible): reviews run only before
a commit, and small checkpoints may be bundled into one commit. Instead of separate architecture +
test-gap agents, **one** combined review agent (model `sonnet`, narrow brief: defects and
convention breaks only, short report) covers design and missing tests; the tests themselves are
written inline as code lands. `security-auditor` still runs in full where a plan asks for it or the
global trigger applies (auth, external paths, new endpoints, …).

`architecture-reviewer`, `security-auditor`, `docs-writer`, and `refactoring-specialist` are
already language-agnostic as globally defined and apply here unchanged. When
`architecture-reviewer` or another background sub-agent run is not available (e.g. a session rate
limit), do a manual review pass yourself rather than skipping the check — the trigger is
mandatory, not the specific tool.
