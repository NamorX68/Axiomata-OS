/**
 * Small dev-only escape hatches, each gated behind its own Vite env var —
 * not an app setting, no UI toggle, never something a release build's user
 * sees. `VITE_`-prefixed because that's the one prefix Vite actually
 * exposes to `import.meta.env` on the client (anything else is stripped
 * from the bundle, by design — see Vite's own env-var docs). Set one in a
 * local `apps/dashboard/.env.local` (already gitignored via the scaffold's
 * own `*.local` rule, so nothing here is ever accidentally committed) —
 * `cargo tauri dev` picks it up the same way `vite`/`npx vite` alone does,
 * since Tauri's dev mode just runs the same Vite dev server underneath.
 */

/** `VITE_AXIOMATA_DISABLE_AUTO_REFRESH=true` skips every connector
 *  module's (`calendar`/`mail`/`reminders`) mount-time auto-refresh — each
 *  one is a real, billed agent turn (`run_skill`), not a cache read, and
 *  `cargo tauri dev` restarts repeatedly during active development,
 *  re-triggering all three every single time for no testing benefit unless
 *  the module actually under test *is* one of them. Cached results from
 *  the last real run still load and display normally regardless of this
 *  flag (`loadLatest`, unaffected) — only the automatic *new* run is
 *  skipped; each module's own manual ↻ button (`refreshNow`) still works
 *  regardless too, since a deliberate click is exactly the opposite of the
 *  "not this time, I'm testing something unrelated" case this flag exists
 *  for. Owner request, Checkpoint 5d follow-up (`docs/plans/terminal.md`)
 *  — raised while live-testing the Terminal module, unrelated to Terminal
 *  itself, but the same session surfaced it.
 *
 *  Gated on `import.meta.env.DEV` too (architecture review) — a plain
 *  `VITE_*` var only reflects whatever `.env*`/shell env happened to be
 *  active during a given build, with no guarantee it's absent from a real
 *  `tauri build`/`vite build` output (a leftover shell export, a shared CI
 *  env, …). `import.meta.env.DEV` is Vite's own build-mode flag instead —
 *  statically `false` in any production build regardless of env vars — the
 *  same guard every other dev-only branch in this codebase already uses
 *  (`App.svelte`, `core/backend.ts`, `modules/index.ts`'s `dev` module
 *  flag). Without it, a stray env var reaching a shipped build would
 *  silently stop the real, billed digest refresh from ever firing again —
 *  a quiet failure mode, unlike most dev-flag mistakes. */
export function autoRefreshDisabled(): boolean {
  return import.meta.env.DEV && import.meta.env.VITE_AXIOMATA_DISABLE_AUTO_REFRESH === "true";
}

/** Calls `fn` unless `autoRefreshDisabled()` — the one rule every connector
 *  module's mount-time `onMount` needs, pulled out here (architecture
 *  review) instead of each of `calendar`/`mail`/`reminders.svelte`
 *  restating the same inline ternary and its rationale three times. Takes
 *  a thunk, not a value, so the guarded call (`refreshNow`, a real billed
 *  agent turn) is never even invoked when the flag is set — not just its
 *  result discarded. */
export function runUnlessAutoRefreshDisabled(fn: () => unknown): void {
  if (!autoRefreshDisabled()) fn();
}
