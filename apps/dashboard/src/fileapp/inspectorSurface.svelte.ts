/**
 * The surface settings the inspector's live preview draws with
 * (`docs/plans/editor-look.md`, LK1, I5): the owner's editor settings in the
 * face that has loaded. Shared by the file app and the IDE, which both show the
 * inspector; call it during component setup.
 */

import { fromStore } from "svelte/store";

import { editorFace } from "./editorFace.svelte";
import { editorSettings } from "./editorSettings";
import { surfaceSettings, type SurfaceSettings } from "./surfaceSettings";

export function inspectorSurface(): { readonly current: SurfaceSettings } {
  const settings = fromStore(editorSettings);
  const face = editorFace();
  const value = $derived({
    ...surfaceSettings(settings.current, false),
    fontFamily: face.family,
    fontWeight: face.weight,
  });
  return {
    get current() {
      return value;
    },
  };
}
