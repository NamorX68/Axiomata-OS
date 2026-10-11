/**
 * The time-of-day light (`docs/plans/ui-ux-round.md` UX3, G10 "very subtle"): where a faint light falls across the
 * ground and how warm it is. The sun rises on the left, stands high at noon and sets on the right; mornings and
 * evenings are warm, noon is a neutral brightness, the night has only a cool trace. `shell/Ambient.svelte` draws it.
 */

/** Where the light comes from (percent of the window) and how strong and warm it is, both 0…1. */
export interface DayLight {
  x: number;
  y: number;
  strength: number;
  warmth: number;
  /** Between sunset and sunrise: the cool trace instead of the sun. */
  night: boolean;
}

const SUNRISE = 6;
const SUNSET = 21;

/** The light at a local hour (fractional: 13.5 is half past one). */
export function dayLightAt(hour: number): DayLight {
  const h = ((hour % 24) + 24) % 24;
  if (h < SUNRISE || h >= SUNSET) {
    // Night: a faint cool light from high up in the middle.
    return { x: 50, y: -20, strength: 0.25, warmth: 0, night: true };
  }
  const t = (h - SUNRISE) / (SUNSET - SUNRISE);
  const height = Math.sin(t * Math.PI);
  return {
    x: 5 + 90 * t,
    y: 40 - 60 * height,
    strength: 0.5 + 0.5 * height,
    // Warm low in the sky, neutral at its height.
    warmth: 1 - height,
    night: false,
  };
}
