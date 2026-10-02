/**
 * The one debug session of the Studio (#51), shared by the Debug view, the editor gutters and the
 * pane that opens the file at a stop.
 *
 * One program is debugged at a time (the Rust side holds one session too), so this is a module
 * singleton rather than something each component owns: the editor's gutter has to know where the
 * program stands without being handed the controller through every pane in between.
 */
import { derived, get, writable, type Readable } from "svelte/store";

import { getSetting, setSetting } from "../core/persist";

import {
  applyEdit,
  breakpointInfo,
  breakpoints,
  setBreakpointInfo,
  specsOf,
  toggleBreakpoint,
  type BpInfo,
  type EditSpan,
} from "./breakpoints";
import { createDebugController, realBackend } from "./debugSession";
import type { DebugTarget, StackFrame } from "./debugBackend";
import { absoluteInside } from "./outputPath";

/** Where the running session lives: the project root id and its folder on disk. */
interface Running {
  root: string;
  folder: string;
}

const running = writable<Running | null>(null);
/** The last problem worth showing, cleared when a new session starts. */
export const debugProblem = writable<string | null>(null);

let reveal: ((root: string, rel: string, line: number) => void) | null = null;

/** The host opens the file at the stop (the Studio's dock). */
export function onDebugReveal(handler: ((root: string, rel: string, line: number) => void) | null): void {
  reveal = handler;
}

let terminalHandler: ((title: string, line: string) => void) | null = null;

/** The host runs the program in a terminal pane when the debugger asks for one. */
export function onDebugTerminal(handler: ((title: string, line: string) => void) | null): void {
  terminalHandler = handler;
}

function show(frame: StackFrame): void {
  const at = get(running);
  if (!at || !frame.path) return;
  const rel = absoluteInside(frame.path, at.folder);
  if (rel !== null) reveal?.(at.root, rel, frame.line);
}

export const debug = createDebugController(realBackend, {
  onStop: show,
  onTerminal: (title, line) => terminalHandler?.(title, line),
  onError: (message) => debugProblem.set(message),
  isUserFrame: (frame) => {
    const at = get(running);
    return !!at && absoluteInside(frame.path, at.folder) !== null;
  },
});

/** Where the selected frame stands, in the editor's terms (`line` zero-based); `null` unless stopped. */
export interface ExecPoint {
  root: string;
  rel: string;
  line: number;
}

export const execPoint: Readable<ExecPoint | null> = derived([debug.state, running], ([s, at]) => {
  if (!at || s.phase !== "stopped") return null;
  const frame = s.frames.find((f) => f.id === s.frameId) ?? s.frames[0];
  if (!frame?.path) return null;
  const rel = absoluteInside(frame.path, at.folder);
  return rel === null ? null : { root: at.root, rel, line: Math.max(0, frame.line - 1) };
});

/** Starts a session; the breakpoints are the ones the owner set in this project. */
export async function startDebugging(
  root: string,
  folder: string,
  target: DebugTarget,
  name: string,
  terminal = false,
  args: string[] | null = null,
): Promise<void> {
  debugProblem.set(null);
  running.set({ root, folder });
  await debug.start(root, target, specsOf(get(breakpoints), get(breakpointInfo), root), name, terminal, args);
}

/** Tells a running session the current breakpoints of one file (lines and their extras). */
function syncFile(root: string, rel: string): void {
  const files = specsOf(get(breakpoints), get(breakpointInfo), root);
  void debug.syncBreakpoints(rel, files.find((f) => f.rel === rel)?.breakpoints ?? []);
}

/** Toggles a breakpoint in the editor or the list; a running session learns of it at once. */
export function toggleDebugBreakpoint(root: string, rel: string, line: number): void {
  toggleBreakpoint(root, rel, line);
  syncFile(root, rel);
}

/** Gives a breakpoint a condition, a hit count or a log message (`null` = back to a plain breakpoint). */
export function editDebugBreakpoint(root: string, rel: string, line: number, info: BpInfo | null): void {
  setBreakpointInfo(root, rel, line, info);
  syncFile(root, rel);
}

/** An edit in the editor moved lines: the breakpoints and their extras go along; a running session learns it. */
export function moveDebugBreakpoints(root: string, rel: string, edit: EditSpan, lineCountBefore: number): void {
  if (applyEdit(root, rel, edit, lineCountBefore) !== null) syncFile(root, rel);
}

// ---- watch expressions, kept per project ----------------------------------------------------------------


const SETTINGS_KEY = "ide";

/** The watch expressions remembered for `root`. */
export function loadWatches(root: string): string[] {
  const all = getSetting<{ watches?: Record<string, unknown> }>(SETTINGS_KEY)?.watches;
  const list = all?.[root];
  return Array.isArray(list) ? list.filter((e): e is string => typeof e === "string" && e.trim() !== "").slice(0, 50) : [];
}

export function saveWatches(root: string, expressions: string[]): void {
  const current = getSetting<{ watches?: Record<string, string[]> }>(SETTINGS_KEY) ?? {};
  setSetting(SETTINGS_KEY, { ...current, watches: { ...current.watches, [root]: expressions } });
}
