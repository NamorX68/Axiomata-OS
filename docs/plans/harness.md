# Axiomata's own harness (board card #37, M7.4)

Status: HS1, HS2 and HS3a built 2026-10-13. Each further step is planned on its own and confirmed by the owner before it is built.

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
| **HS3a** ✅ | Permissions: rules, modes, settings files, asking (P1–P8 below) |
| HS3b | Sandbox for `bash` (macOS Seatbelt first) — planned on its own |
| HS4 | Slash commands: registry, Markdown command files, built-ins (`/help`, `/model`, `/clear`) |
| HS5 | MCP client (stdio, JSON-RPC lines; template: the removed client of `54173bf`), tools as `mcp__server__tool` |
| HS6 | Skills: SKILL.md parser in the harness, a skill as a run; then `backend: axh` in the core |
| HS7 | Hooks: SessionStart / PreToolUse / PostToolUse / Stop as shell hooks with JSON on stdin (Claude-compatible) |
| HS8 | Subagents: a `task` tool starting a nested loop with its own prompt, tools and limits |

After the backend: sessions over several turns, streaming, the Anthropic client, cost metering (`studio_spend` /
`spend.rs`), `axh` as an engine in the Studio, a settings page — and the front ends: a **TUI** and an **app** of its
own, both on the event stream of H3.

## HS3a — permissions (decided 2026-10-13)

- **P1 — rules in Claude Code's format.** `Tool` or `Tool(pattern)` in three lists, `allow`, `ask`, `deny`; across
  every source `deny` beats `ask` beats `allow`. `Bash(cargo test:*)` is a command prefix (`:*` = followed by
  arguments or nothing), `*` matches anywhere (`Bash(git * push *)`); `Read(src/**)` / `Edit(**/*.rs)` are globs
  relative to the working folder. `Read` covers `read_file`, `list_dir`, `grep`, `glob`; `Edit` (and `Write`) covers
  `write_file` and `edit_file`; `Bash` covers `bash`. A role's existing `permissions` (`AGENT.md`) are such rules.
- **P2 — reading a command (reworked after two security reviews).** A small lexer reads the command as the shell
  would — quotes, escapes, `&&`/`||`/`;`/`|`/`&`/line breaks, redirections, `2>&1`. Whatever it cannot vouch for makes
  the command *opaque*: a comment, a here-document, any `$` expansion (`$X`, `$(…)`, `${…}`), backticks,
  `<(…)`/`=(…)`, `$'…'`, grouping and braces, `!`, keywords (`if`, `for`, `repeat`, …), a command word that is not
  literal (`=rm`, a glob, `~/…`), builtins that run or define other commands (`eval`, `source`/`.`, `trap`, `alias`,
  `export`, …), a shell running a script or its input, `sh -c '…'`, git's own configuration (`git config`, `-c core.*`),
  a carriage return, a redirection without a command. An opaque command is never allowed by a rule nor by `auto` — it
  is asked (refused where the mode asks nothing). Deny and ask rules are held against every part as words — with
  assignments in front (`X=`, `X+=`, `a[0]=`) dropped, every word sequence after a wrapper (`env`, `sudo`, `command`,
  `xargs`, `noglob`, `nocorrect`, `-`, `nohup`, `time`, `nice`, `timeout`, `exec`, `stdbuf`, …), the command's folder
  (`/bin/rm`), git's global options (`git -C . push`), inside `sh -c` strings; the command word without case on macOS
  (`RM` is `rm`). What a command writes (`>`, `>>`, `&>`) — or reads (`<`) — is held against the protected names, also
  through a glob (`.ax?`), refused; a write outside the folder is asked; a write inside it lands where the shell is
  (after an earlier `cd`) and is held against the `Edit` rules; a `cd` together with a write is asked. A part with
  `NAME=value` in front is never allowed by a rule. Best effort until HS3b: `find -delete`, a script, a program's own
  config still do more than their words say; `Read` rules bind the file tools, not what a command reads (as in Claude
  Code).
- **P3 — modes, like Claude Code.** `default`: reads free, edits and commands asked; `accept_edits`: edits free too,
  commands asked; `plan`: reads only, edits and commands refused (an allow rule still lets a command through);
  `dont_ask`: whatever no rule allows is refused, nothing is asked — an `ask` rule refuses too; `auto`: whatever no
  rule denies is allowed (the owner's auto mode of #89), except an opaque command or one naming a protected path,
  which is asked. `plan` never writes, whatever a rule says, and never asks. No bypass mode. `bash_output` and `kill_shell` touch only the run's own jobs: always
  allowed.
- **P4 — settings files, the harness's own.** `~/.axh/settings.json` (the owner's), `<project>/.axh/settings.json`
  (checked in) and `<project>/.axh/settings.local.json` (not checked in), each
  `{"permissions": {"allow": [], "ask": [], "deny": [], "defaultMode": "default"}}`. A file that does not parse — an
  unknown field, a misspelled rule name (`bash(…)`), more than 1 000 rules — stops the run: a deny list silently
  ignored would be worse than no run. The mode comes from the flag, else local, project,
  user, else `default`.
- **P5 — trusting a project, like Claude Code.** A project may always tighten: its `deny` and `ask` rules count in
  every folder. What loosens — its `allow` rules and its mode — counts only in a folder the owner trusted once
  (`~/.axh/trusted.json`), and its mode never past the owner's own (`plan` < `dont_ask` < `default` < `accept_edits` <
  `auto`). The CLI shows what trusting would add, then asks; without a terminal it is left out, and the run says so.
  Agents never write `.axh/` — neither with the file tools (any case, also through a link) nor with a redirection.
- **P6 — who answers an ask.** The CLI asks in the terminal (`y` / the word `always` for this run / no; the command
  shown as plain text, at most 300 characters plus a count, invisible Unicode removed); headless (`axh`, no
  terminal) refuses. The app answers through `permission_asks` (#88) once the harness runs in the Studio, with an
  agent's identity — the trait is there for it. "Always" adds the rule for the rest of the run only.
- **P7 — the flags.** `--permission-mode`, `--allow`, `--ask`, `--deny` (repeatable) on `axh run` and
  `axiomata-cli harness run`; `--allow-bash` goes — the shell is always offered and its calls are judged.
- **P8 — visible.** Every judged call is an event (`permission`: tool, what, allowed / asked / refused, and why: which
  rule, the mode, or the answer), so a log, the TUI or the Studio can show why something did or did not run.

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

## HS3a — what exists

- `permissions` (`Rule`, `RuleSet`, `Mode`, `Gate`, `Asker` with `Terminal` and `Nobody`, `build_gate`, `printable`),
  `permissions::command` (the lexer: `analyze`, opaque reasons, deny candidates, writes, protected names) and
  `permissions::settings` (the `.axh` files, `Trust`). Every tool says what a call touches (`Tool::access`); the
  toolbox's gate judges it before the call, a search leaves out what the read rules hold back, and the loop reports
  each judgement as a `permission` event.
- `axh run` and `axiomata-cli harness run`: `--permission-mode`, `--allow`, `--ask`, `--deny`; the CLI also `--role`
  (a role's `permissions` as allow rules), asks in the terminal and asks to trust a project with `.axh/` settings.
  `--allow-bash` is gone. The app's asker (`permission_asks`) follows with the Studio integration.
- Reviews (code and two security passes, 2026-10-13) found the first, naive splitter could be talked past (a `'` in a comment, a
  here-document, `$'…'`, `FOO=1 rm`, `/bin/rm`, `echo x > .axh/settings.json`), `grep`/`glob` read past `Read` denies,
  and protected names failed on case or through a link; the second pass found `eval`/`source`/`alias`, `X+=1 rm`,
  zsh precommands (`noglob`, `-`), non-literal command words (`=rm`, `$X`, `/bin/r?`), protected names behind globs,
  `RM` on macOS, `git -C . push`, writes after an earlier `cd`, links and hard links for path rules — all rebuilt as
  above, each with tests. Still open by the owner's choice: trust is per folder, not per content (P5).

## HS2 — what exists

- `tools::shell` (`shell_tools(cwd, ShellOptions)`): `bash`, `bash_output`, `kill_shell` over one `Session` per run
  (snapshot, folder, environment, jobs); the jobs are killed when the run's toolbox is dropped.
- Offered on every run since HS3a; each call is judged by the run's permissions (S1 superseded).
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
