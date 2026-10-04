/**
 * Handing a file from the floating panel to the file app (`docs/plans/editor.md`,
 * ED4, W11): the panel lets go of its live session (unsaved text, undo history)
 * and the file app opens it as a tab. The file app may not be mounted yet — it
 * mounts the first time it is shown — so hand-overs wait in a queue it drains.
 */

import { writable } from "svelte/store";

import { emit } from "../core/bus";
import type { FileSession } from "./session";
import type { ViewMode } from "./fileKinds";
import type { FileRef } from "./tabs";

export interface Handoff {
  /** `null`: a new note, not filed yet. */
  file: FileRef | null;
  /** The panel's live session; `null` for what has none (a picture). */
  handed: { session: FileSession; viewMode: ViewMode } | null;
}

/** Hand-overs the file app has not taken yet. */
export const handoffs = writable<Handoff[]>([]);

/** Opens `handoff` as a tab in the file app, and shows the file app. */
export function handToFileApp(handoff: Handoff): void {
  handoffs.update((list) => [...list, handoff]);
  emit("shell:editor");
}

/** Takes every waiting hand-over, oldest first. */
export function takeHandoffs(): Handoff[] {
  let taken: Handoff[] = [];
  handoffs.update((list) => {
    taken = list;
    return [];
  });
  return taken;
}
