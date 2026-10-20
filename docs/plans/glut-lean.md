# Glut lean — less goes to the model, nothing is cut away

Status: grilled 2026-10-20 (N1–N6, owner's answers below); built after the owner's go.

## Why

LocalBrain showed that with the providers in use old context is nearly free — cached input costs 3 % (DeepSeek V4.1
Flash) to 20 % (Ling 3.0 Flash) of the input price — so what saves money is a **stable conversation start** and
**less new text per step**, not rewriting what was already sent. The owner (2026-10-20): no cutting of results; build
our own compact outputs instead ("bei der Ausgabe eines Diff lässt sich was sparen, bei einem Git Status ebenfalls");
code mode is there and points the same way.

## Where we stand (main, 2026-10-20)

| Place | What happens | Lost? |
|---|---|---|
| `history.rs` (Glut 2, B/G6) | old long results shrink to a 1,200-character head + note, `read_full_result` gives them back | head only; behind `shrink_results`, **off** |
| `tools::cap` | every tool result over 30,000 characters is cut, "narrow the call" | **yes** |
| `bash` foreground | `Capture::keep_head(256 KB)`, then `[output cut]` | **yes — the end**, where `cargo test` puts its failures |
| `web_fetch` | pages in parts of 25,000 characters, `offset` reads on | no |
| code mode (G9) | over 40,000 characters: head + tail, the whole in the run's `ResultStore` | no |
| `/compact` | the model summarises on the owner's command, the old session is kept | by intent |

Already stable: the tool list does not change within a run (MCP tools behind code are found by search, Glut 2 E), and
the todo reminder rides in `todo_write`'s answer, not in the system prompt.

## Decisions (proposed)

- **N1 — G6 goes.** `history.rs`'s shrinking and the `shrink_results` switch are removed: cutting the owner does not
  want, behind a switch that stays off. `ResultStore` and `read_full_result` stay — N2 needs them.
- **N2 — relocate, never cut.** A tool result over the cap is not cut: the model gets its head and tail (as code mode
  already does) and a note with the id; the whole text goes into the run's `ResultStore`, `read_full_result` reads it
  on with `offset`. `read_full_result` is offered whenever a run has a store. `bash` keeps head **and** tail of its
  output instead of the head alone.
- **N3 — compact outputs for frequent commands, transparent in `bash`** (owner: no tools of their own — the model
  calls the commands it knows, Glut rewrites their output). Glut rewrites the *output*, never the command: lines without content go (progress, "Compiling …", "Downloading …", passing `test x … ok` lines,
  `index …` heads), everything else stays word for word, and the raw output goes into the store
  (`[compacted from N lines; the raw output: read_full_result <id>]`). First set:
  - `git status` → the short form with the branch line;
  - `git diff` → a file summary (+/−) first, then the hunks without `index` lines;
  - `git log` → one line a commit (hash, date, author, subject) unless the model asked for a format itself;
  - `cargo build|check|clippy|test`, `npm|pnpm install|test` → errors, warnings and the summary lines; on a failing
    test its own output whole;
  - `ls -l`/`find`/`tree` → long listings grouped by folder (counts for big folders), the raw listing in the store;
  - Python (owner: "die üblichen Verdächtigen"): `pytest` (passing dots and `PASSED` lines go, failures and the
    summary stay), `pip install`/`uv sync|add|pip install` (download and progress lines go), `ruff check`/`ruff format`,
    `mypy`/`pyright` (diagnostics and summary stay);
  - the Studio's other languages (owner: "andere Sprachen, die wir im Studio schon kennen" — tasks and grammars know
    Rust, TS/JS/Svelte, Python, Swift, C/C++, Lua, SQL, Bash): `tsc`, `svelte-check`, `vitest`/`jest` (passing
    tests go), `eslint`; `swift build|test` and `xcodebuild` (compile steps go, errors and the test summary stay);
    `make` and `cmake --build` (entering/leaving directory and progress percentages go, compiler diagnostics stay).
  A command with its own format flags (`--porcelain`, `--stat`, `--format=…`, `-q`) is left as it is.
- **N4 — an unchanged re-read is not sent again.** `read_file` of a file (or a range) that the conversation already
  holds whole and that has not changed since answers "unchanged since step N — it is above". Lossless as long as the
  earlier text is still in the conversation (after `/compact` it is not, so the file is sent again).
- **N5 — outline before reading** (owner: build it now): `read_file` with `outline: true` gives a file's declarations
  with line numbers (and the file's length), the model then reads the ranges it needs with `offset`/`limit`. Its own
  declaration reader per language (Rust, TS/JS/Svelte, Python, Swift, C/C++, Lua, Go; doc lines and attributes stay
  with their item, bodies go) — no new native dependency and no language server started for it (LocalBrain's lesson;
  the servers stay for diagnostics). A file of a language it does not know gets no outline but a clear answer. The
  tool's description tells the model to use it on files longer than a few hundred lines.
- **N6 — measure** with LocalBrain's CP4 set-up (own `AXIOMATA_HOME` per arm, a git worktree per code session, three
  7-turn code sessions per arm, news-web): money with the cached price, steps, input tokens, same result.

## Checkpoints

- **CP1 — N1 + N2**: G6 out, the cap relocates, `bash` head + tail; tests for "nothing lost" (every character readable
  back through `read_full_result`).
- **CP2 — N3**: the output filters (`tools/compact_output.rs` or similar), one table of commands; tests from real
  outputs of this repo (`git diff`, `cargo test` with a failure).
- **CP3 — N4**: the re-read check (file path + range + content hash per conversation).
- **CP4 — N5**: the outline, with tests per language from real files of this repo.
- **CP5 — N6**: measure; the plan records the numbers.
