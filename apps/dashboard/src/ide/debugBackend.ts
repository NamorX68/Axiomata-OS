/**
 * The debugger, frontend side (`docs/plans/editor-projekt-werkzeuge.md`, #51): the `debug_*` commands over
 * `axiomata-dap`. Only this file knows the wire shape (`snake_case`; Tauri turns argument names into camelCase).
 */

import { Channel } from "@tauri-apps/api/core";

import { insideTauri, invokeBackend as invoke } from "../core/backend";
import type { NewDebugConfig } from "./debugForm";

export interface DebugConfigInfo {
  name: string;
  language: "python";
  program: string | null;
  module: string | null;
  args: string[];
  cwd: string | null;
  env: [string, string][];
  just_my_code: boolean;
  /** Found from the project's files rather than written in its `debug.json`. */
  detected: boolean;
}

export interface DebugListInfo {
  configs: DebugConfigInfo[];
  project_file: { hash: string; trusted: boolean } | null;
  problems: string[];
}

/** What a session reports, as `DebugEvent` serialises in Rust (tagged by `event`). */
export type DebugEvent =
  | { event: "stopped"; thread_id: number; reason: string; text: string | null }
  | { event: "continued"; thread_id: number }
  | { event: "output"; category: string; text: string }
  | { event: "exited"; code: number }
  | { event: "terminated" }
  | { event: "closed"; stderr: string };

export interface StackFrame {
  id: number;
  name: string;
  path: string | null;
  line: number;
  column: number;
}

export interface DebugScope {
  name: string;
  variables_reference: number;
  expensive: boolean;
}

export interface DebugVariable {
  name: string;
  value: string;
  type_name: string | null;
  variables_reference: number;
}

export interface BreakpointInfo {
  line: number;
  verified: boolean;
  message: string | null;
}

export type DebugTarget = { kind: "named"; name: string } | { kind: "current_file"; rel: string };
export type DebugAction = "continue" | "next" | "step_in" | "step_out" | "pause";

export const listDebugConfigs = (root: string): Promise<DebugListInfo> => invoke<DebugListInfo>("debug_configs", { root });

/** The owner confirmed the project's `debug.json` as shown (`hash` is what was shown). */
export const trustDebugFile = (root: string, hash: string): Promise<void> => invoke<void>("debug_trust", { root, hash });

/** Starts a session; its events arrive on `onEvent` until it ends. */
export function startDebug(
  root: string,
  target: DebugTarget,
  breakpoints: { rel: string; lines: number[] }[],
  onEvent: (event: DebugEvent) => void,
): Promise<void> {
  // The browser mock has no Tauri channel; it calls the same `onmessage` on a plain object.
  const channel = insideTauri() ? new Channel<DebugEvent>() : { onmessage: (_: DebugEvent) => {} };
  channel.onmessage = onEvent;
  return invoke<void>("debug_start", { root, target, breakpoints, onEvent: channel });
}

export const stopDebug = (): Promise<void> => invoke<void>("debug_stop");
export const controlDebug = (action: DebugAction): Promise<void> => invoke<void>("debug_control", { action });
export const debugStack = (): Promise<StackFrame[]> => invoke<StackFrame[]>("debug_stack");
export const debugScopes = (frameId: number): Promise<DebugScope[]> => invoke<DebugScope[]>("debug_scopes", { frameId });
export const debugVariables = (variablesReference: number): Promise<DebugVariable[]> =>
  invoke<DebugVariable[]>("debug_variables", { variablesReference });
export const debugEvaluate = (expression: string, frameId: number | null): Promise<DebugVariable> =>
  invoke<DebugVariable>("debug_evaluate", { expression, frameId });
export const debugSetBreakpoints = (rel: string, lines: number[]): Promise<BreakpointInfo[]> =>
  invoke<BreakpointInfo[]>("debug_set_breakpoints", { rel, lines });

/** Languages the debugger can start for: the file in front is offered when it is one of these. */
export function isDebuggable(rel: string): boolean {
  return rel.toLowerCase().endsWith(".py");
}

/** Saves a configuration of your own into the project's `debug.json` (`replace` = the name being edited). */
export const saveDebugConfig = (root: string, config: NewDebugConfig, replace: string | null): Promise<void> =>
  invoke<void>("debug_save", { root, config, replace });

export const removeDebugConfig = (root: string, name: string): Promise<void> => invoke<void>("debug_remove", { root, name });
