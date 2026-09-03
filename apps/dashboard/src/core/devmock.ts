/**
 * Fixture backend for `vite` in a plain browser (DEV only, never bundled
 * into the Tauri app's code path — `invokeBackend` imports it lazily and only
 * when `__TAURI_INTERNALS__` is absent). Enough state to exercise every
 * module's happy path; not a faithful simulation.
 */

import type {
  AppInfo,
  ChatReply,
  LoadedDashboardState,
  MemoryStatus,
  NewRoutine,
  Routine,
  RunSummary,
  Skill,
  SyncReport,
} from "./backend";

const LATENCY_MS = 120;
const delay = () => new Promise((r) => setTimeout(r, LATENCY_MS));

let dashboardJson: string | null = null;
/** Set from the console (`window.__ax.mockCss = "..."`) to exercise the validator. */
let mockCustomCss: string | null = null;
export function setMockCustomCss(css: string | null): void {
  mockCustomCss = css;
}
const files = new Map<string, string>([
  [
    "notes/inbox.md",
    "# Inbox\n\nA scratch note in the *dev* workspace.\n\n- [x] wire md-file\n- [ ] ship M5\n\n| col | val |\n|-----|-----|\n| a | 1 |\n\n```ts\nconst x = 1;\n```\n",
  ],
  ["README.md", "# Axiomata-Workspace\n\nSecond-brain root.\n"],
]);
let memory: MemoryStatus = {
  workspace_root: "/Users/dev/Axiomata-Workspace",
  last_sync: new Date(Date.now() - 5 * 60_000).toISOString(),
  stale: true,
  tracked_files: 42,
};
const skills: Skill[] = [
  { name: "example-skill", description: "Bundled example skill.", backend: "claude-code" },
  { name: "sprint-planning", description: "Draft the next sprint plan.", backend: "claude-code" },
  { name: "newsletter", description: "Summarise the week into a newsletter.", backend: "ollama" },
];
let runs: RunSummary[] = [
  {
    id: 3,
    skill_name: "example-skill",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 2310,
    error: null,
    started_at: new Date(Date.now() - 40 * 60_000).toISOString(),
  },
  {
    id: 2,
    skill_name: "newsletter",
    backend: "ollama",
    status: "failed",
    exit_code: 1,
    duration_ms: 810,
    error: "model not found",
    started_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
  },
];
let routines: Routine[] = [
  {
    id: 1,
    name: "morning digest",
    cron_expr: "0 0 9 * * *",
    target: { type: "skill", value: "newsletter" },
    backend: null,
    enabled: true,
    next_fire_at: new Date(Date.now() + 2 * 3_600_000).toISOString(),
    last_fired_at: new Date(Date.now() - 22 * 3_600_000).toISOString(),
  },
  {
    id: 2,
    name: "evening ritual",
    cron_expr: "0 0 20 * * *",
    target: { type: "prompt", value: "Summarise today's notes." },
    backend: "claude-code",
    enabled: false,
    next_fire_at: null,
    last_fired_at: null,
  },
];

export async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  await delay();
  switch (cmd) {
    case "get_app_info":
      return {
        owner: "Dev",
        workspace_name: "Axiomata-Workspace",
        workspace_root: memory.workspace_root,
        version: "0.0.0-dev",
      } satisfies AppInfo as T;
    case "get_dashboard_state":
      return {
        json: dashboardJson ?? '{"version":1,"settings":{"theme":"graphite"},"canvas":{"instances":[]}}',
        recovered_backup: null,
      } satisfies LoadedDashboardState as T;
    case "save_dashboard_state":
      dashboardJson = String(args.json);
      return undefined as T;
    case "get_memory_status":
      return { ...memory } as T;
    case "sync_memory": {
      const report: SyncReport = {
        written: memory.stale ? ["CLAUDE.md", "projects/CLAUDE.md"] : [],
        unchanged: memory.stale ? 3 : 5,
        failed: [],
        tracked_files: memory.tracked_files,
      };
      memory = { ...memory, stale: false, last_sync: new Date().toISOString() };
      return report as T;
    }
    case "list_skills":
      return [...skills] as T;
    case "run_skill": {
      const name = String(args.name);
      const run: RunSummary = {
        id: (runs[0]?.id ?? 0) + 1,
        skill_name: name,
        backend: skills.find((s) => s.name === name)?.backend ?? "claude-code",
        status: "success",
        exit_code: 0,
        duration_ms: 1500,
        error: null,
        started_at: new Date().toISOString(),
      };
      runs = [run, ...runs];
      return run as T;
    }
    case "list_runs":
      return runs.slice(0, Number(args.limit ?? 25)) as T;
    case "list_routines":
      return [...routines] as T;
    case "add_routine": {
      const n = args.new as NewRoutine;
      const created: Routine = {
        id: (routines[routines.length - 1]?.id ?? 0) + 1,
        ...n,
        next_fire_at: new Date(Date.now() + 3_600_000).toISOString(),
        last_fired_at: null,
      };
      routines = [...routines, created];
      return created as T;
    }
    case "set_routine_enabled": {
      const id = Number(args.id);
      const enabled = Boolean(args.enabled);
      let found = false;
      routines = routines.map((r) => {
        if (r.id !== id) return r;
        found = true;
        return { ...r, enabled, next_fire_at: enabled ? new Date(Date.now() + 3_600_000).toISOString() : null };
      });
      return found as T;
    }
    case "assistant_send": {
      await new Promise((r) => setTimeout(r, 900));
      const message = String(args.message);
      const mode = String(args.mode);
      const session_id = typeof args.sessionId === "string" ? args.sessionId : `mock-${Date.now()}`;
      const reply =
        mode === "instruct"
          ? `Done (mock). I would have carried out:\n\n> ${message}\n\n_No files were touched in the browser mock._`
          : `You said: **${message}**\n\nThis is a mocked reply in session \`${session_id}\`.\n\n- markdown renders\n- \`code\` too`;
      return {
        session_id,
        reply_markdown: reply,
        is_error: false,
        cost_usd: 0.0012,
        usage: { input_tokens: 12, output_tokens: 40 },
        duration_ms: 900,
      } satisfies ChatReply as T;
    }
    case "load_custom_css":
      return (mockCustomCss ?? null) as T;
    case "write_module_manifest":
      return true as T;
    case "poll_module_actions":
      return [] as T;
    case "complete_module_action":
      return undefined as T;
    case "read_workspace_file": {
      const rel = String(args.rel);
      if (rel.startsWith("/") || rel.split("/").includes("..")) {
        throw new Error(`invalid workspace file ${rel}: resolves outside the workspace`);
      }
      const content = files.get(rel);
      if (content === undefined) throw new Error(`I/O error at ${rel}: No such file or directory`);
      return { path: rel, content, modified: new Date().toISOString() } as T;
    }
    case "write_workspace_file":
      files.set(String(args.rel), String(args.content));
      return undefined as T;
    default:
      throw new Error(`devmock: no fixture for "${cmd}"`);
  }
}
