/**
 * How the Diffs view is laid out, remembered per place (`docs/plans/git-layer.md`,
 * H1): the narrow side tab defaults to one column, the dock pane to two. Kept
 * under `settings.diffs` in `dashboard.json`, like the editor's recent files.
 */

import { getSetting, setSetting } from "../core/persist";
import type { DiffLayout } from "../editor/diff/view";

/** Where a Diffs view sits: an agent's side tab, or its own dock pane (H14). */
export type DiffPlace = "tab" | "dock";

const KEY = "diffs";
const DEFAULTS: Record<DiffPlace, DiffLayout> = { tab: "unified", dock: "split" };

type DiffPrefs = Partial<Record<DiffPlace, DiffLayout>>;

function isLayout(value: unknown): value is DiffLayout {
  return value === "unified" || value === "split";
}

export function diffLayout(place: DiffPlace): DiffLayout {
  const stored = getSetting<DiffPrefs>(KEY)?.[place];
  return isLayout(stored) ? stored : DEFAULTS[place];
}

export function rememberDiffLayout(place: DiffPlace, layout: DiffLayout): void {
  setSetting(KEY, { ...getSetting<DiffPrefs>(KEY), [place]: layout });
}
