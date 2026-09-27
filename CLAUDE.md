# CLAUDE.md

Claude Code's entry point for this repository. The project guidance every coding agent shares
— status, commands, the traps — lives in [`AGENTS.md`](AGENTS.md), which Opencode reads
directly (Opencode 2 reads only `AGENTS.md`, `docs/plans/opencode2.md` Q4) and which is
imported here, so there is one source to keep current. Below the import: what applies to
Claude Code alone.

@AGENTS.md

## Sub-agents (use the Rust variants, not the Python-oriented defaults)

The owner's global `~/.claude/CLAUDE.md` defines mandatory automatic sub-agent triggers.
Three of the named agents there (`test-engineer`, `dependency-auditor`,
`performance-analyzer`) are worded for a Python/`uv` stack and **do not apply to this repo**.
Global, Rust-flavored replacements exist at `~/.claude/agents/{rust-test-engineer,
rust-dependency-auditor,rust-performance-analyzer}.md` (usable in any Rust project) — use
those instead, same trigger conditions translated to Rust terms (`cargo test`, `cargo audit`,
`rusqlite`/`tokio`), with a **project-local cadence override** (owner, 2026-09-08; rationale
in `docs/architecture.md` §7): fire them **once per plan checkpoint, and always before a
commit**, not after every single changed `fn`/`struct`/`Cargo.toml` line mid-task. Keep
writing/updating tests inline as code lands regardless — the test-engineer's run is a bundled
second-pass gap check over the accumulated diff, not the first pass. The same override applies
to Claude's own verification loop: batch `cargo build`/`clippy`/`fmt`/`test` per unit of work,
and run the full set only when the work is done, before handing off to a sub-agent, and before
a commit.

**Lean review cadence (owner, 2026-09-27, to save resources; reversible):** reviews run only
before a commit, and small checkpoints may be bundled into one commit. Instead of separate
architecture + test-gap agents, **one** combined review agent (model `sonnet`, narrow brief:
defects and convention breaks only, short report) covers design and missing tests; Claude keeps
writing the tests inline. `security-auditor` still runs in full where a plan asks for it or the
global trigger applies (auth, external paths, new endpoints, …).

`architecture-reviewer`, `security-auditor`, `docs-writer`, and `refactoring-specialist` are
already language-agnostic as globally defined and apply here unchanged. When
`architecture-reviewer` or another background sub-agent run is not available (e.g. a session
rate limit), do a manual review pass yourself rather than skipping the check — the trigger
is mandatory, not the specific tool.
