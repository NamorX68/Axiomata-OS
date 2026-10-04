/**
 * Global Terminal preferences (Checkpoint 5d of docs/plans/terminal.md):
 * one shared settings blob for every Terminal tile, persisted in its own
 * `~/.axiomata/terminal-settings.json` (via `get_terminal_settings`/
 * `save_terminal_settings`) rather than each tile's own per-instance
 * `dashboard.json` config (`ctx.config`, Checkpoints 5/5b's original
 * design). Owner feedback that motivated the switch: closing/removing a
 * Terminal tile discarded every setting on it, because per-instance config
 * lives and dies with the `CanvasInstance` it's attached to — nothing here
 * is attached to a tile, so there's nothing to lose. `terminal.svelte`/
 * `terminal-settings.svelte` both read/write this store directly instead
 * of `ctx.config` for every Terminal-specific field (font, theme, cursor,
 * shell, cwd, env, scrollback, opacity, bell) — `ctx.config` itself is
 * simply unused by the Terminal module from this checkpoint on.
 *
 * Mirrors `core/persist.ts`'s own load-once/debounced-save shape, just
 * scoped to this one file instead of the whole dashboard: `terminalSettings`
 * is the live store, `ensureTerminalSettingsLoaded()` fetches it the first
 * time any Terminal instance needs it (not eagerly at app boot — a
 * dashboard with no Terminal tile placed has no reason to), and every
 * write after that load debounce-saves back to disk.
 */

import { get, writable, type Writable } from "svelte/store";

import { invokeBackend as invoke, type LoadedTerminalSettings } from "../core/backend";
import { toast } from "../core/toast";

/** Current on-disk schema version — mirrors
 *  `axiomata_core::terminal_settings::SETTINGS_VERSION`. Stripped out of
 *  the live store itself (nothing here ever reads a `version` key) and
 *  re-added only when building the JSON to save. */
const SETTINGS_VERSION = 1;
const SAVE_DEBOUNCE_MS = 400;

export const terminalSettings: Writable<Record<string, unknown>> = writable({});

let loaded = false;
let loadingPromise: Promise<void> | null = null;
/** `true` only while `ensureTerminalSettingsLoaded` is writing the
 *  just-fetched value into the store — same purpose as `core/persist.ts`'s
 *  own `loading` flag: without it, that one write would itself trigger the
 *  subscribe-below's save, debounce-racing the load with a redundant (or,
 *  worse, momentarily-empty-object) write back to the same file. */
let loading = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** Loads `terminal-settings.json` into the store, the first time any
 *  Terminal instance needs it — idempotent (a second call while the first
 *  is still in flight awaits the same promise instead of double-fetching)
 *  and a no-op once already loaded. Both `terminal.svelte` and
 *  `terminal-settings.svelte` call this from their own `onMount`; since
 *  Checkpoint 5c's flip-card design keeps both faces mounted simultaneously
 *  (`canvas/Tile.svelte`), either can genuinely be first. */
export async function ensureTerminalSettingsLoaded(): Promise<void> {
  if (loaded) return;
  if (!loadingPromise) {
    loadingPromise = invoke<LoadedTerminalSettings>("get_terminal_settings")
      .then((res) => {
        loading = true;
        try {
          const parsed = JSON.parse(res.json) as Record<string, unknown>;
          const { version: _version, ...rest } = parsed;
          terminalSettings.set(rest);
        } catch {
          // A corrupt file is already handled Rust-side (moved to `.bak`,
          // the default `{"version":1}` returned instead) — this only
          // guards against `res.json` somehow still not being valid JSON,
          // so the store falls back to empty rather than throwing.
          terminalSettings.set({});
        }
        loading = false;
        loaded = true;
      })
      .catch((err) => {
        toast(`Could not read terminal-settings.json: ${String(err)}`, "danger");
        // Marked loaded anyway (not retried) — the store just stays at its
        // empty default, same "degrade, don't wedge the UI" convention
        // `core/persist.ts`'s own `initPersistence` failure path uses.
        loaded = true;
      });
  }
  return loadingPromise;
}

function scheduleSave(): void {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void flush(), SAVE_DEBOUNCE_MS);
}

async function flush(): Promise<void> {
  const json = JSON.stringify({ version: SETTINGS_VERSION, ...get(terminalSettings) }, null, 2) + "\n";
  try {
    await invoke("save_terminal_settings", { json });
  } catch (err) {
    toast(`Could not save terminal-settings.json: ${String(err)}`, "danger");
  }
}

// Module-scope subscription (not inside a component) so a save is
// scheduled regardless of which component's edit changed the store, same
// "one subscription owns persistence" shape `core/persist.ts` uses for its
// own stores. Svelte fires a fresh subscriber once, synchronously, with
// the store's *current* value — `firstFire` skips that call (there's
// nothing to save yet, the real load hasn't necessarily even started).
let firstFire = true;
terminalSettings.subscribe(() => {
  if (firstFire) {
    firstFire = false;
    return;
  }
  if (loading) return;
  scheduleSave();
});
