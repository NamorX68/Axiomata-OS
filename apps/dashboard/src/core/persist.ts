/**
 * Layout persistence: `~/.axiomata/dashboard.json` ⇄ the stores.
 *
 * Boot: `initPersistence()` asks Rust for the file (or defaults), sanitises
 * the instance list (a hand-edit may be sloppy), loads the stores, then wires
 * a debounced save to every store mutation. The frontend owns the schema;
 * Rust only guarantees "object with numeric `version`" and does the atomic,
 * 0600 write. Unknown top-level / settings keys from a hand-edit are carried
 * through untouched.
 */

import { get } from "svelte/store";

import { invokeBackend as invoke, type LoadedDashboardState as LoadedState } from "./backend";
import { activeTheme, instances, loadInstances, onDirty } from "./stores";
import { DEFAULT_THEME, applyTheme } from "./themes";
import { toast } from "./toast";
import type { CanvasInstance } from "./types";

export const STATE_VERSION = 1;
export const SAVE_DEBOUNCE_MS = 400;

interface DashboardSettings extends Record<string, unknown> {
  theme: string;
  customCssPath: string | null;
}

interface DashboardState extends Record<string, unknown> {
  version: number;
  settings: DashboardSettings;
  canvas: { instances: CanvasInstance[] };
}

/** Everything from the loaded file except what the stores own, so hand-added
 *  keys survive a round-trip. */
let extras: Record<string, unknown> = {};
let extraSettings: Record<string, unknown> = {};
let customCssPath: string | null = null;

let timer: ReturnType<typeof setTimeout> | null = null;
let loading = false;
let started = false;

const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === "string" && v.length > 0;

/** Keeps only well-formed instances; a bad hand-edit drops the row, not the file. */
export function sanitizeInstances(raw: unknown): CanvasInstance[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const out: CanvasInstance[] = [];
  for (const item of raw) {
    if (typeof item !== "object" || item === null) continue;
    const r = item as Record<string, unknown>;
    if (!isStr(r.id) || !isStr(r.type) || seen.has(r.id)) continue;
    if (!isNum(r.x) || !isNum(r.y) || !isNum(r.w) || !isNum(r.h)) continue;
    seen.add(r.id);
    out.push({
      id: r.id,
      type: r.type,
      x: Math.max(0, r.x),
      y: Math.max(0, r.y),
      w: Math.max(1, r.w),
      h: Math.max(1, r.h),
      z: isNum(r.z) ? r.z : 0,
      flipped: r.flipped === true,
      config:
        typeof r.config === "object" && r.config !== null && !Array.isArray(r.config)
          ? (r.config as Record<string, unknown>)
          : {},
    });
  }
  return out;
}

export function parseState(text: string): DashboardState | null {
  try {
    const v = JSON.parse(text) as unknown;
    if (typeof v !== "object" || v === null || Array.isArray(v)) return null;
    const obj = v as Record<string, unknown>;
    const settings =
      typeof obj.settings === "object" && obj.settings !== null
        ? (obj.settings as Record<string, unknown>)
        : {};
    const canvas =
      typeof obj.canvas === "object" && obj.canvas !== null
        ? (obj.canvas as Record<string, unknown>)
        : {};
    return {
      ...obj,
      version: isNum(obj.version) ? obj.version : STATE_VERSION,
      settings: {
        ...settings,
        theme: isStr(settings.theme) ? settings.theme : DEFAULT_THEME,
        customCssPath: isStr(settings.customCssPath) ? settings.customCssPath : null,
      },
      canvas: { instances: sanitizeInstances(canvas.instances) },
    };
  } catch {
    return null;
  }
}

export function buildState(): DashboardState {
  return {
    ...extras,
    version: STATE_VERSION,
    settings: { ...extraSettings, theme: get(activeTheme), customCssPath },
    canvas: { instances: get(instances) },
  };
}

export function getCustomCssPath(): string | null {
  return customCssPath;
}

export async function initPersistence(): Promise<void> {
  if (started) return;
  started = true;

  let loaded: LoadedState;
  try {
    loaded = await invoke<LoadedState>("get_dashboard_state");
  } catch (err) {
    toast(`Could not read dashboard.json: ${String(err)}`, "danger");
    return;
  }

  const state = parseState(loaded.json);
  if (state) {
    const { version: _v, settings, canvas, ...rest } = state;
    const { theme, customCssPath: css, ...restSettings } = settings;
    extras = rest;
    extraSettings = restSettings;
    customCssPath = css;
    loading = true;
    loadInstances(canvas.instances);
    applyTheme(theme);
    loading = false;
  }

  if (loaded.recovered_backup) {
    toast(`dashboard.json was unreadable and moved to ${loaded.recovered_backup}.`, "warning");
  } else if (!state) {
    toast("dashboard.json could not be parsed; starting with an empty canvas.", "warning");
  }

  onDirty(scheduleSave);
  // Svelte stores call the subscriber once immediately, so this also
  // materialises dashboard.json on the very first boot — handy for hand-edits.
  activeTheme.subscribe(() => {
    if (!loading) scheduleSave();
  });
  window.addEventListener("pagehide", () => void flush());
}

export function scheduleSave(): void {
  if (loading) return;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void flush(), SAVE_DEBOUNCE_MS);
}

/** Write now (cancels a pending debounce). */
export async function flush(): Promise<void> {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
  const json = JSON.stringify(buildState(), null, 2) + "\n";
  try {
    await invoke("save_dashboard_state", { json });
  } catch (err) {
    toast(`Could not save dashboard.json: ${String(err)}`, "danger");
  }
}
