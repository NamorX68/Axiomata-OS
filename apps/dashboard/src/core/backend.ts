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

export interface SearchHit {
  path: string;
  line: number;
  snippet: string;
  matches: number;
}

export interface WorkspaceFile {
  path: string;
  content: string;
  modified: string | null;
}

export interface GraphFile {
  path: string;
  area: string | null;
  title: string;
  bytes: number;
  modified: string | null;
  is_markdown: boolean;
}

export interface GraphArea {
  name: string;
  files: number;
}

export interface GraphLink {
  from: string;
  to: string;
}

export interface GraphSkill {
  name: string;
  description: string;
  backend: string;
  model: string | null;
  effort: string | null;
}

export interface WorkspaceGraph {
  workspace_root: string;
  hub: string | null;
  areas: GraphArea[];
  files: GraphFile[];
  links: GraphLink[];
  skills: GraphSkill[];
  routines: Routine[];
  total_files: number;
  truncated: boolean;
  generated_at: string;
}

export type ChatMode = "chat" | "instruct";

export interface ChatReply {
  session_id: string;
  reply_markdown: string;
  is_error: boolean;
  cost_usd: number | null;
  usage: unknown;
  duration_ms: number;
}

export interface LoadedDashboardState {
  json: string;
  recovered_backup: string | null;
}

export type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Builds an `asset://` URL for an absolute file path, preserving `/` as
 * literal path separators — unlike `@tauri-apps/api/core`'s
 * `convertFileSrc`, which runs `encodeURIComponent` over the *whole* path
 * and so turns every `/` into `%2F`, collapsing it into one opaque URL
 * segment with no directory structure left for the browser to see.
 *
 * That breaks *relative* links inside anything framed through it: resolving
 * `href="0003-funktionen.html"` against a base URL with no literal slash to
 * drop the last segment from lands back at `asset://localhost/0003-...html`
 * (i.e. the lesson's own folder is lost), which Tauri's asset handler then
 * can't resolve. Tauri's handler percent-decodes the whole request path as
 * one string regardless of which form arrives (`tauri::protocol::asset`),
 * so a URL built with real `/` separators between individually-encoded
 * segments decodes to the exact same absolute path and is served
 * identically — it just also lets a multi-page course navigate between its
 * own lessons.
 *
 * macOS only for now (Tauri serves `http://asset.localhost/<path>` instead
 * on Windows/Android) — this app has no other target yet, see CLAUDE.md.
 */
export function assetFileUrl(absPath: string): string {
  const encoded = absPath.split("/").map(encodeURIComponent).join("/");
  return `asset://localhost${encoded}`;
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
