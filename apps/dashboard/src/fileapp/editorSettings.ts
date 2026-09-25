/**
 * The editor's preferences (`docs/plans/editor.md`, D6, F12), kept in
 * `~/.axiomata/editor-settings.json` through `get_editor_settings` /
 * `save_editor_settings` — the same load-once, debounce-save shape as the
 * terminal's settings (`modules/terminalSettings.ts`).
 *
 * The file is the frontend's: `parseEditorSettings` reads whatever is there,
 * keeps every field it understands and falls back to F12's defaults for the
 * rest, so a hand-edit or an older file never breaks the editor.
 */

import { writable, type Writable } from "svelte/store";

import { invokeBackend, type LoadedJsonState } from "../core/backend";
import type { LineNumberMode } from "../editor/gutter";
import { toast } from "../core/toast";
import { fontFamily } from "./fonts";

export type EditorMode = "normal" | "vi";
export type Autosave = "off" | "delay" | "leave";
export type CursorAnimation = "off" | "glide" | "trail";

export interface EditorSettings {
  /** Mac-style keys, or Vi (ED3). */
  mode: EditorMode;
  /** Vi's unnamed register is the Mac clipboard, or kept apart from it (D17, V4). */
  viClipboard: "shared" | "separate";
  fontFamily: string;
  /** The owner's choice, 100–900; drawn with the nearest real face (F13). */
  fontWeight: number;
  fontSize: number;
  lineHeight: number;
  ligatures: boolean;
  lineNumbers: LineNumberMode;
  /** Soft wrap for prose files (`.md`, `.txt`) and for everything else (F6). */
  wrapProse: boolean;
  wrapCode: boolean;
  /** Indentation for a file that shows none (F11). */
  indentKind: "spaces" | "tabs";
  indentSize: number;
  /** Width a tab is drawn with. */
  tabSize: number;
  /** F9: off, 1 s after the last change, or when the view is left. */
  autosave: Autosave;
  /** G7: the cursor glides to its new place, with or without a trail. */
  cursorAnimation: CursorAnimation;
  /** G7: jumps (page, ⌘↓, a far click) scroll smoothly. */
  smoothScroll: boolean;
  /** G7: the cursor's line is tinted. */
  currentLine: boolean;
  /** G7: guides at every indentation level. */
  indentGuides: boolean;
  /** G7: bracket pairs coloured by depth. */
  bracketColors: boolean;
  /** G7: a faint accent glow on the cursor and its line number. */
  glow: boolean;
}

/** F12's defaults, confirmed by the owner. */
export const DEFAULT_EDITOR_SETTINGS: EditorSettings = {
  mode: "normal",
  viClipboard: "shared",
  fontFamily: "JetBrains Mono",
  fontWeight: 400,
  fontSize: 14,
  lineHeight: 1.5,
  ligatures: true,
  lineNumbers: "hybrid",
  wrapProse: true,
  wrapCode: false,
  indentKind: "spaces",
  indentSize: 4,
  tabSize: 4,
  autosave: "off",
  cursorAnimation: "trail",
  smoothScroll: true,
  currentLine: true,
  indentGuides: true,
  bracketColors: true,
  glow: true,
};

const SETTINGS_VERSION = 1;
const SAVE_DEBOUNCE_MS = 400;

function pick<T>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function number(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/** The settings in `raw` (a parsed file), every missing or bad field defaulted. */
export function parseEditorSettings(raw: unknown): EditorSettings {
  const r = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_EDITOR_SETTINGS;
  return {
    mode: pick(r.mode, ["normal", "vi"] as const, d.mode),
    viClipboard: pick(r.viClipboard, ["shared", "separate"] as const, d.viClipboard),
    fontFamily: typeof r.fontFamily === "string" && fontFamily(r.fontFamily) ? r.fontFamily : d.fontFamily,
    fontWeight: Math.round(number(r.fontWeight, 100, 900, d.fontWeight) / 100) * 100,
    fontSize: Math.round(number(r.fontSize, 9, 32, d.fontSize)),
    lineHeight: number(r.lineHeight, 1, 2.5, d.lineHeight),
    ligatures: bool(r.ligatures, d.ligatures),
    lineNumbers: pick(r.lineNumbers, ["absolute", "relative", "hybrid"] as const, d.lineNumbers),
    wrapProse: bool(r.wrapProse, d.wrapProse),
    wrapCode: bool(r.wrapCode, d.wrapCode),
    indentKind: pick(r.indentKind, ["spaces", "tabs"] as const, d.indentKind),
    indentSize: Math.round(number(r.indentSize, 1, 8, d.indentSize)),
    tabSize: Math.round(number(r.tabSize, 1, 8, d.tabSize)),
    autosave: pick(r.autosave, ["off", "delay", "leave"] as const, d.autosave),
    cursorAnimation: pick(r.cursorAnimation, ["off", "glide", "trail"] as const, d.cursorAnimation),
    smoothScroll: bool(r.smoothScroll, d.smoothScroll),
    currentLine: bool(r.currentLine, d.currentLine),
    indentGuides: bool(r.indentGuides, d.indentGuides),
    bracketColors: bool(r.bracketColors, d.bracketColors),
    glow: bool(r.glow, d.glow),
  };
}

export const editorSettings: Writable<EditorSettings> = writable({ ...DEFAULT_EDITOR_SETTINGS });

let loaded: Promise<void> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** Loads the file once; later calls wait for the same load. */
export function ensureEditorSettingsLoaded(): Promise<void> {
  loaded ??= invokeBackend<LoadedJsonState>("get_editor_settings")
    .then((res) => {
      let parsed: unknown = {};
      try {
        parsed = JSON.parse(res.json);
      } catch {
        // Rust already moved a corrupt file aside; defaults it is.
      }
      editorSettings.set(parseEditorSettings(parsed));
    })
    .catch((err) => toast(`Could not read editor-settings.json: ${String(err)}`, "danger"));
  return loaded;
}

/** Changes settings and saves them (debounced). */
export function updateEditorSettings(patch: Partial<EditorSettings>): void {
  editorSettings.update((current) => {
    const next = parseEditorSettings({ ...current, ...patch });
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      invokeBackend("save_editor_settings", { json: JSON.stringify({ version: SETTINGS_VERSION, ...next }) }).catch(
        (err) => toast(`Could not save editor-settings.json: ${String(err)}`, "danger"),
      );
    }, SAVE_DEBOUNCE_MS);
    return next;
  });
}
