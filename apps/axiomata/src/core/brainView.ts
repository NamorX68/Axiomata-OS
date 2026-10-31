/**
 * How the Orbit's graph moves and is labelled — one setting for the cloud on the Orbit and the Second Brain view alike
 * (`docs/plans/orbit-brain.md`, B6). It lived in two places before: the Orbit tile's own "Slow spin / Labels" and the Second
 * Brain panel's rotation slider and "File names". Shown in the app settings. Whether the graph moves at all is the
 * app-wide motion level now (`core/motion.ts`, G3 of `docs/plans/ui-ux-round.md`); the Orbit's corner switch sets that.
 */

import { writable, type Writable } from "svelte/store";

import { turns, type MotionLevel } from "./motion";
import { tSettings } from "../i18n/settings";

export interface BrainView {
  /** Radians per second the cloud turns by; the slider's value. */
  speed: number;
  /** Skill, area and hub captions. */
  labels: boolean;
  /** File names beside the notes of the Second Brain view. */
  fileNames: boolean;
  /** How the hex cells catch the light (`docs/plans/ui-ux-round.md` G7): still, twinkling, or lit like metal plates. */
  shine: Shine;
  /** How the Second Brain view lays the vault out. */
  layout: BrainLayout;
}

/**
 * The Second Brain's layouts (`docs/plans/brain-views.md`): `bundle` puts the notes on one ring, an arc per area, and
 * bundles their links through the middle (it replaced the earlier `rings`, Q1), `map` makes every area a country of hex
 * fields (UX5), `hex` tiles the notes as a honeycomb in the areas' wedges (the layout before the map, brought back as a
 * choice).
 */
export type BrainLayout = "bundle" | "map" | "hex";

export const BRAIN_LAYOUTS: readonly { id: BrainLayout; label: () => string }[] = [
  { id: "bundle", label: () => tSettings("layoutBundle") },
  { id: "map", label: () => tSettings("layoutMap") },
  { id: "hex", label: () => tSettings("layoutHex") },
];

const isLayout = (value: unknown): value is BrainLayout => BRAIN_LAYOUTS.some((x) => x.id === value);

/** A stored layout read as today's: `rings` was replaced by `bundle` (brain-views Q7), so its choosers land there. */
function readLayout(value: unknown): BrainLayout | undefined {
  if (value === "rings") return "bundle";
  return isLayout(value) ? value : undefined;
}

/** The hex cells' shimmer: `calm` holds still, `twinkle` breathes each cell, `metal` lights each by its tilt. */
export type Shine = "calm" | "twinkle" | "metal";

export const SHINES: readonly { id: Shine; label: () => string }[] = [
  { id: "calm", label: () => tSettings("shineCalm") },
  { id: "twinkle", label: () => tSettings("shineTwinkle") },
  { id: "metal", label: () => tSettings("shineMetal") },
];

export const DEFAULT_BRAIN_VIEW: BrainView = {
  speed: 0.02,
  labels: true,
  fileNames: false,
  shine: "metal",
  layout: "map",
};

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

/** The moment the time travel shows (ms), or `null` for today (G17) — one for the Orbit's hive and the Second
 *  Brain's map, so going from one to the other stays at the same moment. */
export const travelAt: Writable<number | null> = writable(null);

export const brainView: Writable<BrainView> = writable({ ...DEFAULT_BRAIN_VIEW });

const isSpeed = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= SPEED_MAX;

/**
 * The stored view setting, read defensively. A file written before the setting existed has the old Second Brain
 * preferences (`spin`: the slider, 0 = off; `fileNames`; `layout`, which the view kept there until it became a setting)
 * instead, and those are taken over. A layout that no longer exists by its stored name is read as its successor.
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
    shine: SHINES.some((x) => x.id === own.shine) ? (own.shine as Shine) : DEFAULT_BRAIN_VIEW.shine,
    layout: readLayout(own.layout) ?? readLayout(legacy.layout) ?? DEFAULT_BRAIN_VIEW.layout,
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
