# HS4 — slash commands: build instructions

Self-contained instructions for building step HS4 of the harness (`docs/plans/harness.md`, board card #37). Written
2026-10-13 so that another model can build it without the conversation that decided it. Decisions are final (owner,
2026-10-13: "so bauen, aufbohren können wir es immer noch") — do not redesign, do not add features beyond this file.

## Where and how

- Work in the git worktree `.claude/worktrees/harness`, branch `worktree-harness` (the owner's `cargo tauri dev` runs
  on `main`; never edit the main checkout). Base: commit `c0f9608` (HS3b).
- Crate `crates/axiomata-harness` (library + binary `axh`), plus the CLI entry `crates/axiomata-cli/src/harness_cmd.rs`.
- The crate must **not** depend on `axiomata-core`: `cargo tree -p axiomata-harness | grep axiomata-core` stays empty.
- No new dependencies unless unavoidable. Frontmatter: parse it by hand (a `---` block of `key: value` lines, see
  below) — do not add `gray_matter` or `serde_yaml` for this step.
- Read these first, they show the conventions: `src/permissions/settings.rs` (how `~/.axh` and `<project>/.axh` are
  read, `Trust`, `deny_unknown_fields`), `src/permissions/mod.rs` (`Rule`, `RuleSet`, `Gate`, `build_gate`,
  `printable`), `src/tools/mod.rs` (`Toolbox`, `Tool`, `CallContext`), `src/event.rs`, `src/run.rs`, `src/bin/axh.rs`.
- Style (from `AGENTS.md` and `~/.claude/rules/rust.md`): edition 2024, max line 120, `///` on every public item, `//!`
  per module, comments say *why*, English only, no `unwrap()` outside tests, errors through `HarnessError`, named
  `const`s for limits. Tests inline (`#[cfg(test)] mod tests`), temp folders unique per test, never the real `$HOME`.
- Quality gate, must be green before the commit:
  `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- One commit at the end: `feat(harness): slash commands, step HS4 (#37)`, ending with a `Co-Authored-By:` line naming
  the model that built it. Do not push, do not merge into `main`.

## What to build

### 1. Module `src/commands/mod.rs` (new; add `pub mod commands;` to `src/lib.rs`)

```rust
/// Where a command comes from.
pub enum Source { Builtin, User, Project }

/// One custom command read from a Markdown file.
pub struct Command {
    pub name: String,            // file stem, lowercase: `review.md` -> "review"; a subfolder becomes a prefix "git:commit"
    pub source: Source,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub allowed_tools: Vec<Rule>, // parsed with Rule::parse; a rule that does not parse makes the file an error
    pub model: Option<String>,
    pub body: String,            // the text after the frontmatter
}

/// Every command a run knows: built-ins first, then the files.
pub struct Registry { /* … */ }
```

- `Registry::load(home: &Path, project: &Path, trusted: bool) -> Result<Registry, HarnessError>`:
  - reads `home/.axh/commands/**/*.md` (source `User`) and, **only if `trusted`**, `project/.axh/commands/**/*.md`
    (source `Project`) — the same trust as the project's allow rules (`settings.rs` P5);
  - a project command with the same name as a user command wins (like Claude Code); a **built-in always wins** over
    both, and a file shadowed by a built-in is skipped (no error);
  - limits as named consts: at most 200 command files, at most 64 KiB per file, nesting depth 3; beyond that → error
    `HarnessError::Settings`;
  - read files through `axiomata_files` with `LinkPolicy::Contained` (as `tools/files.rs` does) so a link cannot lead
    outside `.axh/commands/`.
- Command names: `[a-z0-9_-]+` per segment, segments joined by `:`. Other names → error naming the file.
- Frontmatter: optional; if the file starts with a line `---`, everything up to the next `---` line is `key: value`
  lines. Known keys: `description`, `argument-hint`, `allowed-tools` (comma-separated rules, e.g.
  `Bash(git status:*), Read`), `model`. Unknown keys → error (same spirit as `deny_unknown_fields`). Values may be
  wrapped in `"…"`; strip one pair of quotes.

### 2. Expanding a command: `Registry::expand(&self, input: &str) -> Option<Result<Expanded, String>>`

- `None` when `input` does not start with `/` — it is a normal prompt.
- `/name rest of line` → look up `name`. Unknown → `Some(Err("unknown command /name; /help lists them"))`.
- Arguments: `$ARGUMENTS` = the whole rest (trimmed); `$1`, `$2`, … = whitespace-separated words of it (a missing
  one becomes empty). Replace in the body; nothing else is substituted.
- `@path` in the body (an `@` at the start or after whitespace, followed by a relative path): **do not** read the
  file. Leave it in the text. (Reading `@file` is deferred; the model can call `read_file`.)
- Lines starting with `!` (Claude's pre-run shell lines): **not supported in HS4.** If the body has one, return
  `Some(Err("/name uses !command lines, which the harness does not run yet"))`. This is deliberate — they would need
  the gate and the sandbox; we add them later.
- `Expanded { name, prompt: String, allowed_tools: Vec<Rule>, model: Option<String> }`.

### 3. `allowed-tools` only narrows

- When the expanded command has `allowed_tools`, the run may use **only** tools those rules name — on top of all
  existing rules, never instead of them. Implementation: add to `Gate` (in `permissions/mod.rs`) an optional
  `restrict: Option<Vec<Rule>>`; in `Gate::judge`, if `restrict` is set and no rule in it matches the call
  (`Rule::matches` / `matches_path`, same matching as allow rules), refuse with reason
  `"the command /name allows only: <rules>"`. If it matches, continue with the normal judgement (deny/ask/allow,
  mode) — **a restrict rule never allows anything by itself**.
- `Access::Own` (`bash_output`, `kill_shell`) stays always allowed.
- Test: a command with `allowed-tools: Read` → `read_file` judged normally, `bash` refused, `write_file` refused;
  a command with `allowed-tools: Bash(git status:*)` in mode `plan` → `git status` still refused by `plan`.

### 4. Built-in commands

Implemented in Rust in `src/commands/builtin.rs`, not as files:

- `/help` — lists every command: name, source (`built-in`, `user`, `project`), `argument-hint`, `description`.
- `/permissions` — shows the run's mode and its allow / ask / deny rules (and the sandbox settings: enabled,
  allowWrite, denyRead, allowLocalBinding), each line passed through `permissions::printable`.
- Both are answered **without calling the model**: the result is text for the front end.
- Reserve the names `clear`, `model`, `compact` as built-ins that answer `"/clear is not available in a single run
  yet"` (and so on) — they come with the TUI; reserving them now stops a file from taking the names.

`Registry::expand` returns for a built-in: `Some(Ok(Expanded::Builtin(text)))` — so make `Expanded` an enum:
`Expanded::Prompt { name, prompt, allowed_tools, model }` | `Expanded::Builtin { name, text }`.

### 5. Wiring

- `event.rs`: new event `Event::Command { name: String, source: String, prompt_chars: usize }`, serialized like the
  others (tagged, snake_case: `{"type":"command",…}`), emitted before the first model call when a command expanded.
  For a built-in: emit `Event::Command` and then `Event::AssistantText { text }` with the built-in's text, then
  `Event::Finished` — no model call.
- `bin/axh.rs` (`axh run … -- "/review src/"`) and `harness_cmd.rs` (`axiomata-cli harness run "/review src/"`):
  build the `Registry` after the gate (same `home`, `cwd`, `trusted` as `build_gate`), then:
  - `None` → as today;
  - `Some(Err(msg))` → print the message, exit code 1 (axh) / error (CLI), no model call;
  - `Some(Ok(Builtin))` → print the text (CLI) / emit the events (axh), exit 0;
  - `Some(Ok(Prompt))` → run with `prompt` as the user prompt, the command's `model` overriding the default model if
    set, and `gate.restrict = allowed_tools` if non-empty.
- `--json` of the CLI shows the `command` event like the other events; the readable output prints `/name (source)`.

### 6. Tests (inline, all must pass)

- frontmatter: none; all four keys; quoted values; unknown key → error; bad rule in `allowed-tools` → error.
- names: `review.md` → `review`; `git/commit.md` → `git:commit`; `Bad Name.md` → error; depth 4 → error.
- precedence: built-in beats project beats user; untrusted project files are not loaded.
- expand: `$ARGUMENTS`, `$1`/`$2`, missing `$3` → empty; unknown command → Err; `!` line → Err; plain text → None.
- limits: 201 files → error; a 65 KiB file → error; a symlink out of `.axh/commands` → refused.
- restrict: the two cases of section 3.
- built-ins: `/help` lists a user command with its hint; `/permissions` shows a deny rule; `/clear` answers the
  "not available" text.
- event: `Event::Command` serializes to `{"type":"command","name":…,"source":…,"prompt_chars":…}`.

### 7. Docs (same commit)

- `docs/plans/harness.md`: road-map row HS4 → `**HS4** ✅`; add a section "HS4 — slash commands (decided
  2026-10-13)" with the decisions above in short (C1 files and trust, C2 format, C3 allowed-tools only narrows,
  C4 `!` lines and `@file` deferred, C5 built-ins in code and always first, C6 headless expansion + `command` event),
  and "HS4 — what exists".
- `docs/architecture.md`: one line after the HS3b paragraph in the status log.
- `AGENTS.md`: `HS1–HS3b built, HS4–HS8 open` → `HS1–HS4 built, HS5–HS8 open`.

## Out of scope (do not build)

`!command` lines, reading `@file`, `/clear` `/model` `/compact` behaviour, sessions over several turns, a TUI,
plugins, MCP prompts as commands, skills. Each is a later step.
