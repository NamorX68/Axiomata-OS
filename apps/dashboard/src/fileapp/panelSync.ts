/**
 * The file panel's config following its editor (`docs/plans/editor.md`, ED4,
 * W4, W11): a filed note or a followed HTML link moves the editor to another
 * file, and the panel's config must then name that file — so opening it again
 * brings this panel forward (`core/staging.ts` `samePath`) and the panel never
 * reopens the file it just left.
 *
 * The one invariant, kept here and tested: the patch is `null` exactly when
 * the config already names what the editor shows, which is what stops the
 * config → open → state → config round trip after one step.
 */

import type { OpenFileState } from "./FileEditor.svelte";

/** The workspace is the default root: the openers leave it unnamed, and so does the panel. */
const DEFAULT_ROOT = "workspace";

/** The fields of a file panel's config this sync writes. */
export interface PanelTarget {
  path: string;
  root: string | undefined;
  isNew: false;
}

/** What the config must become to name the editor's file, or `null` if it already does (or there is none). */
export function panelTarget(config: Record<string, unknown>, state: OpenFileState | null): PanelTarget | null {
  if (!state || state.untitled) return null;
  const root = state.root === DEFAULT_ROOT ? undefined : state.root;
  if (config.path === state.rel && config.root === root && config.isNew !== true) return null;
  return { path: state.rel, root, isNew: false };
}
