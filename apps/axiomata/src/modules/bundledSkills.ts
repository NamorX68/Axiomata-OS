/**
 * The Skills Deck's "Bundled skills" section and its notice (`skills::bundled` in the core): the words and tones of
 * each state, what "update all" takes, and the one shared list both faces of the deck read.
 */

import { writable } from "svelte/store";

import type { BundledSkill, BundledState } from "../core/backend";
import { tCanvas } from "../i18n/canvas";
import type { StatusTone } from "../ui/status";

/** The latest list from the backend — `null` until it was read once. The front face reads it once at start for the
 *  notice; the settings face refreshes it after every update, so the notice goes away with the last update. */
export const bundledSkills = writable<BundledSkill[] | null>(null);

/** Bumped by the notice's button: the settings face scrolls its section into view on each bump. */
export const bundledFocus = writable(0);

/** The state as the section says it. */
export function stateLabel(state: BundledState): string {
  switch (state) {
    case "up_to_date":
      return tCanvas("stateUpToDate");
    case "outdated":
      return tCanvas("stateOutdated");
    case "missing":
      return tCanvas("stateMissing");
    case "changed":
      return tCanvas("stateChanged");
    case "differs":
      return tCanvas("stateDiffers");
  }
}

/** Calm on purpose: an update waits for no one and nothing works there (the accent rule), so only "fine" has a tone
 *  of its own and every other state is neutral — the words say which. */
export function stateTone(state: BundledState): StatusTone {
  return state === "up_to_date" ? "ok" : "neutral";
}

/** Whether updating replaces something the owner may have written, so the section asks first. */
export function isOwners(state: BundledState): boolean {
  return state === "changed" || state === "differs";
}

/** What "Update all outdated" (and the notice's count) takes: only copies that hold nothing of the owner's. */
export function needsUpdate(state: BundledState): boolean {
  return state === "outdated" || state === "missing";
}

/** How many bundled skills can be updated without a question. */
export function updatableCount(list: readonly BundledSkill[]): number {
  return list.filter((s) => needsUpdate(s.state)).length;
}

/** The notice's line, or `null` when there is nothing to update. */
export function noticeText(list: readonly BundledSkill[] | null): string | null {
  const n = list ? updatableCount(list) : 0;
  if (n === 0) return null;
  return tCanvas("bundledNotice", { n });
}
