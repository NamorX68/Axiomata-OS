/**
 * What the editor surface needs to know to draw (`docs/plans/editor.md`, F12),
 * derived from the owner's `EditorSettings` for one particular file.
 */

import type { LineNumberMode } from "../editor/gutter";
import type { CursorAnimation, EditorSettings } from "./editorSettings";
import { nearestWeight } from "./fonts";

export interface SurfaceSettings {
  fontFamily: string;
  /** A weight the family really has (F13) — the nearest to the owner's choice. */
  fontWeight: number;
  /** Pixels. */
  fontSize: number;
  /** Multiple of the font size. */
  lineHeight: number;
  ligatures: boolean;
  lineNumbers: LineNumberMode;
  /** Soft wrap for this file (F6: on for prose, off for code; ⌥Z flips it). */
  wrap: boolean;
  tabSize: number;
  /** The eye candy of D8/G7; the surface switches motion off under "reduce motion". */
  effects: SurfaceEffects;
  /** Vi's keys instead of the Mac's (ED3). */
  vi: boolean;
}

export interface SurfaceEffects {
  cursor: CursorAnimation;
  smoothScroll: boolean;
  currentLine: boolean;
  indentGuides: boolean;
  bracketColors: boolean;
  glow: boolean;
}

/** Every effect off — plain text, for read-only side views. */
export const NO_EFFECTS: SurfaceEffects = {
  cursor: "off",
  smoothScroll: false,
  currentLine: false,
  indentGuides: false,
  bracketColors: false,
  glow: false,
};

/** Extensions that count as prose for the wrap default (F6). */
const PROSE = new Set(["md", "markdown", "txt", "text", ""]);

export function isProse(fileName: string): boolean {
  const dot = fileName.lastIndexOf(".");
  return PROSE.has(dot < 0 ? "" : fileName.slice(dot + 1).toLowerCase());
}

/** Whether `fileName` wraps by default under `settings`. */
export function wrapsByDefault(settings: EditorSettings, fileName: string): boolean {
  return isProse(fileName) ? settings.wrapProse : settings.wrapCode;
}

/** The surface settings for one file; `wrap` is the file's current toggle. */
export function surfaceSettings(settings: EditorSettings, wrap: boolean): SurfaceSettings {
  return {
    fontFamily: settings.fontFamily,
    fontWeight: nearestWeight(settings.fontFamily, settings.fontWeight),
    fontSize: settings.fontSize,
    lineHeight: settings.lineHeight,
    ligatures: settings.ligatures,
    lineNumbers: settings.lineNumbers,
    wrap,
    tabSize: settings.tabSize,
    vi: settings.mode === "vi",
    effects: {
      cursor: settings.cursorAnimation,
      smoothScroll: settings.smoothScroll,
      currentLine: settings.currentLine,
      indentGuides: settings.indentGuides,
      bracketColors: settings.bracketColors,
      glow: settings.glow,
    },
  };
}
