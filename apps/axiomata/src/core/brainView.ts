/**
 * How the Orbit's graph moves and is labelled — one setting for the cloud on the Orbit and the Second Brain view alike
 * (`docs/plans/orbit-brain.md`, B6). It lived in two places before: the Orbit tile's own "Slow spin / Labels" and the Second
 * Brain panel's rotation slider and "File names". Shown in the app settings; the Orbit's corner has the quick switch for
 * the motion alone.
 */

import { writable, type Writable } from "svelte/store";

export interface BrainView {
  /** The quick switch: false stops the motion whatever `speed` says. */
  motion: boolean;
  /** Radians per second the cloud turns by; the slider's value. */
  speed: number;
  /** Skill, area and hub captions. */
  labels: boolean;
  /** File names beside the notes of the Second Brain view. */
  fileNames: boolean;
}

export const DEFAULT_BRAIN_VIEW: BrainView = { motion: true, speed: 0.02, labels: true, fileNames: false };

/** The slider's range, shared by the settings and the validation. */
export const SPEED_MAX = 0.12;

export const brainView: Writable<BrainView> = writable({ ...DEFAULT_BRAIN_VIEW });

const isSpeed = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= SPEED_MAX;

/**
 * The stored view setting, read defensively. A file written before the setting existed has the old Second Brain
 * preferences (`spin`: the slider, 0 = off; `fileNames`) instead, and those are taken over.
 */
export function readBrainView(stored: unknown, legacySecondBrain?: unknown): BrainView {
  const own = (stored && typeof stored === "object" ? stored : {}) as Record<string, unknown>;
  const legacy = (legacySecondBrain && typeof legacySecondBrain === "object" ? legacySecondBrain : {}) as Record<
    string,
    unknown
  >;
  const legacySpin = isSpeed(legacy.spin) ? legacy.spin : undefined;
  const speed = isSpeed(own.speed) ? own.speed : legacySpin && legacySpin > 0 ? legacySpin : DEFAULT_BRAIN_VIEW.speed;
  return {
    motion: typeof own.motion === "boolean" ? own.motion : legacySpin !== 0,
    speed,
    labels: typeof own.labels === "boolean" ? own.labels : DEFAULT_BRAIN_VIEW.labels,
    fileNames:
      typeof own.fileNames === "boolean" ? own.fileNames : legacy.fileNames === true || DEFAULT_BRAIN_VIEW.fileNames,
  };
}

/** What the renderer is given as its `spin`. */
export const spinOf = (view: BrainView): number => (view.motion ? view.speed : 0);
