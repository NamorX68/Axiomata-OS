/**
 * The app's ambient effects (`docs/plans/ui-ux-round.md` G10), each one switchable under Appearance: light under the
 * pointer on the hex pattern, a very faint light that follows the time of day, the ignition at a cold start — all on
 * by default — and the sounds (UX7d, `core/sounds.ts`), off by default.
 */

import { writable, type Writable } from "svelte/store";

export interface Effects {
  /** A soft light on the hex pattern where the pointer is. */
  pointerLight: boolean;
  /** A faint light across the ground that follows the time of day. */
  dayLight: boolean;
  /** The hex logo drawing itself and igniting when the app starts (only at motion Full). */
  ignition: boolean;
  /** Soft cues when an agent finishes or waits, or a routine fires, while the app lies in the background. */
  sounds: boolean;
}

export const DEFAULT_EFFECTS: Effects = { pointerLight: true, dayLight: true, ignition: true, sounds: false };

/** The setting, persisted in `dashboard.json` by `persist.ts`. */
export const effects: Writable<Effects> = writable({ ...DEFAULT_EFFECTS });

/** A stored value read back: every field that is no boolean takes its default, unknown fields are dropped. */
export function readEffects(value: unknown): Effects {
  const r = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const bool = (v: unknown, d: boolean): boolean => (typeof v === "boolean" ? v : d);
  return {
    pointerLight: bool(r.pointerLight, DEFAULT_EFFECTS.pointerLight),
    dayLight: bool(r.dayLight, DEFAULT_EFFECTS.dayLight),
    ignition: bool(r.ignition, DEFAULT_EFFECTS.ignition),
    sounds: bool(r.sounds, DEFAULT_EFFECTS.sounds),
  };
}
