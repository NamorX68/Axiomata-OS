/**
 * Shapes mirrored from the Rust command layer (`src-tauri/src/commands.rs`
 * and the core structs they serialise) plus the one `invokeBackend` every
 * shell component and module goes through.
 *
 * Outside Tauri (plain `vite` in a browser, used for layout work and
 * screenshots) `invokeBackend` falls back to `core/devmock.ts` in DEV builds
 * so modules render with fixture data instead of a wall of errors.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export interface AppInfo {
  owner: string;
  workspace_name: string;
  workspace_root: string;
  version: string;
}

export interface Skill {
  name: string;
  description: string;
  backend: string;
}

export type RunStatus = "success" | "failed";

export interface RunSummary {
  id: number;
  skill_name: string;
  backend: string;
  status: RunStatus;
  exit_code: number | null;
  duration_ms: number;
  error: string | null;
  started_at: string;
}

export interface MemoryStatus {
  workspace_root: string;
  last_sync: string | null;
  stale: boolean;
  tracked_files: number;
}

export interface SyncReport {
  written: string[];
  unchanged: number;
  failed: [string, string][];
  tracked_files: number;
}

export type RoutineTarget = { type: "skill"; value: string } | { type: "prompt"; value: string };

export interface Routine {
  id: number;
  name: string;
  cron_expr: string;
  target: RoutineTarget;
  backend: string | null;
  enabled: boolean;
  next_fire_at: string | null;
  last_fired_at: string | null;
}

export interface NewRoutine {
  name: string;
  cron_expr: string;
  target: RoutineTarget;
  backend: string | null;
  enabled: boolean;
}

export interface WorkspaceFile {
  path: string;
  content: string;
  modified: string | null;
}

export interface LoadedDashboardState {
  json: string;
  recovered_backup: string | null;
}

export type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let devMock: InvokeFn | null = null;

/** `invoke` with the DEV browser fallback. */
export const invokeBackend: InvokeFn = async <T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> => {
  if (insideTauri()) return tauriInvoke<T>(cmd, args);
  if (import.meta.env.DEV) {
    devMock ??= (await import("./devmock")).mockInvoke;
    return devMock<T>(cmd, args);
  }
  throw new Error(`invoke(${cmd}): not running inside Tauri`);
};
