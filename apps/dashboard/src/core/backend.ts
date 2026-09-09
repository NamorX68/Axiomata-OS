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

/** Model-routing provider for the `claude` child process. Serde
 *  `rename_all = "snake_case"` variants of the Rust `ProviderId` enum. */
export type ProviderId = "anthropic" | "open_router" | "ollama";

/** Mirrors `axiomata_core::spend::SpendSummary` — `get_spend_summary`. */
export interface SpendSummary {
  provider: string;
  today_usd: number;
  month_usd: number;
  daily_cap_usd: number | null;
  /** `false` for the subscribed Anthropic provider. */
  metered: boolean;
}

// ---- Config: redacted view (get_config) + update payload (save_config) ----
// The raw `api_key` / `claude_env` values never cross the IPC boundary
// (provider-hardening CP7). `get_config` returns a `has_key` flag and the
// `claude_env` key names only; `save_config` takes key changes as a
// `KeyUpdate`, and the Rust side merges the retained secret back in.

/** One provider's settings as `get_config` returns them — no raw key. */
export interface ProviderSettingsView {
  base_url: string | null;
  /** Whether a credential is stored. The value is never sent. */
  has_key: boolean;
  chat_model: string;
  skill_model: string;
}

export interface AgentDefaultsView {
  ollama_model: string;
  skill_timeout_secs: number;
  providers: Record<ProviderId, ProviderSettingsView>;
  active_provider: ProviderId;
  /** Daily USD spend cap for a paid provider; `null` disables it. */
  daily_usd_cap: number | null;
  /** Names only of the `agents.claude_env` overrides. */
  claude_env_keys: string[];
}

/** What `get_config` returns; `workspace_root` is a plain string path. */
export interface ConfigView {
  owner: string;
  workspace_root: string;
  agents: AgentDefaultsView;
}

/** How a provider's stored key should change on save. `keep` = field
 *  untouched, `clear` = field emptied, `set` = new value typed. */
export type KeyUpdate = { kind: "keep" } | { kind: "clear" } | { kind: "set"; value: string };

export interface ProviderSettingsUpdate {
  base_url: string | null;
  api_key: KeyUpdate;
  chat_model: string;
  skill_model: string;
}

export interface AgentDefaultsUpdate {
  ollama_model: string;
  skill_timeout_secs: number;
  providers: Record<ProviderId, ProviderSettingsUpdate>;
  active_provider: ProviderId;
  daily_usd_cap: number | null;
}

/** The payload `save_config` takes — no `claude_env`, no `claude_model`. */
export interface ConfigUpdate {
  owner: string;
  workspace_root: string;
  agents: AgentDefaultsUpdate;
}

export interface Skill {
  name: string;
  description: string;
  backend: string;
}

/** A skill directory `list_skills` could not turn into a `Skill` — a broken
 *  `SKILL.md`, a symlink, … — surfaced instead of just vanishing. */
export interface SkippedSkill {
  name: string;
  reason: string;
}

export type RunStatus = "success" | "failed";

/** Who/what triggered a run — a person, or a routine firing unattended. */
export type RunSource = "manual" | "routine";

export interface RunSummary {
  id: number;
  skill_name: string;
  backend: string;
  status: RunStatus;
  exit_code: number | null;
  duration_ms: number;
  error: string | null;
  started_at: string;
  source: RunSource;
}

/** The full record `run_skill` / `get_run` return — `RunSummary` plus the
 *  captured output `list_runs` deliberately omits. */
export interface RunRecord extends RunSummary {
  stdout: string;
  stderr: string;
  finished_at: string;
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

/** A raster image read via `read_workspace_image` — `mime` is one of
 *  `image/png`, `image/jpeg`, `image/gif`, `image/webp`; `base64` is ready
 *  to wrap into a `data:<mime>;base64,<...>` URI. */
export interface WorkspaceImage {
  path: string;
  mime: string;
  base64: string;
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
