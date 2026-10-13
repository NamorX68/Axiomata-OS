# Axiomata's own harness (board card #37, M7.4)

Status: HS1 and HS2 built 2026-10-13. Each further step is planned on its own and confirmed by the owner before it is built.

## Why

Every agent of Axiomata runs on a foreign harness today: Opencode (skills, routines, the Studio) and Claude Code
(the Studio). An own, lean harness should run skills with a small system prompt and little overhead, run Studio
agents that need no TUI, and be built so it can later be lifted out into an app of its own (working name "Glut" —
note the clash with GLUT/freeglut; check before publishing).

## Decisions (owner, 2026-10-13)

- **H1 — name.** Crate `axiomata-harness`, binary `axh`. The app's name comes when it is lifted out. `Harness::Mini`
  in the engine catalog is renamed when the harness reaches the Studio.
- **H2 — on its own.** The crate does not depend on `axiomata-core` (`cargo tree -p axiomata-harness | grep
  axiomata-core` is empty). It may use `axiomata-files`, later `axiomata-roster`. Configuration, keys, cost metering
  and permission asks come from the caller as values and traits.
- **H3 — form.** A library (the loop) plus the binary `axh`, which runs headless and writes events as JSON lines.
- **H4 — models.** First a client for the OpenAI-compatible `POST {base}/v1/chat/completions` with `tools`
  (OpenRouter, Ollama). Anthropic's own API follows as a second client behind the same trait. No streaming yet.
- **H5 — keys.** From `~/.axiomata/config.toml` (`providers[..].api_key` / `base_url`); the core side hands them in,
  `axh` reads its key from `AXH_API_KEY`, never from an argument. Never in a log (`Debug` redacted), event or prompt.
- **H6 — a small system prompt.** A few fixed lines (role, folder, date, "use the tools, answer briefly") plus the
  caller's instructions; tools describe themselves through the API's `tools` field.
- **H7 — permissions** (step HS3). Rules allow / deny / ask per tool, with patterns in the role-file style
  (`Bash(git status:*)`); "ask" goes through a trait to the caller — in Axiomata a permission request through
  `permission_asks` (#88), headless without an answer refused.

## Road map

Backend, one checkpoint and commit each:

| Step | What |
|---|---|
| **HS1** ✅ | Crate, OpenAI-compatible client, loop, events, file tools, `axh`, `axiomata-cli harness run` |
| **HS2** ✅ | Shell: `bash` (foreground and background), `bash_output`, `kill_shell` — S1–S6 below |
| HS3 | Permissions (H7): rules, the ask trait, `permission_asks` |
| HS4 | Slash commands: registry, Markdown command files, built-ins (`/help`, `/model`, `/clear`) |
| HS5 | MCP client (stdio, JSON-RPC lines; template: the removed client of `54173bf`), tools as `mcp__server__tool` |
| HS6 | Skills: SKILL.md parser in the harness, a skill as a run; then `backend: axh` in the core |
| HS7 | Hooks: SessionStart / PreToolUse / PostToolUse / Stop as shell hooks with JSON on stdin (Claude-compatible) |
| HS8 | Subagents: a `task` tool starting a nested loop with its own prompt, tools and limits |

After the backend: sessions over several turns, streaming, the Anthropic client, cost metering (`studio_spend` /
`spend.rs`), `axh` as an engine in the Studio, a settings page — and the front ends: a **TUI** and an **app** of its
own, both on the event stream of H3.

## HS2 — the shell (decided 2026-10-13)

- **S1 — off unless asked for.** Until the permissions of HS3 exist, the shell tools are offered only when the caller
  says so (`axh --allow-bash`, `axiomata-cli harness run --allow-bash`); the shell reaches whatever the owner's user
  reaches. With HS3, rules take the switch's place.
- **S2 — the owner's shell, with a profile snapshot, like Claude Code.** `$SHELL` (zsh on the Mac; `/bin/sh` when unset
  or unknown). Once per run, an interactive login shell writes a snapshot of the profile — functions, aliases, `PATH` —
  and every command sources it, so a command finds what the owner's terminal finds without `~/.zshrc` running each
  time. The profile is the owner's own code; it runs once per run. If the snapshot fails, commands run without it.
- **S3 — a fresh process per command, the folder carried over.** Each call is a new shell; after it the harness reads
  where it ended (`pwd -P`) and the next call starts there — but only inside the run's folder: a `cd` outside is set
  back, and the model is told so. Variables do not carry over.
- **S4 — limits.** 2 minutes per command by default, at most 10; stdin is empty (nothing waits for input); stdout and
  stderr together, cut like every tool result; the exit code always comes back, and a non-zero one marks the call as
  failed. On a timeout, a stop or the end of the run the whole process group is killed, children included.
- **S5 — environment.** The cleaned tool environment of `axiomata-files` (`toolenv`, the one the language servers get)
  plus the snapshot's `PATH` — no stray variables, so not the harness's own key either.
- **S6 — background jobs.** `bash` with `run_in_background` starts a job and returns its id at once; `bash_output`
  reads what it wrote since the last look and whether it still runs; `kill_shell` ends it. At most 8 jobs per run,
  the last 1 MiB of each job's output kept; every job ends with the run.

## HS2 — what exists

- `tools::shell` (`shell_tools(cwd, ShellOptions)`): `bash`, `bash_output`, `kill_shell` over one `Session` per run
  (snapshot, folder, environment, jobs); the jobs are killed when the run's toolbox is dropped.
- `axh run … --allow-bash` and `axiomata-cli harness run … --allow-bash` offer them (S1).
- Reviews (2026-10-13), built in: a process group is forgotten once its leader is reaped (no stale id ever hits
  another group), and a dropped call (timeout or stop of the run) kills its group; the snapshot runs as a login shell
  that reads `~/.zshrc` itself (an interactive `-i` shell in its own group would fight the terminal and stop); a
  `$SHELL` other than zsh or bash runs as `/bin/sh`; `builtin cd/eval/pwd` so a profile function cannot take over the
  wrapper; a deleted folder sends the next command to the run's folder; job output never breaks a UTF-8 character;
  at most 32 jobs are kept; Ctrl-C, SIGTERM and SIGHUP all end the run cleanly, its jobs included.
- **Known limits, until HS3's sandbox:** a child that leaves its process group on purpose (`setsid`, a daemon,
  `launchctl`) survives the kill; a command can read what the owner's user can read, including the harness's own
  start environment through `ps` (so the key) and the credential files — the clean environment (S5) keeps stray
  variables out of commands, it does not hide them.
- Tests: output and exit code, the folder carried over and set back, the cleaned environment (no key), a timeout
  killing a backgrounded grandchild, jobs read, stopped and ended with the run, and the zsh profile snapshot
  (alias, function, `PATH`) against a scratch `HOME`.

## HS1 — what exists

- `crates/axiomata-harness`: `config` (`ModelEndpoint`, `RunConfig`), `model` (`ChatModel`, `OpenAiCompat`),
  `tools` (`Tool`, `Toolbox`, the results cut to 30 000 characters) and `tools::files` (`read_file`, `write_file`,
  `edit_file`, `list_dir`, `grep`, `glob`, all through `axiomata_files::Root` with `LinkPolicy::Contained`), `prompt`,
  `event` (`Event`, `EventSink`), `run` (the loop, `Cancel`).
- `axh run --base-url … --model … [--cwd …] [--instructions-file …] [--max-turns N] [--timeout-secs S] "prompt"`.
- `axiomata-cli harness run [--provider open-router|ollama] [--model …] [--cwd …] [--json] "prompt"` — owner only.
- Limits and guards (security audit 2026-10-13): no write into `.git`, `.hg`, `.jj`, `.axiomata`, `.claude`,
  `.opencode`, `.vscode`, `.mcp.json`, `opencode.json` (a line in `.git/config` runs at the owner's next `git status`;
  the others are configuration a later session obeys) and no read or listing of the version-control stores; at most
  16 tool calls per turn; answers read up to 16 MiB; no redirects; a key only over https or to this machine, and
  redacted from echoed errors; a stopped run stops a running search and starts no tool (a write already on a
  blocking thread still finishes). Build files (`Makefile`, `package.json`, `build.rs`) stay writable — the owner
  reviews the diff, as with any coding agent.
- Tests: the client against a fake API on the loopback, the loop with a scripted model, the file tools in a temp
  folder (escapes refused), and a live test (`tests/live.rs`, ignored) against a real API.
