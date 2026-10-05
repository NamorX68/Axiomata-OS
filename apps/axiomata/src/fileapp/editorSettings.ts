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
import { usableFamilyName } from "../core/installedFonts";
import { FILE_ICON_STYLES, type FileIconStyle } from "../core/fileIcons";

export type EditorMode = "normal" | "vi";
export type Autosave = "off" | "delay" | "leave";
/** The cursor's glide (editor-look K8): off, subtle (short, no smear) or strong (longer the farther, smeared). */
export type CursorAnimation = "off" | "subtle" | "strong";

export interface EditorSettings {
  /** Mac-style keys, or Vi (ED3). */
  mode: EditorMode;
  /** Vi's unnamed register is the Mac clipboard, or kept apart from it (D17, V4). */
  viClipboard: "shared" | "separate";
  fontFamily: string;
  /** The owner's choice, 100–900; drawn with the nearest real face (F13). */
  fontWeight: number;
  fontSize: number;
  /** The floating file window's own text size, so it can read larger than the Studio editor's `fontSize`. */
  panelFontSize: number;
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
  /** ED5 (T7): folding and unfolding slide the lines below. */
  foldAnimation: boolean;
  /** ED5 (T8): the minimap beside the text (not in the floating panel, not in the light mode). */
  minimap: boolean;
  /** ED5 (T9): the headers of the blocks around the top line stay pinned above the text. */
  stickyScroll: boolean;
  /** ED6 (L6): a line's worst language-server message also written after its text. */
  diagnosticsInline: boolean;
  /** ED6.5 (L14): ⌘S and `:w` format the file first. */
  formatOnSave: boolean;
  /** ED6.5 (L14): languages (editor ids, `python`, `markdown`) saved as they are. */
  formatOnSaveExcept: string[];
  /** editor-look K14: tabs tinted with their language's colour. */
  tabColors: boolean;
  /** editor-look K5: the file tree's icons — Catppuccin, Git (Octicons), JetBrains, monochrome or none. */
  fileIcons: FileIconStyle;
}

/** F12's defaults, confirmed by the owner. */
export const DEFAULT_EDITOR_SETTINGS: EditorSettings = {
  mode: "normal",
  viClipboard: "shared",
  fontFamily: "JetBrains Mono",
  fontWeight: 400,
  fontSize: 14,
  panelFontSize: 16,
  lineHeight: 1.5,
  ligatures: true,
  lineNumbers: "hybrid",
  wrapProse: true,
  wrapCode: false,
  indentKind: "spaces",
  indentSize: 4,
  tabSize: 4,
  autosave: "off",
  cursorAnimation: "strong",
  smoothScroll: true,
  currentLine: true,
  indentGuides: true,
  bracketColors: true,
  glow: true,
  foldAnimation: true,
  minimap: true,
  stickyScroll: true,
  diagnosticsInline: false,
  formatOnSave: true,
  tabColors: true,
  fileIcons: "catppuccin",
  formatOnSaveExcept: [],
};

const SETTINGS_VERSION = 1;
const SAVE_DEBOUNCE_MS = 400;

/** Bounds of the floating file window's text size, wider at the top than the Studio's 9–32 for reading. */
export const PANEL_FONT_SIZE_MIN = 10;
export const PANEL_FONT_SIZE_MAX = 40;
/** One zoom step, in points. */
export const PANEL_FONT_SIZE_STEP = 1;

function pick<T>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function number(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/** A list of language ids: strings only, trimmed, lower-case, no doubles. */
export function languageList(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const ids = value.filter((v): v is string => typeof v === "string").map((v) => v.trim().toLowerCase());
  return [...new Set(ids.filter((v) => /^[a-z0-9_+-]+$/.test(v)))];
}

function clampPanelFontSize(size: number): number {
  return Math.min(PANEL_FONT_SIZE_MAX, Math.max(PANEL_FONT_SIZE_MIN, size));
}

/** The next larger window text size; at the upper bound it stays where it is. */
export function panelFontSizeUp(size: number): number {
  return clampPanelFontSize(size + PANEL_FONT_SIZE_STEP);
}

/** The next smaller window text size; at the lower bound it stays where it is. */
export function panelFontSizeDown(size: number): number {
  return clampPanelFontSize(size - PANEL_FONT_SIZE_STEP);
}

/** The window text size's default, for the "reset" step. */
export function panelFontSizeReset(): number {
  return DEFAULT_EDITOR_SETTINGS.panelFontSize;
}

/** Whether saving `language` formats first (L14). */
export function formatsOnSave(settings: EditorSettings, language: string | null): boolean {
  return settings.formatOnSave && language !== null && !settings.formatOnSaveExcept.includes(language);
}

/** The settings in `raw` (a parsed file), every missing or bad field defaulted. */
export function parseEditorSettings(raw: unknown): EditorSettings {
  const r = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_EDITOR_SETTINGS;
  return {
    mode: pick(r.mode, ["normal", "vi"] as const, d.mode),
    viClipboard: pick(r.viClipboard, ["shared", "separate"] as const, d.viClipboard),
    // Any family whose name is safe to draw with: a bundled one, or one installed on the Mac (T10).
    fontFamily: typeof r.fontFamily === "string" && usableFamilyName(r.fontFamily) ? r.fontFamily : d.fontFamily,
    fontWeight: Math.round(number(r.fontWeight, 100, 900, d.fontWeight) / 100) * 100,
    fontSize: Math.round(number(r.fontSize, 9, 32, d.fontSize)),
    panelFontSize: Math.round(number(r.panelFontSize, PANEL_FONT_SIZE_MIN, PANEL_FONT_SIZE_MAX, d.panelFontSize)),
    lineHeight: number(r.lineHeight, 1, 2.5, d.lineHeight),
    ligatures: bool(r.ligatures, d.ligatures),
    lineNumbers: pick(r.lineNumbers, ["absolute", "relative", "hybrid"] as const, d.lineNumbers),
    wrapProse: bool(r.wrapProse, d.wrapProse),
    wrapCode: bool(r.wrapCode, d.wrapCode),
    indentKind: pick(r.indentKind, ["spaces", "tabs"] as const, d.indentKind),
    indentSize: Math.round(number(r.indentSize, 1, 8, d.indentSize)),
    tabSize: Math.round(number(r.tabSize, 1, 8, d.tabSize)),
    autosave: pick(r.autosave, ["off", "delay", "leave"] as const, d.autosave),
    // Saved before K8 as "glide" / "trail": the same two steps, renamed.
    cursorAnimation: pick(
      r.cursorAnimation === "glide" ? "subtle" : r.cursorAnimation === "trail" ? "strong" : r.cursorAnimation,
      ["off", "subtle", "strong"] as const,
      d.cursorAnimation,
    ),
    smoothScroll: bool(r.smoothScroll, d.smoothScroll),
    currentLine: bool(r.currentLine, d.currentLine),
    indentGuides: bool(r.indentGuides, d.indentGuides),
    bracketColors: bool(r.bracketColors, d.bracketColors),
    glow: bool(r.glow, d.glow),
    foldAnimation: bool(r.foldAnimation, d.foldAnimation),
    minimap: bool(r.minimap, d.minimap),
    stickyScroll: bool(r.stickyScroll, d.stickyScroll),
    diagnosticsInline: bool(r.diagnosticsInline, d.diagnosticsInline),
    formatOnSave: bool(r.formatOnSave, d.formatOnSave),
    tabColors: bool(r.tabColors, d.tabColors),
    fileIcons: pick(r.fileIcons, FILE_ICON_STYLES, d.fileIcons),
    formatOnSaveExcept: languageList(r.formatOnSaveExcept),
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
