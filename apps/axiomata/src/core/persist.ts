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

import { brainView, legacyMotionLevel, readBrainView } from "./brainView";
import { isMotionLevel, motionSetting } from "./motion";
import { isUiFont, uiFont } from "./uiFont";
import { get, writable, type Readable } from "svelte/store";

import { appGroups, loadAppGroups, type AppGroup } from "./appGroups";
import { effects, readEffects } from "./effects";
import { hiddenBuiltins, loadHiddenBuiltins, loadUserApps, userApps, type UserApp } from "./apps";
import { invokeBackend as invoke, type LoadedDashboardState as LoadedState } from "./backend";
import { RETIRED_TILE_TYPES } from "./ringTypes";
import { activeTheme, instances, loadInstances, onDirty, showGrid, snapEdges, windowTransparency } from "./stores";
import { DEFAULT_THEME, applyTheme } from "./themes";
import { isKeyboardSetting, keyboardSetting } from "./keys";
import { isUiSize, uiSize } from "./uiScale";
import { toast } from "./toast";
import type { CanvasInstance, TileAnchor } from "./types";

const dashboardRead = writable(false);

/**
 * Whether the saved dashboard has been read (or could not be). The app mounts before it is, so anything that judges
 * the layout — the empty canvas — must wait for it, or every start would flash "empty".
 */
export const dashboardLoaded: Readable<boolean> = { subscribe: dashboardRead.subscribe };

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
  apps: { user: UserApp[]; groups: AppGroup[]; hiddenBuiltins: string[] };
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
    if (RETIRED_TILE_TYPES.includes(r.type)) continue;
    if (!isNum(r.x) || !isNum(r.y) || !isNum(r.w) || !isNum(r.h)) continue;
    seen.add(r.id);
    const a = r.anchor as Record<string, unknown> | undefined;
    const anchor: TileAnchor | undefined =
      a && (a.x === "left" || a.x === "right") && (a.y === "top" || a.y === "bottom") && isNum(a.w) && isNum(a.h)
        ? { x: a.x, y: a.y, w: a.w, h: a.h }
        : undefined;
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
      ...(anchor ? { anchor } : {}),
    });
  }
  return out;
}

/** Keeps only well-formed rows, deduplicated by path — a bad hand-edit drops
 *  the row, not the file (same contract as `sanitizeInstances`). */
export function sanitizeUserApps(raw: unknown): UserApp[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const out: UserApp[] = [];
  for (const item of raw) {
    if (typeof item !== "object" || item === null) continue;
    const r = item as Record<string, unknown>;
    if (!isStr(r.path) || !isStr(r.name) || seen.has(r.path)) continue;
    seen.add(r.path);
    out.push({ path: r.path, name: r.name, ...(isStr(r.glyph) ? { glyph: r.glyph } : {}) });
  }
  return out;
}

/** Keeps only non-empty string entries, deduplicated — a bad hand-edit drops
 *  the entry, not the file (same contract as `sanitizeInstances`/
 *  `sanitizeUserApps`). A hidden-but-no-longer-ring-eligible/registered
 *  type is left in untouched: `apps.ts`'s `listBuiltinApps` already just
 *  ignores anything not in `listAllRingEligibleBuiltins`, so there's
 *  nothing here that needs cross-checking against the registry. */
export function sanitizeHiddenBuiltins(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  for (const item of raw) {
    if (isStr(item)) seen.add(item);
  }
  return [...seen];
}

/** Keeps only well-formed groups; a bad hand-edit drops the row, not the
 *  file (same contract as `sanitizeInstances`/`sanitizeUserApps`). Members
 *  are deduplicated within a group and, per side, across groups — the
 *  first group in file order keeps a member a later group's hand-edit also
 *  lists, so one app is never rendered on two ring slots at once. A group
 *  left with no members after dedup is dropped entirely — a 0-member group
 *  doesn't exist even transiently, matching `appGroups.ts`'s own
 *  auto-dissolve-at-zero-members behaviour. */
export function sanitizeAppGroups(raw: unknown): AppGroup[] {
  if (!Array.isArray(raw)) return [];
  const seenIds = new Set<string>();
  const seenMembers: Record<"builtin" | "user", Set<string>> = { builtin: new Set(), user: new Set() };
  const out: AppGroup[] = [];
  for (const item of raw) {
    if (typeof item !== "object" || item === null) continue;
    const r = item as Record<string, unknown>;
    if (!isStr(r.id) || !isStr(r.name) || seenIds.has(r.id)) continue;
    if (r.side !== "builtin" && r.side !== "user") continue;
    const side = r.side;
    const glyph = isStr(r.glyph) ? r.glyph : "group";
    const rawMembers = Array.isArray(r.members) ? r.members : [];
    const members: string[] = [];
    for (const m of rawMembers) {
      if (!isStr(m) || seenMembers[side].has(m)) continue;
      seenMembers[side].add(m);
      members.push(m);
    }
    if (members.length === 0) continue;
    seenIds.add(r.id);
    out.push({ id: r.id, side, name: r.name, glyph, members });
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
    const apps =
      typeof obj.apps === "object" && obj.apps !== null
        ? (obj.apps as Record<string, unknown>)
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
      apps: {
        user: sanitizeUserApps(apps.user),
        groups: sanitizeAppGroups(apps.groups),
        hiddenBuiltins: sanitizeHiddenBuiltins(apps.hiddenBuiltins),
      },
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
    apps: { user: get(userApps), groups: get(appGroups), hiddenBuiltins: get(hiddenBuiltins) },
  };
}

export function getCustomCssPath(): string | null {
  return customCssPath;
}

/** Any extra `settings.<key>` from dashboard.json (e.g. Second Brain view
 *  preferences). Unknown keys round-trip untouched. */
export function getSetting<T>(key: string): T | undefined {
  return extraSettings[key] as T | undefined;
}

export function setSetting(key: string, value: unknown): void {
  extraSettings = { ...extraSettings, [key]: value };
  scheduleSave();
}

export async function initPersistence(): Promise<void> {
  if (started) return;
  started = true;

  let loaded: LoadedState;
  try {
    loaded = await invoke<LoadedState>("get_dashboard_state");
  } catch (err) {
    toast(`Could not read dashboard.json: ${String(err)}`, "danger");
    dashboardRead.set(true);
    return;
  }

  const state = parseState(loaded.json);
  if (state) {
    const { version: _v, settings, canvas, apps, ...rest } = state;
    const { theme, customCssPath: css, ...restSettings } = settings;
    extras = rest;
    extraSettings = restSettings;
    customCssPath = css;
    loading = true;
    loadInstances(canvas.instances);
    loadUserApps(apps.user);
    loadAppGroups(apps.groups);
    loadHiddenBuiltins(apps.hiddenBuiltins);
    applyTheme(theme);
    loading = false;
  }

  if (loaded.recovered_backup) {
    toast(`dashboard.json was unreadable and moved to ${loaded.recovered_backup}.`, "warning");
  } else if (!state) {
    toast("dashboard.json could not be parsed; starting with an empty canvas.", "warning");
  }

  loading = true;
  showGrid.set(getSetting<boolean>("showGrid") === true);
  snapEdges.set(getSetting<boolean>("snapEdges") !== false);
  windowTransparency.set(getSetting<number>("windowTransparency") ?? 50);
  const savedUiSize = getSetting<unknown>("uiSize");
  uiSize.set(isUiSize(savedUiSize) ? savedUiSize : "auto");
  const savedKeyboard = getSetting<unknown>("keyboard");
  keyboardSetting.set(isKeyboardSetting(savedKeyboard) ? savedKeyboard : "auto");
  // Written back at once: the Second Brain view rewrites its own preferences, and the old rotation/file-names values that
  // this is taken over from would be gone before the first change.
  const storedBrainView = getSetting<unknown>("brainView");
  const savedBrainView = readBrainView(storedBrainView, getSetting<unknown>("secondBrain"));
  brainView.set(savedBrainView);
  setSetting("brainView", savedBrainView);
  // The app-wide motion level replaced the graph's own switch (`core/motion.ts`); a graph that was switched off keeps
  // standing still, once, until the level has been stored.
  const savedMotion = getSetting<unknown>("motion");
  motionSetting.set(
    isMotionLevel(savedMotion)
      ? savedMotion
      : (legacyMotionLevel(storedBrainView, getSetting<unknown>("secondBrain")) ?? "full"),
  );
  setSetting("motion", get(motionSetting));
  const savedUiFont = getSetting<unknown>("uiFont");
  if (isUiFont(savedUiFont)) uiFont.set(savedUiFont);
  effects.set(readEffects(getSetting<unknown>("effects")));
  loading = false;
  dashboardRead.set(true);
  onDirty(scheduleSave);
  // Svelte stores call the subscriber once immediately, so this also
  // materialises dashboard.json on the very first boot — handy for hand-edits.
  activeTheme.subscribe(() => {
    if (!loading) scheduleSave();
  });
  userApps.subscribe(() => {
    if (!loading) scheduleSave();
  });
  appGroups.subscribe(() => {
    if (!loading) scheduleSave();
  });
  hiddenBuiltins.subscribe(() => {
    if (!loading) scheduleSave();
  });
  let first = true;
  showGrid.subscribe((v) => {
    if (!first) setSetting("showGrid", v);
  });
  snapEdges.subscribe((v) => {
    if (!first) setSetting("snapEdges", v);
  });
  brainView.subscribe((v) => {
    if (!first) setSetting("brainView", v);
  });
  // `motion.ts` and `uiFont.ts` put these on `<html>`; only their persistence is here.
  motionSetting.subscribe((v) => {
    if (!first) setSetting("motion", v);
  });
  uiFont.subscribe((v) => {
    if (!first) setSetting("uiFont", v);
  });
  // `shell/Ambient.svelte` reads it; only its persistence is here.
  effects.subscribe((v) => {
    if (!first) setSetting("effects", v);
  });
  // `uiScale.ts` applies it; only its persistence is here.
  uiSize.subscribe((v) => {
    if (!first) setSetting("uiSize", v);
  });
  // `keys.ts` reads it; only its persistence is here.
  keyboardSetting.subscribe((v) => {
    if (!first) setSetting("keyboard", v);
  });
  // Its effect on the glass is `glass.ts`'s; only its persistence is here.
  windowTransparency.subscribe((v) => {
    if (!first) setSetting("windowTransparency", v);
  });
  first = false;
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
