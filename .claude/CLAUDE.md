# Project: Axiomata-OS

Language standards load automatically from `~/.claude/rules/` when matching files are read:
`rust.md` (crates, `src-tauri`), `svelte-typescript.md` (`apps/dashboard`), `swift.md` (debug-playground samples).
Project-specific guidance lives in `AGENTS.md` / `CLAUDE.md` at the repo root.

## Project-Specific Notes
- Quality gate: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`, plus
  `cd apps/dashboard && npm run check && npx vitest run` when the frontend changed.
- `cargo audit` lives in `~/.cargo/bin` (not on the non-interactive `PATH`); config in `.cargo/audit.toml`.
