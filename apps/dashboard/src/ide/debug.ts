/**
 * The one debug session of the Studio (#51), shared by the Debug view, the editor gutters and the
 * pane that opens the file at a stop.
 *
 * One program is debugged at a time (the Rust side holds one session too), so this is a module
 * singleton rather than something each component owns: the editor's gutter has to know where the
 * program stands without being handed the controller through every pane in between.
 */
import { derived, get, writable, type Readable } from "svelte/store";

import { breakpoints, filesOf, linesOf, toggleBreakpoint } from "./breakpoints";
import { createDebugController, realBackend } from "./debugSession";
import type { DebugTarget, StackFrame } from "./debugBackend";
import { relativeInside } from "./outputPath";

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
  const rel = relativeInside(frame.path, at.folder);
  if (rel !== null) reveal?.(at.root, rel, frame.line);
}

export const debug = createDebugController(realBackend, {
  onStop: show,
  onTerminal: (title, line) => terminalHandler?.(title, line),
  onError: (message) => debugProblem.set(message),
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
  const rel = relativeInside(frame.path, at.folder);
  return rel === null ? null : { root: at.root, rel, line: Math.max(0, frame.line - 1) };
});

/** Starts a session; the breakpoints are the ones the owner set in this project. */
export async function startDebugging(root: string, folder: string, target: DebugTarget, name: string, terminal = false): Promise<void> {
  debugProblem.set(null);
  running.set({ root, folder });
  await debug.start(root, target, filesOf(get(breakpoints), root), name, terminal);
}

/** Toggles a breakpoint in the editor or the list; a running session learns of it at once. */
export function toggleDebugBreakpoint(root: string, rel: string, line: number): void {
  toggleBreakpoint(root, rel, line);
  void debug.syncBreakpoints(rel, [...linesOf(get(breakpoints), root, rel)].sort((a, b) => a - b));
}
