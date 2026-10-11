/**
 * How much the app moves (`docs/plans/ui-ux-round.md`, G3): one app-wide
 * setting instead of the Orbit's own switch.
 *
 * * **Full** — everything: the glow breathes, sparks fly, the start ignites,
 *   the Orbit and the Second Brain turn by themselves.
 * * **Calm** — the glow still shows but holds still; no sparks, no ignition.
 *   The graph still turns.
 * * **Off** — like Calm, and nothing turns by itself.
 *
 * macOS "Reduce motion" counts as Off, whatever the setting says: the owner of
 * the machine asked for it at the system level. The level in effect is set as
 * `data-motion` on `<html>`, where `themes/tokens.css` switches the breathing
 * animation off for Calm and Off.
 */

import { derived, writable, type Readable, type Writable } from "svelte/store";

export const MOTION_LEVELS = [
  { id: "full", label: "Full" },
  { id: "calm", label: "Calm" },
  { id: "off", label: "Off" },
] as const;

export type MotionLevel = (typeof MOTION_LEVELS)[number]["id"];

/** The setting, persisted in `dashboard.json` by `persist.ts`. */
export const motionSetting: Writable<MotionLevel> = writable("full");

/** Whether the system asks for reduced motion (kept current by {@link startMotion}). */
export const systemReducesMotion: Writable<boolean> = writable(false);

/** Whether a stored value is a valid level. */
export function isMotionLevel(value: unknown): value is MotionLevel {
  return MOTION_LEVELS.some((m) => m.id === value);
}

/** The level in effect: the system's "Reduce motion" wins over the setting. */
export function effectiveMotion(setting: MotionLevel, systemReduces: boolean): MotionLevel {
  return systemReduces ? "off" : setting;
}

/** The level in effect, for components and the renderer. */
export const motion: Readable<MotionLevel> = derived([motionSetting, systemReducesMotion], ([setting, reduces]) =>
  effectiveMotion(setting, reduces),
);

/** Whether the graph may turn by itself at this level. */
export const turns = (level: MotionLevel): boolean => level !== "off";

/** Whether the glow may breathe, sparks fly and the start ignite at this level. */
export const animates = (level: MotionLevel): boolean => level === "full";

/**
 * Follows the system preference and puts the level in effect on `<html>`;
 * returns a function that stops both.
 */
export function startMotion(): () => void {
  const query = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
  const onQuery = (): void => systemReducesMotion.set(query?.matches ?? false);
  onQuery();
  query?.addEventListener("change", onQuery);
  const unsubscribe = motion.subscribe((level) => {
    document.documentElement.dataset.motion = level;
  });
  return () => {
    query?.removeEventListener("change", onQuery);
    unsubscribe();
  };
}
