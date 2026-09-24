/**
 * What the editor surface needs to know to draw (`docs/plans/editor.md`, F12),
 * derived from the owner's `EditorSettings` for one particular file.
 */

import type { LineNumberMode } from "../editor/gutter";
import type { EditorSettings } from "./editorSettings";
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
}

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
  };
}
