# Glut lean — less goes to the model, nothing is cut away

Status: grilled 2026-10-20 (N1–N6, owner's answers below); **CP1–CP5 done** (2026-10-20, branch `glut-lean`); see
"CP5 — measured".

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
  declaration reader per language (Rust, TS/JS/Svelte, Python, Swift, C/C++, Lua, Go; the declaration's own line,
  bodies, doc lines and attributes go) — no new native dependency and no language server started for it (LocalBrain's lesson;
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

## CP5 — measured 2026-10-20

The chat on DeepSeek V4.1 Flash, each session in its own `AXIOMATA_HOME` and git worktree; A = `main` before this
plan, B = Glut lean. Money with the cached price.

| Task | A $ | B $ | A steps | B steps | B: compacted / unchanged / relocated / outline |
|---|---|---|---|---|---|
| 7-turn code session on `axiomata-tasks` (×3) | 0.0067 / 0.0082 / 0.0360¹ | 0.0073 / 0.0208 / 0.0123 | 36 / 38 / 56 | 39 / 73 / 58 | 1 / 0 / 0 / 0 in all three |
| 4-turn questions on `run.rs` (3,400 lines), `shell.rs`, `permissions` (×2) | 0.0084² / 0.0078 | 0.0111 / 0.0093 | 33 / 30 | 25 / 30 | 0 / 0 / 1 / 4 |

¹ two of seven turns timed out at 300 s; ² two of four turns failed (a 300 s time-out, a refused command) — both in A,
neither caused by this plan. Every code session ended with the tests green and clippy clean.

- **The mechanisms seldom fire with this model**: DeepSeek already filters its own commands (`cargo test 2>&1 |
  tail`, `cargo fmt && cargo test`), and N3 leaves pipes and chains alone by design; it re-reads files after editing
  them (so never word for word); the outline is used on large files (4 times on `run.rs`), not on 250-line ones.
- **The costs differ within the runs' own scatter** (36–73 steps for the same task): no measurable saving, no harm.
- **What it is worth anyway**: N1/N2 mean nothing is lost any more — no result is cut, `bash` keeps the end of a long
  output, and a chat's later turns read relocated texts back. N3–N5 pay where a model does not filter itself (skills
  on Ling, a role with another model) and in larger repositories; they cost nothing where they do not fire.
- **Next, if wanted**: N3 for a chain of commands of one family (`cargo fmt && cargo test`), the commonest form here.

## What building it taught (2026-10-20)

- **A diff is mostly content**: on this repo's commits ~80 % of a code diff are changed lines, which all stay, so the
  `index`/`---`/`+++` lines alone saved under 20 % and the diff filter also trims unchanged lines to one around each
  change; diffs are compacted from a 10 % saving on (other outputs from 20 %). A small edit in a large file saves most.
- **Many outputs are already lean**: vitest's non-interactive summary and svelte-check's diagnostics stay whole — the
  filter only pays where progress or passing lines dominate (cargo, swift, xcodebuild, pip/uv, pnpm, cmake, listings).
- **Review findings fixed before the commit**: `read_full_result`'s answer was bounded a second time (its middle was
  hidden); `git show` lost its commit header and `git show rev:path` its file; `[exit code N]` and the `error: ` prefix
  got lost in the git filters; `find -exec cat {} +` had another program's output treated as a listing; the model's
  own `-U10`/`-W` and a diff's trailing spaces were not respected; a relocated text was gone in a chat's next turn
  (now a chat's store sits beside its session file); a one-line file's pointer was longer than the file; a character
  at the seam of `bash`'s head and tail was broken.
