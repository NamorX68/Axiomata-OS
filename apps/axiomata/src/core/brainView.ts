/**
 * How the Orbit's graph moves and is labelled — one setting for the cloud on the Orbit and the Second Brain view alike
 * (`docs/plans/orbit-brain.md`, B6). It lived in two places before: the Orbit tile's own "Slow spin / Labels" and the Second
 * Brain panel's rotation slider and "File names". Shown in the app settings. Whether the graph moves at all is the
 * app-wide motion level now (`core/motion.ts`, G3 of `docs/plans/ui-ux-round.md`); the Orbit's corner switch sets that.
 */

import { writable, type Writable } from "svelte/store";

import { turns, type MotionLevel } from "./motion";

export interface BrainView {
  /** Radians per second the cloud turns by; the slider's value. */
  speed: number;
  /** Skill, area and hub captions. */
  labels: boolean;
  /** File names beside the notes of the Second Brain view. */
  fileNames: boolean;
}

export const DEFAULT_BRAIN_VIEW: BrainView = { speed: 0.02, labels: true, fileNames: false };

/**
 * The radius of the disc the Second Brain graph is drawn in, as a share of the shorter side of the Orbit's own box (the one the cloud is sized from). The cloud it replaces has
 * 0.36 and the App Ring lies at 0.425 (`graph/layout.ts`'s `APP_RING` × the cloud's radius): in between, so that no tile is
 * covered that the cloud did not cover and the ring stays free.
 */
export const BRAIN_DISC = 0.4;

/** The slider's range, shared by the settings and the validation. */
export const SPEED_MAX = 0.12;

/** Whether the Second Brain layer is up over the Orbit (set by the shell). */
export const brainActive: Writable<boolean> = writable(false);

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
    speed,
    labels: typeof own.labels === "boolean" ? own.labels : DEFAULT_BRAIN_VIEW.labels,
    fileNames:
      typeof own.fileNames === "boolean" ? own.fileNames : legacy.fileNames === true || DEFAULT_BRAIN_VIEW.fileNames,
  };
}

/**
 * The motion level an old stored view stands for, or `null` when it says nothing. Before the app-wide level existed the
 * view had its own `motion` switch (and before that a `spin` of 0): a switched-off graph becomes "off", once, when the
 * level has not been stored yet.
 */
export function legacyMotionLevel(stored: unknown, legacySecondBrain?: unknown): MotionLevel | null {
  const own = (stored && typeof stored === "object" ? stored : {}) as Record<string, unknown>;
  const legacy = (legacySecondBrain && typeof legacySecondBrain === "object" ? legacySecondBrain : {}) as Record<
    string,
    unknown
  >;
  if (own.motion === false) return "off";
  if (own.motion === undefined && legacy.spin === 0) return "off";
  return null;
}

/** What the renderer is given as its `spin`. */
export const spinOf = (view: BrainView, level: MotionLevel): number => (turns(level) ? view.speed : 0);
