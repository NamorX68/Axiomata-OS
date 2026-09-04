/**
 * Fixture backend for `vite` in a plain browser (DEV only, never bundled
 * into the Tauri app's code path — `invokeBackend` imports it lazily and only
 * when `__TAURI_INTERNALS__` is absent). Enough state to exercise every
 * module's happy path; not a faithful simulation.
 */

import type {
  AppInfo,
  ChatReply,
  GraphFile,
  GraphLink,
  WorkspaceGraph,
  LoadedDashboardState,
  MemoryStatus,
  NewRoutine,
  Routine,
  RunRecord,
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
  [
    "ToDo.md",
    "# ToDo\n\n- [ ] Steuerunterlagen sortieren\n- [ ] Rückruf Werkstatt\n\n## Done\n\n- [x] Reifen wechseln lassen (done: 2026-09-04)\n",
  ],
  ["Learning/Rust/GLOSSARY.md", "# Glossar\n\n- **Ownership** — wer den Wert besitzt.\n"],
  [
    "Learning/Rust/lessons/0000-roadmap.html",
    lessonPage("Roadmap", "Der Kurs in Etappen.", "0001-hallo-rust.html", "Hallo Rust"),
  ],
  [
    "Learning/Rust/lessons/0001-hallo-rust.html",
    lessonPage("Lektion 1 · Hallo Rust", "Erstes Programm mit <code>cargo run</code>.", "0002-variablen.html", "Variablen"),
  ],
  [
    "Learning/Rust/lessons/0002-variablen.html",
    lessonPage("Lektion 2 · Variablen &amp; Datentypen", "let, mut und Shadowing.", "0000-roadmap.html", "Roadmap"),
  ],
  ["Learning/BlockOS/0000-roadmap.html", lessonPage("BlockOS Roadmap", "Ein OS in Rust.", "0001-freestanding-binary.html", "Weiter")],
]);

/** A self-contained course page like the owner's: inline style, quiz script, relative link. */
function lessonPage(title: string, intro: string, nextHref: string, nextLabel: string): string {
  return `<!DOCTYPE html><html lang="de"><head><meta charset="utf-8"><title>${title}</title>
<style>body{font-family:Georgia,serif;background:#141219;color:#f4efe6;padding:2rem;max-width:720px;margin:auto}
h1{color:#ff6b1a}.quiz{background:#1e1b26;padding:1rem;border-radius:8px}button{background:#ff6b1a;border:0;padding:.5rem 1rem;border-radius:6px}
#result{margin-top:.5rem;color:#7bd88f}</style></head><body>
<h1>${title}</h1><p>${intro}</p>
<div class="quiz"><p>Quiz: Was gibt <code>let x = 5;</code> zurück?</p>
<label><input type="radio" name="q" value="a"> Nichts, es bindet x</label><br>
<label><input type="radio" name="q" value="b"> 5</label><br>
<button onclick="gradeQuiz()">Prüfen</button><div id="result"></div></div>
<p><a href="${nextHref}">${nextLabel} →</a> · <a href="https://doc.rust-lang.org/book/">Rust Book</a></p>
<script>function gradeQuiz(){const v=document.querySelector('input[name=q]:checked');document.getElementById('result').textContent=v&&v.value==='a'?'Richtig!':'Nochmal.';}</script>
</body></html>`;
}
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
  { name: "calendar-digest", description: "Reads upcoming calendar events via whichever calendar MCP tool is available.", backend: "claude-code" },
];

/** Fixture digest, same shape `calendar-digest`'s SOP produces — invented
 *  events, not the owner's real calendar. */
const CALENDAR_DIGEST_JSON = JSON.stringify({
  calendars: ["Arbeit", "Privat", "Familie"],
  events: [
    { title: "Team-Sync", start: new Date(Date.now() + 20 * 3_600_000).toISOString().slice(0, 16), end: new Date(Date.now() + 21 * 3_600_000).toISOString().slice(0, 16), calendar: "Arbeit", location: null, allDay: false },
    { title: "Zahnarzt", start: new Date(Date.now() + 2 * 86_400_000).toISOString().slice(0, 10), end: new Date(Date.now() + 2 * 86_400_000).toISOString().slice(0, 10), calendar: "Privat", location: "Praxis Dr. Beispiel", allDay: true },
    { title: "Geburtstag Mira", start: new Date(Date.now() + 5 * 86_400_000).toISOString().slice(0, 10), end: new Date(Date.now() + 5 * 86_400_000).toISOString().slice(0, 10), calendar: "Familie", location: null, allDay: true },
  ],
});

let runs: RunRecord[] = [
  {
    id: 4,
    skill_name: "calendar-digest",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 9200,
    stdout: CALENDAR_DIGEST_JSON,
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 10 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 10 * 60_000 + 9200).toISOString(),
  },
  {
    id: 3,
    skill_name: "example-skill",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 2310,
    stdout: "Working directory: /Users/dev/Axiomata-Workspace\nCLAUDE.md\nInbox\nLearning",
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 40 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 40 * 60_000 + 2310).toISOString(),
  },
  {
    id: 2,
    skill_name: "newsletter",
    backend: "ollama",
    status: "failed",
    exit_code: 1,
    duration_ms: 810,
    stdout: "",
    stderr: "Error: model not found",
    error: "model not found",
    started_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
    finished_at: new Date(Date.now() - 3 * 3_600_000 + 810).toISOString(),
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

/** Deterministic pseudo-random vault: 8 areas, ~260 notes, ~70 links. */
function mockGraph(): WorkspaceGraph {
  const areas: [string, number][] = [
    ["Entwicklung", 62],
    ["Arbeit", 38],
    ["KI", 44],
    ["System und Werkzeuge", 25],
    ["Fotografie", 18],
    ["Gesellschaft", 9],
    ["Persönlich", 31],
    ["Inbox", 12],
  ];
  let seed = 7;
  const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
  const files: GraphFile[] = [];
  for (const [area, n] of areas) {
    for (let i = 0; i < n; i++) {
      const title = `${area.split(" ")[0]} Notiz ${i + 1}`;
      files.push({
        path: `${area}/${title}.md`,
        area,
        title,
        bytes: Math.floor(200 + rnd() * 9000),
        modified: new Date(Date.now() - rnd() * 90 * 86_400_000).toISOString(),
        is_markdown: true,
      });
    }
  }
  files.push({ path: "README.md", area: null, title: "Vault", bytes: 300, modified: null, is_markdown: true });
  for (const [rel, title] of [
    ["Rust/GLOSSARY.md", "Glossar"],
    ["Rust/NOTES.md", "Notizen"],
    ["Rust/lessons/0000-roadmap.html", "Roadmap"],
    ["Rust/lessons/0001-hallo-rust.html", "Lektion 1 · Hallo Rust"],
    ["Rust/lessons/0002-variablen.html", "Lektion 2 · Variablen & Datentypen"],
    ["Rust/lessons/0003-funktionen.html", "Lektion 3 · Funktionen"],
    ["Rust/lessons/0004-kontrollfluss.html", "Lektion 4 · Kontrollfluss"],
    ["BlockOS/0000-roadmap.html", "BlockOS Roadmap"],
    ["BlockOS/0001-freestanding-binary.html", "Freestanding Binary"],
    ["BlockOS/0002-minimal-kernel.html", "Minimal Kernel"],
  ] as const) {
    files.push({ path: `Learning/${rel}`, area: "Learning", title, bytes: 24_000, modified: new Date().toISOString(), is_markdown: rel.endsWith(".md") });
  }
  const links: GraphLink[] = [
    { from: "Learning/Rust/lessons/0000-roadmap.html", to: "Learning/Rust/lessons/0001-hallo-rust.html" },
    { from: "Learning/Rust/lessons/0001-hallo-rust.html", to: "Learning/Rust/lessons/0002-variablen.html" },
    { from: "Learning/Rust/lessons/0002-variablen.html", to: "Learning/Rust/lessons/0003-funktionen.html" },
    { from: "Learning/BlockOS/0000-roadmap.html", to: "Learning/BlockOS/0001-freestanding-binary.html" },
  ];
  for (let i = 0; i < 70; i++) {
    const a = files[Math.floor(rnd() * files.length)];
    const b = files[Math.floor(rnd() * files.length)];
    if (a !== b) links.push({ from: a.path, to: b.path });
  }
  return {
    workspace_root: memory.workspace_root,
    hub: "CLAUDE.md",
    areas: [...areas.map(([name, n]) => ({ name, files: n })), { name: "Learning", files: 10 }],
    files,
    links,
    skills: skills.map((s) => ({ name: s.name, description: s.description, backend: s.backend, model: null, effort: null })),
    routines,
    total_files: files.length,
    truncated: false,
    generated_at: new Date().toISOString(),
  };
}

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
      const startedAt = new Date();
      const durationMs = 1500;
      const run: RunRecord = {
        id: (runs[0]?.id ?? 0) + 1,
        skill_name: name,
        backend: skills.find((s) => s.name === name)?.backend ?? "claude-code",
        status: "success",
        exit_code: 0,
        duration_ms: durationMs,
        stdout: name === "calendar-digest" ? CALENDAR_DIGEST_JSON : `Ran ${name}.`,
        stderr: "",
        error: null,
        started_at: startedAt.toISOString(),
        finished_at: new Date(startedAt.getTime() + durationMs).toISOString(),
      };
      runs = [run, ...runs];
      return run as T;
    }
    case "list_runs":
      // `RunSummary` deliberately omits `stdout`/`stderr`/`finished_at` — same
      // trim the real `list_runs` command does over the full `RunRecord` rows.
      return runs.slice(0, Number(args.limit ?? 25)).map(
        ({ stdout: _stdout, stderr: _stderr, finished_at: _finished_at, ...summary }): RunSummary => summary,
      ) as T;
    case "get_run": {
      const id = Number(args.id);
      return (runs.find((r) => r.id === id) ?? null) as T;
    }
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
    case "get_workspace_graph":
      return mockGraph() as T;
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
    case "create_note": {
      // No agent to ask in the browser mock — always files into "Inbox",
      // mirroring `notes::write_placed_note`'s dedup-on-collision rule. No
      // separate title argument either: use the content's own heading if it
      // has one, else a placeholder standing in for the agent's guess.
      const content = String(args.content).trim();
      const heading = /^#\s+(.+)/.exec(content)?.[1]?.trim();
      const title = heading || "Untitled";
      const markdown = heading ? `${content}\n` : `# ${title}\n\n${content}\n`;
      const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "note";
      let rel = `Inbox/${slug}.md`;
      for (let n = 2; files.has(rel); n++) rel = `Inbox/${slug}-${n}.md`;
      files.set(rel, markdown);
      return rel as T;
    }
    case "search_workspace": {
      const words = String(args.query).toLowerCase().split(/\s+/).filter(Boolean);
      const out: { path: string; line: number; snippet: string; matches: number }[] = [];
      if (words.length === 0) return out as T;
      for (const [path, content] of files) {
        let line = 0;
        let snippet = "";
        let matches = 0;
        const lines = content.split("\n");
        for (let i = 0; i < lines.length; i++) {
          const text = lines[i].replace(/<[^>]+>/g, "").replace(/&amp;/g, "&");
          if (words.every((w) => text.toLowerCase().includes(w))) {
            matches++;
            if (line === 0) {
              line = i + 1;
              snippet = text.trim().slice(0, 160);
            }
          }
        }
        if (matches > 0) out.push({ path, line, snippet, matches });
      }
      out.sort((a, b) => b.matches - a.matches || a.path.localeCompare(b.path));
      return out.slice(0, Number(args.limit ?? 40)) as T;
    }
    case "open_workspace_html": {
      const rel = String(args.rel);
      if (!/\.html?$/i.test(rel)) throw new Error(`${rel}: only .html / .htm files are framed`);
      if (!files.has(rel)) throw new Error(`I/O error at ${rel}: No such file or directory`);
      return `/Users/dev/Axiomata-Workspace/${rel}` as T;
    }
    default:
      throw new Error(`devmock: no fixture for "${cmd}"`);
  }
}
