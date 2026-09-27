/**
 * The font face editor surfaces draw with (`docs/plans/editor.md`, F13): the
 * owner's family and nearest real weight, switched to only once the face has
 * loaded — so a surface never measures its cell width against a fallback font.
 * Shared by the file app and the diff view; call it during component setup.
 */

import { fromStore } from "svelte/store";

import { ensureInstalledFonts, installedFonts } from "../core/installedFonts";
import { editorSettings } from "./editorSettings";
import { drawnFamily, loadFace, nearestWeight } from "./fonts";

export interface EditorFace {
  readonly family: string;
  readonly weight: number;
}

export function editorFace(): EditorFace {
  const settings = fromStore(editorSettings);
  const installed = fromStore(installedFonts);
  void ensureInstalledFonts();
  let face = $state({ family: settings.current.fontFamily, weight: 400 });
  $effect(() => {
    // An installed family that is gone (uninstalled) quietly draws the default (T10).
    const family = drawnFamily(settings.current.fontFamily, installed.current.loaded);
    const weight = nearestWeight(family, settings.current.fontWeight);
    let current = true;
    void loadFace(family, weight)
      .catch(() => undefined)
      .then(() => {
        if (current) face = { family, weight };
      });
    return () => {
      current = false;
    };
  });
  return {
    get family() {
      return face.family;
    },
    get weight() {
      return face.weight;
    },
  };
}
