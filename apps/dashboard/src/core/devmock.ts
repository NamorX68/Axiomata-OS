/**
 * Fixture backend for `vite` in a plain browser (DEV only, never bundled
 * into the Tauri app's code path — `invokeBackend` imports it lazily and only
 * when `__TAURI_INTERNALS__` is absent). Enough state to exercise every
 * module's happy path; not a faithful simulation.
 */

import type {
  AppInfo,
  Board,
  BoardCard,
  BoardColumn,
  CardFields,
  NewCard,
  NewColumn,
  ChatReply,
  ConfigUpdate,
  ConfigView,
  AgentFields,
  GraphFile,
  GraphLink,
  Harness,
  IdeAgent,
  IdeProject,
  WorkspaceGraph,
  InstalledAppsResult,
  LoadedDashboardState,
  LoadedTerminalSettings,
  MemoryStatus,
  NewRoutine,
  ProviderId,
  Routine,
  RunRecord,
  RunSummary,
  Skill,
  SpendSummary,
  SyncReport,
  AgentFileChange,
  AgentState,
  BaseFile,
  FileDiff,
} from "./backend";
import { hunksFromTexts, parseHunkHeader } from "../editor/diff/hunks";
import { textLines } from "../editor/diff/model";

const LATENCY_MS = 120;
const delay = () => new Promise((r) => setTimeout(r, LATENCY_MS));

let dashboardJson: string | null = null;
let terminalSettingsJson: string | null = null;

/** In-memory stand-in for `~/.axiomata/config.toml` — the *full* config
 *  including the raw keys, i.e. what lives on disk. `get_config` hands the
 *  webview a redacted `ConfigView` derived from this; `save_config` folds a
 *  `ConfigUpdate` back in, resolving each key per its `KeyUpdate`. */
interface MockProvider {
  base_url: string | null;
  api_key: string | null;
  chat_model: string;
  skill_model: string;
}
let configState: {
  owner: string;
  workspace_root: string;
  agents: {
    ollama_model: string;
    skill_timeout_secs: number;
    claude_env: Record<string, string>;
    chat_provider: ProviderId;
    skill_provider: ProviderId;
    providers: Record<ProviderId, MockProvider>;
    daily_usd_cap: number | null;
  };
} = {
  owner: "Dev",
  workspace_root: "/Users/dev/Axiomata-Workspace",
  agents: {
    ollama_model: "llama3.2",
    skill_timeout_secs: 300,
    claude_env: {},
    chat_provider: "anthropic",
    skill_provider: "anthropic",
    providers: {
      anthropic: { base_url: null, api_key: null, chat_model: "claude-sonnet-5", skill_model: "claude-haiku-4-5" },
      open_router: { base_url: "https://openrouter.ai/api", api_key: null, chat_model: "", skill_model: "" },
      ollama: { base_url: "http://localhost:11434", api_key: "ollama", chat_model: "", skill_model: "" },
    },
    daily_usd_cap: 2,
  },
};

/** Redacted view of `configState`, mirroring `ConfigView::from_config`. */
function configView(): ConfigView {
  const a = configState.agents;
  const providers = Object.fromEntries(
    (Object.entries(a.providers) as [ProviderId, MockProvider][]).map(([id, p]) => [
      id,
      { base_url: p.base_url, has_key: !!p.api_key?.trim(), chat_model: p.chat_model, skill_model: p.skill_model },
    ]),
  ) as ConfigView["agents"]["providers"];
  return {
    owner: configState.owner,
    workspace_root: configState.workspace_root,
    agents: {
      ollama_model: a.ollama_model,
      skill_timeout_secs: a.skill_timeout_secs,
      providers,
      chat_provider: a.chat_provider,
      skill_provider: a.skill_provider,
      daily_usd_cap: a.daily_usd_cap,
      claude_env_keys: Object.keys(a.claude_env),
    },
  };
}
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
    "Learning/Rust/snippets/hello.rs",
    'fn main() {\n    // a plain text / source file — read as monospace, editable\n    let name = "Rust";\n    println!("Hallo, {name}!");\n}\n',
  ],
  ["config/settings.json", '{\n  "theme": "graphite",\n  "autosave": true,\n  "recent": ["notes/inbox.md"]\n}\n'],
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
  { name: "example-skill", description: "Bundled example skill.", backend: "opencode" },
  { name: "sprint-planning", description: "Draft the next sprint plan.", backend: "opencode" },
  { name: "newsletter", description: "Summarise the week into a newsletter.", backend: "ollama" },
  { name: "calendar-digest", description: "Reads upcoming calendar events via whichever calendar MCP tool is available.", backend: "opencode" },
  { name: "reminders-digest", description: "Reads Apple Reminders lists and open tasks via whichever reminders MCP tool is available.", backend: "opencode" },
  { name: "mail-digest", description: "Reads recent mail via whichever mail MCP tool is available, picks out important and topic-matched messages, and summarises each.", backend: "opencode" },
];

/** Fixture digest, same shape `calendar-digest`'s SOP produces — invented
 *  events, not the owner's real calendar. Spread across ~6 weeks (a couple
 *  in the past, several this week, some next month) so the mini-month's
 *  dots + paging are visible in browser-only dev. */
const CALENDAR_DIGEST_JSON = (() => {
  // Local `YYYY-MM-DD` for `n` days from today, and a local `YYYY-MM-DD HH:mm`.
  const day = (n: number) => {
    const d = new Date();
    d.setDate(d.getDate() + n);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };
  const at = (n: number, hh: number, mm = 0) => `${day(n)} ${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
  const timed = (id: string, title: string, n: number, h: number, dur: number, calendar: string, location: string | null = null) => ({
    id,
    title,
    start: at(n, h),
    end: at(n, h + dur),
    calendar,
    location,
    allDay: false,
  });
  const allDay = (id: string, title: string, n: number, calendar: string, location: string | null = null) => ({
    id,
    title,
    start: day(n),
    end: day(n),
    calendar,
    location,
    allDay: true,
  });
  return JSON.stringify({
    calendars: ["Arbeit", "Privat", "Familie"],
    events: [
      timed("mock-evt-p1", "Rückblick Q3", -6, 10, 1, "Arbeit"),
      allDay("mock-evt-p2", "Urlaub Anna", -3, "Familie"),
      timed("mock-evt-1", "Team-Sync", 0, 9, 1, "Arbeit"),
      timed("mock-evt-2", "1:1 mit Sam", 0, 14, 1, "Arbeit"),
      allDay("mock-evt-3", "Zahnarzt", 2, "Privat", "Praxis Dr. Beispiel"),
      timed("mock-evt-4", "Sprint Planning", 3, 11, 2, "Arbeit"),
      allDay("mock-evt-5", "Geburtstag Mira", 5, "Familie"),
      timed("mock-evt-6", "Sport", 6, 18, 1, "Privat"),
      timed("mock-evt-7", "Kundentermin", 12, 10, 1, "Arbeit", "Vor Ort"),
      allDay("mock-evt-8", "Konferenz", 21, "Arbeit"),
      allDay("mock-evt-9", "Konferenz", 22, "Arbeit"),
      allDay("mock-evt-10", "Familientreffen", 34, "Familie"),
    ],
  });
})();

/** Fixture digest, same shape `reminders-digest`'s SOP produces — invented
 *  lists/tasks, not the owner's real reminders. */
const REMINDERS_DIGEST_JSON = JSON.stringify({
  lists: ["Einkaufen", "Werkstatt", "Geschenkideen"],
  tasks: [
    { id: "mock-task-1", title: "Milch kaufen", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
    { id: "mock-task-2", title: "Eier kaufen", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
    { id: "mock-task-3", title: "Rücklicht reparieren", list: "Werkstatt", notes: "Ersatzteil liegt in der Schublade", dueDate: new Date(Date.now() + 3 * 86_400_000).toISOString().slice(0, 10), priority: "medium" },
    { id: "mock-task-4", title: "Buch für Papa", list: "Geschenkideen", notes: null, dueDate: null, priority: "low" },
  ],
});

/** Fixture digest, same shape `mail-digest`'s SOP produces — invented mail,
 *  not the owner's real inbox. */
const MAIL_DIGEST_JSON = JSON.stringify({
  emails: [
    {
      id: "mock-mail-1",
      sender: "Chef",
      subject: "Bitte um Rückmeldung: Budget Q4",
      date: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      reason: "important",
      topic: null,
      summary: "Braucht bis Freitag eine Entscheidung zum Q4-Budget, sonst verschiebt sich die Planung um eine Woche.",
    },
    {
      id: "mock-mail-2",
      sender: "Foto-Newsletter",
      subject: "Neue Kamera-Tests: Drei Vollformatkameras im Vergleich",
      date: new Date(Date.now() - 20 * 3_600_000).toISOString(),
      reason: "topic",
      topic: "Fotografie",
      summary: "Vergleich dreier aktueller Vollformatkameras, Testsieger ist laut Artikel die Sony A7 wegen Autofokus und Akkulaufzeit.",
    },
    {
      id: "mock-mail-3",
      sender: "Dev Weekly",
      subject: "Was ist neu in Rust 1.90",
      date: new Date(Date.now() - 30 * 3_600_000).toISOString(),
      reason: "topic",
      topic: "Development",
      summary: "Überblick über die Neuerungen in Rust 1.90, unter anderem verbesserte Diagnosemeldungen und ein schnellerer Borrow-Checker.",
    },
  ],
});

let runs: RunRecord[] = [
  {
    id: 6,
    skill_name: "mail-digest",
    backend: "opencode",
    status: "success",
    exit_code: 0,
    duration_ms: 21400,
    stdout: MAIL_DIGEST_JSON,
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 4 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 4 * 60_000 + 21400).toISOString(),
    source: "manual",
  },
  {
    id: 5,
    skill_name: "reminders-digest",
    backend: "opencode",
    status: "success",
    exit_code: 0,
    duration_ms: 15800,
    stdout: REMINDERS_DIGEST_JSON,
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 6 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 6 * 60_000 + 15800).toISOString(),
    source: "manual",
  },
  {
    id: 4,
    skill_name: "calendar-digest",
    backend: "opencode",
    status: "success",
    exit_code: 0,
    duration_ms: 9200,
    stdout: CALENDAR_DIGEST_JSON,
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 10 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 10 * 60_000 + 9200).toISOString(),
    source: "manual",
  },
  {
    id: 3,
    skill_name: "example-skill",
    backend: "opencode",
    status: "success",
    exit_code: 0,
    duration_ms: 2310,
    stdout: "Working directory: /Users/dev/Axiomata-Workspace\nCLAUDE.md\nInbox\nLearning",
    stderr: "",
    error: null,
    started_at: new Date(Date.now() - 40 * 60_000).toISOString(),
    finished_at: new Date(Date.now() - 40 * 60_000 + 2310).toISOString(),
    source: "manual",
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
    source: "routine",
  },
];
/* IDE fixtures (M7.1). Two projects so the picker has something to switch
 * between, and the second one's folder is deliberately "missing" so the
 * marked-but-kept case is visible without unmounting a disk. `layout_json`
 * starts `null` on both: that is what makes the view build its starting
 * layout, and the mock then stores whatever the view saves, so switching back
 * and forth in a browser exercises the real round trip. */
let ideProjects: IdeProject[] = [
  {
    id: 1,
    name: "Axiomata-OS",
    repo_root: "/Users/dev/Development/Axiomata-OS",
    layout_json: null,
    created_at: new Date(Date.now() - 12 * 86_400_000).toISOString(),
    last_opened_at: new Date().toISOString(),
    root_exists: true,
  },
  {
    id: 2,
    name: "Auf dem Stick",
    repo_root: "/Volumes/Stick/experiment",
    layout_json: null,
    created_at: new Date(Date.now() - 30 * 86_400_000).toISOString(),
    last_opened_at: null,
    root_exists: false,
  },
];

/* Agent fixtures: two profiles on the first project, one per harness that
 * actually exists, so the picker and the agent pane both have something real
 * to show. `effective_command` is computed here exactly the way Rust computes
 * it, since that is the whole point of the field. */
let ideAgents: IdeAgent[] = [
  {
    id: 1,
    project_id: 1,
    name: "Builder",
    harness: "opencode",
    command: "",
    model: null,
    env: "",
    created_at: new Date(Date.now() - 5 * 86_400_000).toISOString(),
    updated_at: new Date(Date.now() - 5 * 86_400_000).toISOString(),
    effective_command: "opencode",
    worktree_path: null,
    branch: null,
    port: null,
    base_branch: null,
    opencode_session: null,
    effective_env: "AXIOMATA_AGENT_ID=1\nAXIOMATA_AGENT_NAME=Builder",
  },
  {
    id: 2,
    project_id: 1,
    name: "Reviewer",
    harness: "claude_code",
    command: "",
    model: "claude-sonnet-5",
    env: "REVIEW_MODE=strict",
    created_at: new Date(Date.now() - 2 * 86_400_000).toISOString(),
    updated_at: new Date(Date.now() - 2 * 86_400_000).toISOString(),
    effective_command: "claude --model 'claude-sonnet-5'",
    worktree_path: null,
    branch: null,
    port: null,
    base_branch: null,
    opencode_session: null,
    effective_env: "REVIEW_MODE=strict\nAXIOMATA_AGENT_ID=2\nAXIOMATA_AGENT_NAME=Reviewer",
  },
];

const HARNESS_DEFAULTS: Record<Harness, string> = {
  opencode: "opencode",
  claude_code: "claude",
  mini: "axiomata-miniagent",
};

/** Builds `effective_env` the way `Agent::resolve_env` does in Rust. */
function mockEffectiveEnv(
  fields: AgentFields,
  id: number,
  worktree: string | null,
  branch: string | null,
  port: number | null,
): string {
  const lines = fields.env.split("\n").filter((line) => line.trim());
  lines.push(`AXIOMATA_AGENT_ID=${id}`, `AXIOMATA_AGENT_NAME=${fields.name}`);
  if (worktree) lines.push(`AXIOMATA_WORKTREE=${worktree}`);
  if (branch) lines.push(`AXIOMATA_BRANCH=${branch}`);
  if (port !== null) lines.push(`AXIOMATA_PORT=${port}`);
  return lines.join("\n");
}

/** Resolves a command the same way `Agent::resolve_command` does in Rust. */
function mockEffectiveCommand(fields: AgentFields): string {
  const own = fields.command.trim();
  if (own) return own;
  const base = HARNESS_DEFAULTS[fields.harness];
  const model = fields.model?.trim();
  // Opencode 2's terminal UI takes no --model: the model rides on the session.
  if (!model || fields.harness === "opencode") return base;
  return `${base} --model '${model.replace(/'/g, "'\\''")}'`;
}

function mockAgent(id: number, projectId: number, fields: AgentFields): IdeAgent {
  const stamp = new Date().toISOString();
  return {
    id,
    project_id: projectId,
    name: fields.name,
    harness: fields.harness,
    command: fields.command,
    model: fields.model,
    env: fields.env,
    created_at: stamp,
    updated_at: stamp,
    worktree_path: null,
    branch: null,
    port: null,
    base_branch: null,
    opencode_session: null,
    effective_command: mockEffectiveCommand(fields),
    effective_env: mockEffectiveEnv(fields, id, null, null, null),
  };
}

/* Board fixtures. One board, the three default columns, a handful of cards
 * that exercise the cases the tile has to survive: an empty column, a long
 * body, labels, a due date already past, and an assignee that is an agent
 * rather than the default human (so the assignee line actually shows). */
const DAY = 86_400_000;
let boards: Board[] = [
  {
    id: 1,
    name: "Axiomata",
    created_at: new Date(Date.now() - 9 * DAY).toISOString(),
    updated_at: new Date().toISOString(),
  },
];
let boardColumns: BoardColumn[] = [
  { id: 1, board_id: 1, name: "Offen", position: 1, maps_to_status: "open" },
  { id: 2, board_id: 1, name: "In Arbeit", position: 2, maps_to_status: "doing" },
  { id: 3, board_id: 1, name: "Fertig", position: 3, maps_to_status: "done" },
];
function mockCard(card: Partial<BoardCard> & Pick<BoardCard, "id" | "column_id" | "position" | "title">): BoardCard {
  return {
    board_id: 1,
    body: "",
    labels: [],
    assignee: null,
    claimed_by: null,
    claimed_at: null,
    verified_by: null,
    verified_at: null,
    due_at: null,
    archived_at: null,
    created_at: new Date(Date.now() - 3 * DAY).toISOString(),
    updated_at: new Date().toISOString(),
    ...card,
  };
}
/** The cards of one column, in drawing order. */
function cardsIn(columnId: number): BoardCard[] {
  return boardCards
    .filter((c) => c.column_id === columnId)
    .sort((a, b) => a.position - b.position || a.id - b.id);
}

let boardCards: BoardCard[] = [
  mockCard({ id: 1, column_id: 1, position: 1, title: "Spaltenbreite auf 21:9 pruefen", labels: ["design"], due_at: new Date(Date.now() + 3 * DAY).toISOString() }),
  mockCard({ id: 2, column_id: 1, position: 2, title: "Vault-Spiegel: Dateiname festzurren", labels: ["kanban", "vault"] }),
  mockCard({
    id: 3,
    column_id: 1,
    position: 3,
    title: "Migration 0008 gegen bestehende DB testen",
    body: "Auf leerer und auf gewachsener Datenbank, plus der Test in db/mod.rs.",
    labels: ["rust"],
    due_at: new Date(Date.now() - 2 * DAY).toISOString(),
  }),
  mockCard({
    id: 4,
    column_id: 2,
    position: 1,
    title: "Drag-and-Drop ohne HTML5-API",
    body: "canvas/drag.ts als Plumbing, Momentaufnahme bei Drag-Beginn.",
    labels: ["frontend", "design"],
    assignee: "agent:claude-1",
    claimed_by: "agent:claude-1",
    claimed_at: new Date(Date.now() - DAY).toISOString(),
  }),
  mockCard({ id: 5, column_id: 3, position: 1, title: "WAL und busy_timeout entschieden", labels: ["rust"], claimed_by: "agent:claude-1", claimed_at: new Date(Date.now() - 2 * DAY).toISOString(), verified_by: "human:owner", verified_at: new Date(Date.now() - DAY).toISOString() }),
  mockCard({ id: 6, column_id: 3, position: 2, title: "Alte Notiz, archiviert", archived_at: new Date(Date.now() - 5 * DAY).toISOString() }),
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
    backend: "opencode",
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
    ["Rust/snippets/hello.rs", "hello.rs"],
  ] as const) {
    files.push({ path: `Learning/${rel}`, area: "Learning", title, bytes: 24_000, modified: new Date().toISOString(), is_markdown: rel.endsWith(".md") });
  }
  files.push({ path: "config/settings.json", area: "config", title: "settings.json", bytes: 90, modified: new Date().toISOString(), is_markdown: false });
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

/** `editor-settings.json` in the mock. */
let editorSettingsJson = '{"version":1}';
let editorViJson = '{"version":1}';

/** Handlers registered through `listenBackend` in the browser. */
const mockListeners = new Map<string, Set<(payload: unknown) => void>>();

/** The mock side of `listenBackend`. */
export function mockListen(event: string, handler: (payload: unknown) => void): () => void {
  const set = mockListeners.get(event) ?? new Set();
  set.add(handler);
  mockListeners.set(event, set);
  return () => set.delete(handler);
}

/** Raises a backend event in the browser (console: `__ax.mockEmit(...)`). */
export function mockEmit(event: string, payload: unknown): void {
  for (const handler of mockListeners.get(event) ?? []) handler(payload);
}

/**
 * Changes a mock file "from outside" and raises `files:changed`, the way an
 * agent writing into an open file would (console: `__ax.mockExternalWrite`).
 */
export function mockExternalWrite(root: string, rel: string, content: string | null): void {
  const store = fileStore(root);
  const key = fileKey(root, rel);
  const existed = store.has(key);
  if (content === null) store.delete(key);
  else store.set(key, content);
  mockEmit("files:changed", {
    root,
    rel,
    kind: content === null ? "deleted" : existed ? "modified" : "created",
    version: content === null ? null : mockVersion(content),
  });
}

/** The Mac clipboard of the mock (`clipboard_read` / `clipboard_write`). */
let mockClipboard = "";

/** Unsaved editor work in the mock, keyed by `<root>\0<rel>`. */
const recoveries = new Map<string, unknown>();

/** Files of the non-workspace roots in the file-service mock. */
const otherRootFiles = new Map<string, string>([
  ["project:1\0README.md", "# Axiomata-OS\n\nSee `src/main.rs`.\n\n```rust\nfn main() {}\n```\n"],
  [
    "project:1\0src/main.rs",
    [
      "//! A sample for the editor's highlighting.",
      "use std::collections::HashMap;",
      "",
      "/// Counts words.",
      "pub fn count(text: &str) -> HashMap<&str, usize> {",
      "    let mut seen = HashMap::new();",
      "    for word in text.split_whitespace() {",
      "        *seen.entry(word).or_insert(0) += 1; // tally",
      "    }",
      "    seen",
      "}",
      "",
      "fn main() {",
      '    let n: u32 = 42;',
      '    println!("{n} words: {:?}", count("a b a"));',
      "}",
      "",
    ].join("\n"),
  ],
  [
    "project:1\0src/App.svelte",
    [
      '<script lang="ts">',
      "  let count = $state(0);",
      "  const double = $derived(count * 2);",
      "</script>",
      "",
      '<button class="pill" onclick={() => count++}>',
      "  {count} × 2 = {double}",
      "</button>",
      "",
      "<style>",
      "  .pill { color: var(--ax-accent); }",
      "</style>",
      "",
    ].join("\n"),
  ],
]);

// ---- agent 1's worktree (M7.3 CP8) ----
// The mock has no git: agent 1's "base" is simply project 1's files as they
// are, and its worktree is a second set under `worktree:1` that differs from it
// in every way the Diffs tab has to show — a long file changed in two far-apart
// places (folds), a small edit, a new file, a deleted one, and a picture.

/** A long Rust file, so the diff has gaps worth folding. */
function demoLines(changed: boolean): string {
  const out = ["//! The diff view's demo file.", "", "use std::fmt;", ""];
  for (let i = 1; i <= 30; i++) {
    out.push(`/// Step ${i}.`, `pub fn step_${i}(x: u32) -> u32 {`);
    if (changed && i === 4) out.push("    // Doubled since the agent's first pass.", `    x * ${i} * 2`);
    else if (changed && i === 27) out.push(`    x.saturating_mul(${i})`);
    else out.push(`    x * ${i}`);
    out.push("}", "");
  }
  return out.join("\n");
}

otherRootFiles.set("project:1\0src/demo.rs", demoLines(false));
otherRootFiles.set("worktree:1\0src/demo.rs", demoLines(true));
otherRootFiles.set(
  "worktree:1\0src/main.rs",
  (otherRootFiles.get("project:1\0src/main.rs") ?? "").replace("let n: u32 = 42;", "let n: u32 = 7; // fewer"),
);
otherRootFiles.set("worktree:1\0src/App.svelte", otherRootFiles.get("project:1\0src/App.svelte") ?? "");
otherRootFiles.set(
  "worktree:1\0src/words.rs",
  [
    "/// Splits text into words.",
    "pub fn words(text: &str) -> Vec<&str> {",
    "    text.split_whitespace().collect()",
    "}",
    "",
  ].join("\n"),
);

/** 24×24 squares: the base's orange logo and the agent's green one. */
const MOCK_IMAGES = {
  base:
    "iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR4nGP4XyVFFcQwatCoQaMGjRo0atCoQQNvEACJB4runjWXOAAAAABJ" +
    "RU5ErkJggg==",
  worktree:
    "iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR4nGPwv1ZPFcQwatCoQaMGjRo0atCoQQNvEAAEVrEuaRoBFQAAAABJ" +
    "RU5ErkJggg==",
};
const MOCK_IMAGE_PATH = "assets/logo.png";
/** Changes the mock pretends are not committed yet. */
const MOCK_UNCOMMITTED = new Set(["src/words.rs", "src/main.rs"]);
/** The picture's change was discarded or taken over. */
let mockImageSettled = false;

/** Status overrides by agent id (console: `__ax.mockAgentState(1, "idle")`). */
const mockStates = new Map<number, AgentState>();

export function mockAgentState(id: number, state: AgentState): void {
  mockStates.set(id, state);
}

/** Puts `path` in agent 1's worktree back to the base (G13). */
function mockDiscard(path: string): void {
  if (path === MOCK_IMAGE_PATH) {
    mockImageSettled = true;
    return;
  }
  // Through `mockExternalWrite`, so an open file pane hears about it as it would from the watcher.
  mockExternalWrite("worktree:1", path, otherRootFiles.get(`project:1\0${path}`) ?? null);
  MOCK_UNCOMMITTED.add(path);
}

/** Puts one hunk of agent 1's `path` back (H6), refusing a stale header like Rust does. */
function mockDiscardHunk(path: string, index: number, header: string): void {
  const hunk = mockFileDiff(path).hunks[index];
  if (!hunk || hunk.header !== header) throw new Error(`${path} changed since its diff was shown; reload it and try again`);
  const range = parseHunkHeader(header);
  const text = otherRootFiles.get(`worktree:1\0${path}`);
  if (!range || text === undefined || range.oldCount === 0 || range.newCount === 0) {
    mockDiscard(path);
    return;
  }
  const lines = textLines(text);
  const before = hunk.lines.filter((l) => l.kind !== "add" && l.kind !== "no_newline").map((l) => l.text);
  lines.splice(range.newStart - 1, range.newCount, ...before);
  mockExternalWrite("worktree:1", path, `${lines.join("\n")}\n`);
  MOCK_UNCOMMITTED.add(path);
}

/** Text files of one root, by relative path. */
function rootFiles(root: string): Map<string, string> {
  const prefix = `${root}\0`;
  const out = new Map<string, string>();
  for (const [key, content] of otherRootFiles) if (key.startsWith(prefix)) out.set(key.slice(prefix.length), content);
  return out;
}

/** Agent 1's changes against project 1, the way `changes()` reports them. */
function mockAgentChanges(): AgentFileChange[] {
  const base = rootFiles("project:1");
  const work = rootFiles("worktree:1");
  const out: AgentFileChange[] = [];
  for (const path of new Set([...base.keys(), ...work.keys()])) {
    const before = base.get(path);
    const after = work.get(path);
    if (before === after) continue;
    const hunks = hunksFromTexts(textLines(before ?? ""), textLines(after ?? ""));
    const count = (kind: string) => hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === kind).length, 0);
    out.push({
      path,
      old_path: null,
      kind: before === undefined ? "added" : after === undefined ? "deleted" : "modified",
      additions: after === undefined ? 0 : before === undefined ? textLines(after).length : count("add"),
      deletions: before === undefined ? 0 : after === undefined ? textLines(before).length : count("remove"),
      binary: false,
      uncommitted: MOCK_UNCOMMITTED.has(path),
    });
  }
  if (!mockImageSettled) {
    out.push({
      path: MOCK_IMAGE_PATH,
      old_path: null,
      kind: "modified",
      additions: null,
      deletions: null,
      binary: true,
      uncommitted: false,
    });
  }
  return out.sort((a, b) => a.path.localeCompare(b.path));
}

/** One file's diff in the wire shape (`FileDiff`), git's line numbers and all. */
function mockFileDiff(path: string): FileDiff {
  if (path === MOCK_IMAGE_PATH) {
    return { path, binary: true, hunks: [], truncated: false, old_size: 94, new_size: 94 };
  }
  const before = rootFiles("project:1").get(path);
  const after = rootFiles("worktree:1").get(path);
  const lines = (text: string | undefined) => (text === undefined ? [] : textLines(text));
  const hunks = hunksFromTexts(lines(before), lines(after));
  return {
    path,
    binary: false,
    truncated: false,
    old_size: before?.length ?? null,
    new_size: after?.length ?? null,
    hunks: hunks.map((h) => ({
      header: h.header,
      lines: h.lines.map((l) => ({ kind: l.kind, old_line: l.oldLine, new_line: l.newLine, text: l.text })),
    })),
  };
}

function mockBaseFile(path: string): BaseFile {
  if (path === MOCK_IMAGE_PATH) return { kind: "image", mime: "image/png", base64: MOCK_IMAGES.base };
  const text = rootFiles("project:1").get(path);
  return text === undefined ? { kind: "absent" } : { kind: "text", text };
}

/** The file the next mock `file_pick` returns (console: `__ax.mockPickNext`). */
let nextPick: { root: string; rel: string } | null = null;

export function mockPickNext(root: string, rel: string): void {
  nextPick = { root, rel };
}

function fileArgs(args: Record<string, unknown>): { root: string; rel: string } {
  const root = String(args.root);
  const rel = String(args.rel);
  if (root !== "workspace" && !/^(project|worktree|grant):/.test(root)) {
    throw fileError("UnknownRoot", `unknown file root \`${root}\``);
  }
  if (rel.trim() === "" || rel.startsWith("/") || rel.split("/").includes("..")) {
    throw fileError("Refused", `refused ${rel}: resolves outside the root`);
  }
  return { root, rel };
}

function fileStore(root: string): Map<string, string> {
  return root === "workspace" ? files : otherRootFiles;
}

function fileKey(root: string, rel: string): string {
  return root === "workspace" ? rel : `${root}\0${rel}`;
}

/** Folders made with `file_mkdir` that hold no file yet (the mock's files make their folders implicitly). */
const mockDirs = new Set<string>();

/** Every file key of `root`, as `rel` paths. */
function relsOf(root: string): string[] {
  const prefix = root === "workspace" ? "" : `${root}\0`;
  const out: string[] = [];
  for (const key of fileStore(root).keys()) {
    if (root === "workspace" ? !key.includes("\0") : key.startsWith(prefix)) out.push(key.slice(prefix.length));
  }
  for (const key of mockDirs) if (key.startsWith(`${root}\0`)) out.push(`${key.slice(root.length + 1)}/`);
  return out;
}

/** The mock's `file_list`: the direct children of `rel`, folders first; `target`/`node_modules` count as ignored. */
function mockListing(root: string, rel: string) {
  const base = rel ? `${rel.replace(/\/+$/, "")}/` : "";
  const children = new Map<string, "file" | "dir">();
  for (const path of relsOf(root)) {
    if (!path.startsWith(base) || path === base) continue;
    const rest = path.slice(base.length);
    const slash = rest.indexOf("/");
    const name = slash < 0 ? rest : rest.slice(0, slash);
    if (name) children.set(name, slash < 0 ? (children.get(name) ?? "file") : "dir");
  }
  if (base && children.size === 0 && !mockDirs.has(`${root}\0${rel}`)) throw fileError("NotFound", `${rel} does not exist`);
  const entries = [...children].map(([name, kind]) => ({
    name,
    kind,
    size: kind === "file" ? (fileStore(root).get(fileKey(root, base + name))?.length ?? 0) : null,
    ignored: root !== "workspace" && /^(target|node_modules)$/.test(name),
  }));
  entries.sort((a, b) => (a.kind === b.kind ? a.name.toLowerCase().localeCompare(b.name.toLowerCase()) : a.kind === "dir" ? -1 : 1));
  return { entries, truncated: false };
}

/** Moves (or with `to` null, deletes) `rel` and everything under it; the number of entries affected. */
function mockMoveTree(root: string, rel: string, to: string | null): number {
  const store = fileStore(root);
  let n = 0;
  for (const path of relsOf(root)) {
    if (path !== rel && !path.startsWith(`${rel}/`)) continue;
    n++;
    if (path.endsWith("/")) {
      mockDirs.delete(`${root}\0${path.slice(0, -1)}`);
      if (to !== null) mockDirs.add(`${root}\0${to}${path.slice(rel.length, -1)}`);
      continue;
    }
    const content = store.get(fileKey(root, path)) ?? "";
    store.delete(fileKey(root, path));
    if (to !== null) store.set(fileKey(root, to + path.slice(rel.length)), content);
  }
  return n;
}

function fileError(kind: string, message: string): { kind: string; message: string } {
  return { kind, message };
}

/** Same shape as the real `Version` (`<len>-<hash>`); the hash is not FNV,
 *  it only has to change with the content. */
function mockVersion(content: string): string {
  let h = 0;
  for (const ch of content) h = (Math.imul(h, 31) + ch.charCodeAt(0)) >>> 0;
  return `${new TextEncoder().encode(content).length}-${h.toString(16).padStart(16, "0")}`;
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
    case "get_config":
      return configView() as T;
    case "get_spend_summary": {
      const { chat_provider: chat, skill_provider: skill } = configState.agents;
      const summaryFor = (p: ProviderId, role: string): SpendSummary => ({
        provider: p,
        role,
        today_usd: p === "anthropic" ? 0 : 0.42,
        month_usd: p === "anthropic" ? 0 : 7.13,
        daily_cap_usd: configState.agents.daily_usd_cap,
        metered: p !== "anthropic",
      });
      const summaries =
        chat === skill
          ? [summaryFor(chat, "chat & skill")]
          : [summaryFor(chat, "chat"), summaryFor(skill, "skill")];
      return summaries as T;
    }
    case "save_config": {
      const next = args.newConfig as ConfigUpdate;
      const workspaceChanged = next.workspace_root !== configState.workspace_root;
      const a = configState.agents;
      a.ollama_model = next.agents.ollama_model;
      a.skill_timeout_secs = next.agents.skill_timeout_secs;
      a.chat_provider = next.agents.chat_provider;
      a.skill_provider = next.agents.skill_provider;
      a.daily_usd_cap = next.agents.daily_usd_cap;
      for (const [id, u] of Object.entries(next.agents.providers) as [ProviderId, ConfigUpdate["agents"]["providers"][ProviderId]][]) {
        const stored = a.providers[id]?.api_key ?? null;
        a.providers[id] = {
          base_url: u.base_url,
          api_key: u.api_key.kind === "keep" ? stored : u.api_key.kind === "set" ? u.api_key.value : null,
          chat_model: u.chat_model,
          skill_model: u.skill_model,
        };
      }
      configState.owner = next.owner;
      // Mirror the backend: the new root is "written to disk" but the live
      // copy keeps the old one until a restart.
      if (!workspaceChanged) configState.workspace_root = next.workspace_root;
      return workspaceChanged as T;
    }
    case "get_dashboard_state":
      return {
        json: dashboardJson ?? '{"version":1,"settings":{"theme":"graphite"},"canvas":{"instances":[]}}',
        recovered_backup: null,
      } satisfies LoadedDashboardState as T;
    case "save_dashboard_state":
      dashboardJson = String(args.json);
      return undefined as T;
    case "get_editor_settings":
      return { json: editorSettingsJson, recovered_backup: null } as T;
    case "save_editor_settings":
      editorSettingsJson = String(args.json);
      return undefined as T;
    case "get_editor_vi_state":
      return { json: editorViJson, recovered_backup: null } as T;
    case "save_editor_vi_state":
      editorViJson = String(args.json);
      return undefined as T;
    case "get_terminal_settings":
      return {
        json: terminalSettingsJson ?? '{"version":1}',
        recovered_backup: null,
      } satisfies LoadedTerminalSettings as T;
    case "save_terminal_settings":
      terminalSettingsJson = String(args.json);
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
    case "list_skipped_skills":
      // The mock fixture never has a broken skill; keeps the Skills Deck's
      // warning row untriggered in dev-mock runs the same way it is in a
      // real, healthy `~/.axiomata/skills/`.
      return [] as T;
    case "run_skill": {
      const name = String(args.name);
      const startedAt = new Date();
      const durationMs = 1500;
      const run: RunRecord = {
        id: (runs[0]?.id ?? 0) + 1,
        skill_name: name,
        backend: skills.find((s) => s.name === name)?.backend ?? "opencode",
        status: "success",
        exit_code: 0,
        duration_ms: durationMs,
        stdout:
          name === "calendar-digest"
            ? CALENDAR_DIGEST_JSON
            : name === "reminders-digest"
              ? REMINDERS_DIGEST_JSON
              : name === "mail-digest"
                ? MAIL_DIGEST_JSON
                : `Ran ${name}.`,
        stderr: "",
        error: null,
        started_at: startedAt.toISOString(),
        finished_at: new Date(startedAt.getTime() + durationMs).toISOString(),
        source: "manual",
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
    // Argument keys are camelCase here because that is what the frontend
    // sends: Tauri converts them to snake_case on the Rust side, devmock
    // sees them unconverted.
    // ---- ide projects ----
    case "list_ide_projects":
      // Same order the store gives: most recently opened first, never-opened last.
      return [...ideProjects].sort((a, b) => {
        if (!a.last_opened_at && !b.last_opened_at) return a.name.localeCompare(b.name);
        if (!a.last_opened_at) return 1;
        if (!b.last_opened_at) return -1;
        return b.last_opened_at.localeCompare(a.last_opened_at);
      }) as T;
    case "create_ide_project": {
      const repoRoot = String(args.repoRoot);
      if (ideProjects.some((p) => p.repo_root === repoRoot)) {
        // The real store's UNIQUE constraint names who is already there.
        const taken = ideProjects.find((p) => p.repo_root === repoRoot)!;
        throw new Error(`that folder is already project "${taken.name}"`);
      }
      const created: IdeProject = {
        id: (ideProjects[ideProjects.length - 1]?.id ?? 0) + 1,
        name: String(args.name),
        repo_root: repoRoot,
        layout_json: null,
        created_at: new Date().toISOString(),
        last_opened_at: null,
        root_exists: true,
      };
      ideProjects = [...ideProjects, created];
      return created as T;
    }
    case "rename_ide_project": {
      const project = ideProjects.find((p) => p.id === args.id);
      if (project) project.name = String(args.name);
      return (project ?? null) as T;
    }
    case "set_ide_project_root": {
      const project = ideProjects.find((p) => p.id === args.id);
      if (project) {
        project.repo_root = String(args.repoRoot);
        project.root_exists = true;
      }
      return (project ?? null) as T;
    }
    case "set_ide_project_layout": {
      const project = ideProjects.find((p) => p.id === args.id);
      if (project) project.layout_json = args.layout === null ? null : String(args.layout);
      return (project !== undefined) as T;
    }
    case "open_ide_project": {
      const project = ideProjects.find((p) => p.id === args.id);
      if (project) project.last_opened_at = new Date().toISOString();
      return (project ?? null) as T;
    }
    case "delete_ide_project": {
      const before = ideProjects.length;
      ideProjects = ideProjects.filter((p) => p.id !== args.id);
      return (ideProjects.length < before) as T;
    }

    // ---- ide agents ----
    case "list_ide_agents":
      return ideAgents
        .filter((a) => a.project_id === args.projectId)
        .sort((a, b) => a.name.localeCompare(b.name)) as T;
    case "create_ide_agent": {
      const fields = args.fields as AgentFields;
      const projectId = Number(args.projectId);
      if (
        ideAgents.some(
          (a) => a.project_id === projectId && a.name.toLowerCase() === fields.name.trim().toLowerCase(),
        )
      ) {
        // The real store's UNIQUE(project_id, name COLLATE NOCASE).
        throw new Error(`this project already has an agent called "${fields.name.trim()}"`);
      }
      const created = mockAgent((ideAgents[ideAgents.length - 1]?.id ?? 0) + 1, projectId, {
        ...fields,
        name: fields.name.trim(),
      });
      ideAgents = [...ideAgents, created];
      return created as T;
    }
    case "update_ide_agent": {
      const index = ideAgents.findIndex((a) => a.id === args.id);
      if (index === -1) return null as T;
      const fields = args.fields as AgentFields;
      const replaced = {
        ...mockAgent(ideAgents[index].id, ideAgents[index].project_id, { ...fields, name: fields.name.trim() }),
        created_at: ideAgents[index].created_at,
      };
      ideAgents[index] = replaced;
      return replaced as T;
    }
    case "delete_ide_agent": {
      const before = ideAgents.length;
      ideAgents = ideAgents.filter((a) => a.id !== args.id);
      return (ideAgents.length < before) as T;
    }

    // The mock has no git, so it answers the way a non-repository project
    // does: agents share the project folder and still get a port.
    case "ide_agent_new_session": {
      const agent = ideAgents.find((a) => a.id === args.id);
      if (agent) agent.opencode_session = null;
      return Boolean(agent) as T;
    }
    case "prepare_ide_agent": {
      const agent = ideAgents.find((a) => a.id === args.id);
      if (!agent) throw new Error(`no agent ${args.id}`);
      const project = ideProjects.find((p) => p.id === agent.project_id);
      const port = agent.port ?? 4300 + ideAgents.indexOf(agent);
      agent.port = port;
      agent.effective_env = `${agent.effective_env}\nAXIOMATA_PORT=${port}`;
      const events = `/mock/.axiomata/agent-events/${agent.id}`;
      const generated = !agent.command.trim();
      if (agent.harness === "opencode" && generated && !agent.opencode_session) {
        agent.opencode_session = `ses_mock${agent.id}`;
      }
      const hookup = agent.harness === "claude_code" && generated
        ? ` --settings '${events}/claude-settings.json'`
        : agent.harness === "opencode" && generated
          ? ` --session ${agent.opencode_session}`
          : "";
      return {
        agent,
        cwd: project?.repo_root ?? "/",
        shared_folder: true,
        launch_command: `${agent.effective_command}${hookup}`,
        launch_env: `${agent.effective_env}\nAXIOMATA_EVENTS=${events}\nAXIOMATA_CLAUDE_SETTINGS=${events}/claude-settings.json`,
        status_connected: agent.harness !== "mini",
      } as T;
    }
    // One status per agent: the first fixture is mid-plan, the second waits
    // for a permission, anything created in the session is idle — enough for
    // all three places the dot shows up, and for the plan tab.
    case "ide_agent_states": {
      const now = new Date().toISOString();
      return ideAgents
        .filter((a) => a.project_id === args.projectId)
        .map((agent) => ({
          agent_id: agent.id,
          state: mockStates.get(agent.id) ?? (agent.id === 1 ? "working" : agent.id === 2 ? "waiting" : "idle"),
          since: now,
          started_at: now,
          plan:
            agent.id === 1
              ? {
                  steps: [
                    { text: "Read the dock layout code", state: "done", detail: null },
                    { text: "Add the status dot to agent tabs", state: "doing", detail: null },
                    { text: "Write the poller test", state: "todo", detail: null },
                    { text: "Try a websocket push instead", state: "cancelled", detail: null },
                  ],
                  updated_at: now,
                  from_earlier_session: false,
                }
              : null,
          plan_document:
            agent.id === 2
              ? {
                  markdown:
                    "# Review the status channel\n\n1. Read `lifecycle.rs`\n2. Check the hook quoting\n3. Report findings",
                  name: "gentle-walrus",
                  updated_at: now,
                  from_earlier_session: false,
                }
              : null,
        })) as T;
    }
    // Agent 1 has a worktree with changes, agent 2 shares the folder, every
    // other agent has not been started yet — the three states of the tab.
    case "ide_agent_changes":
      if (args.id === 1) {
        return {
          state: "ready",
          changes: {
            base: { branch: "main", commit: "4f9c2d1e8b7a6c5d4e3f2a1b0c9d8e7f6a5b4c3d", fallback: false },
            files: mockAgentChanges(),
          },
        } as T;
      }
      return { state: args.id === 2 ? "shared_folder" : "not_started" } as T;
    case "ide_agent_file_diff":
      return mockFileDiff(String(args.path)) as T;
    case "ide_agent_base_file":
      return mockBaseFile(String(args.path)) as T;
    case "ide_agent_discard":
      for (const path of args.paths as string[]) mockDiscard(path);
      return undefined as T;
    case "ide_agent_discard_hunk":
      mockDiscardHunk(String(args.path), Number(args.index), String(args.header));
      return undefined as T;
    case "ide_agent_commit":
      if (MOCK_UNCOMMITTED.size === 0) throw new Error("there is nothing uncommitted to commit");
      MOCK_UNCOMMITTED.clear();
      return "c0ffee1d2e3f" as T;
    case "ide_agent_last_subject":
      return "Double step 4 and saturate step 27" as T;
    // Taking over lands the agent's files on the base: its diff is then empty.
    case "ide_agent_take_over": {
      const state = mockStates.get(Number(args.id)) ?? (args.id === 1 ? "working" : "idle");
      if (state === "working" || state === "waiting") {
        throw new Error(`Builder is ${state} — wait until it has finished before taking its work over`);
      }
      if (MOCK_UNCOMMITTED.size > 0) throw new Error("the agent has uncommitted work; commit it first");
      for (const key of [...otherRootFiles.keys()].filter((k) => k.startsWith("project:1\0"))) otherRootFiles.delete(key);
      for (const [key, content] of [...otherRootFiles]) {
        if (key.startsWith("worktree:1\0")) otherRootFiles.set(key.replace("worktree:1", "project:1"), content);
      }
      mockImageSettled = true;
      return { outcome: "done", commit: "facade1234567" } as T;
    }
    case "ide_agent_has_changes":
      return false as T;
    case "discard_ide_agent_worktree":
      return false as T;

    case "list_boards":
      return boards as T;
    case "get_board":
      return (boards.find((b) => b.id === args.id) ?? null) as T;
    case "create_board": {
      const created: Board = {
        id: (boards[boards.length - 1]?.id ?? 0) + 1,
        name: String(args.name),
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      boards = [...boards, created];
      // A board is never without columns, not even briefly — same rule the
      // real store enforces in one transaction.
      const base = (boardColumns[boardColumns.length - 1]?.id ?? 0) + 1;
      boardColumns = [
        ...boardColumns,
        { id: base, board_id: created.id, name: "Offen", position: 1, maps_to_status: "open" },
        { id: base + 1, board_id: created.id, name: "In Arbeit", position: 2, maps_to_status: "doing" },
        { id: base + 2, board_id: created.id, name: "Fertig", position: 3, maps_to_status: "done" },
      ];
      return created as T;
    }
    case "rename_board": {
      const target = boards.find((b) => b.id === args.id);
      if (!target) return null as T;
      const renamed: Board = { ...target, name: String(args.name), updated_at: new Date().toISOString() };
      boards = boards.map((b) => (b.id === renamed.id ? renamed : b));
      return renamed as T;
    }
    case "delete_board": {
      const before = boards.length;
      boards = boards.filter((b) => b.id !== args.id);
      boardColumns = boardColumns.filter((c) => c.board_id !== args.id);
      boardCards = boardCards.filter((c) => c.board_id !== args.id);
      return (boards.length < before) as T;
    }
    case "count_board_cards":
      return boardCards.filter((c) => c.board_id === args.boardId).length as T;
    case "list_board_columns":
      return boardColumns.filter((c) => c.board_id === args.boardId) as T;
    case "list_board_cards":
      return boardCards.filter(
        (c) => c.board_id === args.boardId && (args.includeArchived === true || c.archived_at === null),
      ) as T;
    case "create_card": {
      const n = args.new as NewCard;
      const column = boardColumns.find((c) => c.id === n.column_id);
      if (!column) throw new Error(`invalid column_id: no column ${n.column_id}`);
      const created = mockCard({
        id: Math.max(0, ...boardCards.map((c) => c.id)) + 1,
        board_id: column.board_id,
        column_id: n.column_id,
        position: Math.max(0, ...cardsIn(n.column_id).map((c) => c.position)) + 1,
        title: n.title,
        body: n.body,
        labels: n.labels,
        assignee: n.assignee,
        due_at: n.due_at,
      });
      boardCards = [...boardCards, created];
      return created as T;
    }
    case "update_card": {
      const f = args.fields as CardFields;
      const target = boardCards.find((c) => c.id === args.id);
      if (!target) return null as T;
      const updated: BoardCard = { ...target, ...f, updated_at: new Date().toISOString() };
      boardCards = boardCards.map((c) => (c.id === updated.id ? updated : c));
      return updated as T;
    }
    case "move_card": {
      const moved = boardCards.find((c) => c.id === args.id);
      const column = boardColumns.find((c) => c.id === args.columnId);
      if (!moved || !column) return false as T;
      // Mirrors the store: the index counts the cards the moved one will sit
      // among, so it is excluded from its own neighbour list.
      const others = cardsIn(column.id).filter((c) => c.id !== moved.id);
      const index = Math.min(Number(args.index ?? others.length), others.length);
      const before = others[index - 1]?.position;
      const after = others[index]?.position;
      const position =
        before === undefined && after === undefined
          ? 1
          : before === undefined
            ? after! - 1
            : after === undefined
              ? before + 1
              : (before + after) / 2;
      const next: BoardCard = {
        ...moved,
        column_id: column.id,
        board_id: column.board_id,
        position,
        updated_at: new Date().toISOString(),
        // Same rule as the store: no longer done, no longer signed off.
        ...(column.maps_to_status === "done" ? {} : { verified_by: null, verified_at: null }),
      };
      boardCards = boardCards.map((c) => (c.id === next.id ? next : c));
      return true as T;
    }
    case "delete_card": {
      const before = boardCards.length;
      boardCards = boardCards.filter((c) => c.id !== args.id);
      return (boardCards.length < before) as T;
    }
    case "set_card_archived": {
      const target = boardCards.find((c) => c.id === args.id);
      if (!target) return false as T;
      const stamp = new Date().toISOString();
      boardCards = boardCards.map((c) =>
        c.id === target.id ? { ...c, archived_at: args.archived ? stamp : null, updated_at: stamp } : c,
      );
      return true as T;
    }
    case "create_board_column": {
      const n = args.new as NewColumn;
      const created: BoardColumn = {
        id: Math.max(0, ...boardColumns.map((c) => c.id)) + 1,
        board_id: Number(args.boardId),
        name: n.name,
        position: Math.max(0, ...boardColumns.filter((c) => c.board_id === args.boardId).map((c) => c.position)) + 1,
        maps_to_status: n.maps_to_status,
      };
      boardColumns = [...boardColumns, created];
      return created as T;
    }
    case "update_board_column": {
      const target = boardColumns.find((c) => c.id === args.id);
      if (!target) return null as T;
      const status = args.mapsToStatus as BoardColumn["maps_to_status"];
      const updated: BoardColumn = { ...target, name: String(args.name), maps_to_status: status };
      boardColumns = boardColumns.map((c) => (c.id === updated.id ? updated : c));
      if (status !== "done") {
        boardCards = boardCards.map((c) =>
          c.column_id === updated.id ? { ...c, verified_by: null, verified_at: null } : c,
        );
      }
      return updated as T;
    }
    case "delete_board_column": {
      const held = cardsIn(Number(args.id));
      if (held.length > 0) {
        if (args.moveCardsTo === undefined || args.moveCardsTo === null) return false as T;
        const target = Number(args.moveCardsTo);
        boardCards = boardCards.map((c) => (c.column_id === args.id ? { ...c, column_id: target } : c));
      }
      const before = boardColumns.length;
      boardColumns = boardColumns.filter((c) => c.id !== args.id);
      return (boardColumns.length < before) as T;
    }
    case "move_board_column": {
      const moved = boardColumns.find((c) => c.id === args.id);
      if (!moved) return false as T;
      const others = boardColumns
        .filter((c) => c.board_id === moved.board_id && c.id !== moved.id)
        .sort((a, b) => a.position - b.position || a.id - b.id);
      const index = Math.min(Number(args.index ?? others.length), others.length);
      const before = others[index - 1]?.position;
      const after = others[index]?.position;
      const position =
        before === undefined && after === undefined
          ? 1
          : before === undefined
            ? after! - 1
            : after === undefined
              ? before + 1
              : (before + after) / 2;
      boardColumns = boardColumns.map((c) => (c.id === moved.id ? { ...c, position } : c));
      return true as T;
    }
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
    case "update_routine": {
      const id = Number(args.id);
      const n = args.new as NewRoutine;
      let updated: Routine | null = null;
      routines = routines.map((r) => {
        if (r.id !== id) return r;
        updated = { ...r, ...n, next_fire_at: n.enabled ? new Date(Date.now() + 3_600_000).toISOString() : r.next_fire_at };
        return updated;
      });
      return updated as T;
    }
    case "delete_routine": {
      const id = Number(args.id);
      const before = routines.length;
      routines = routines.filter((r) => r.id !== id);
      return (routines.length < before) as T;
    }
    case "assistant_send": {
      await new Promise((r) => setTimeout(r, 900));
      const message = String(args.message);
      const mode = String(args.mode);
      const session_id = typeof args.sessionId === "string" ? args.sessionId : `mock-${Date.now()}`;
      // A connector module's write instruction (core/instruct.ts's
      // buildToolCallInstruction) always ends in one of these two exact
      // phrases — answer them the way the real agent would, so the browser
      // mock can exercise a whole create/complete/delete round trip.
      const reply =
        mode !== "instruct"
          ? `You said: **${message}**\n\nThis is a mocked reply in session \`${session_id}\`.\n\n- markdown renders\n- \`code\` too`
          : message.endsWith("reply with exactly the created item's id and nothing else.")
            ? `mock-${Date.now()}`
            : message.endsWith("reply with exactly OK and nothing else.")
              ? "OK"
              : `Done (mock). I would have carried out:\n\n> ${message}\n\n_No files were touched in the browser mock._`;
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
    case "read_workspace_image": {
      // No real binary file store in this mock — any supported extension
      // resolves to the same tiny fixture image, so the Markdown viewer's
      // image-inlining path is exercisable in a browser without a real
      // workspace file on disk.
      const rel = String(args.rel);
      const ext = rel.split(".").pop()?.toLowerCase();
      const mime = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp" }[ext ?? ""];
      if (!mime) throw new Error(`invalid workspace image ${rel}: not a supported image type (png/jpeg/gif/webp)`);
      // A 1×1 transparent PNG, base64-encoded — a real, valid (if trivial)
      // image regardless of which raster `mime` was actually requested.
      return {
        path: rel,
        mime,
        base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
      } as T;
    }
    case "write_workspace_file":
      files.set(String(args.rel), String(args.content));
      return undefined as T;
    case "delete_workspace_file":
      files.delete(String(args.rel));
      return undefined as T;
    // The file service (editor plan ED0). `workspace` shares the map above;
    // every other root gets its own entries under `<root>\0<rel>`. Errors are
    // thrown as the `{ kind, message }` object the Rust side serialises.
    case "file_roots":
      return [
        { id: "workspace", label: "Second Brain", path: "/mock/vault", kind: "workspace" },
        { id: "project:1", label: "Axiomata-OS", path: "/mock/code/axiomata-os", kind: "project" },
        { id: "worktree:1", label: "Builder", path: "/mock/.axiomata/worktrees/builder-1", kind: "worktree" },
      ] as T;
    case "file_read": {
      const { root, rel } = fileArgs(args);
      const content = fileStore(root).get(fileKey(root, rel));
      if (content === undefined) throw fileError("NotFound", `${rel} does not exist`);
      return { rel, content, version: mockVersion(content), modified: new Date().toISOString(), large: false } as T;
    }
    case "file_write": {
      const { root, rel } = fileArgs(args);
      const store = fileStore(root);
      const current = store.get(fileKey(root, rel));
      const expected = args.expected as string | null | undefined;
      if (expected != null && (current === undefined || mockVersion(current) !== expected)) {
        throw fileError("Conflict", `${rel} changed since it was read`);
      }
      const content = String(args.content);
      store.set(fileKey(root, rel), content);
      return mockVersion(content) as T;
    }
    case "file_delete": {
      const { root, rel } = fileArgs(args);
      if (!fileStore(root).delete(fileKey(root, rel))) throw fileError("NotFound", `${rel} does not exist`);
      return undefined as T;
    }
    case "file_list": {
      const root = String(args.root);
      return mockListing(root, String(args.rel ?? "")) as T;
    }
    case "file_mkdir": {
      const { root, rel } = fileArgs(args);
      if (relsOf(root).some((p) => p === rel || p.startsWith(`${rel}/`))) {
        throw fileError("Refused", `${rel}: something with this name is already there`);
      }
      mockDirs.add(`${root}\0${rel}`);
      return undefined as T;
    }
    case "file_rename": {
      const root = String(args.root);
      const from = String(args.from);
      const to = String(args.to);
      if (relsOf(root).some((p) => p === to || p.startsWith(`${to}/`))) {
        throw fileError("Refused", `${to}: something with this name is already there`);
      }
      if (mockMoveTree(root, from, to) === 0) throw fileError("NotFound", `${from} does not exist`);
      mockEmit("files:renamed", { root, from, to });
      return undefined as T;
    }
    case "file_index": {
      const root = String(args.root);
      const files = relsOf(root)
        .filter((p) => !p.endsWith("/") && !p.split("/").some((part) => part.startsWith(".")))
        .sort();
      return { files, truncated: false } as T;
    }
    // No language servers in the browser mock: the editor runs without them.
    case "lsp_start":
      return { kind: "none" } as T;
    case "lsp_send":
    case "lsp_opened":
    case "lsp_closed":
      return undefined as T;
    case "file_search": {
      // A plain JS search over the fixture files — enough to see the view work.
      const root = String(args.root);
      const q = args.query as { pattern: string; regex: boolean; caseSensitive: boolean; wholeWord: boolean };
      const onBatch = args.onBatch as { onmessage: (b: { files: unknown[] }) => void };
      let body = q.regex ? q.pattern : q.pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      if (q.wholeWord) body = `\\b(?:${body})\\b`;
      let re: RegExp;
      try {
        re = new RegExp(body, q.caseSensitive ? "g" : "gi");
      } catch (err) {
        // The shape the Rust side sends (`FilesError::BadPattern`'s text).
        throw fileError("BadPattern", `bad search pattern: ${(err as Error).message}`);
      }
      const summary = { files: 0, matches: 0, searched: 0, truncated: false, cancelled: false };
      for (const rel of relsOf(root).filter((p) => !p.endsWith("/")).sort()) {
        summary.searched++;
        const matches: unknown[] = [];
        (fileStore(root).get(fileKey(root, rel)) ?? "").split("\n").forEach((text, line) => {
          for (const m of text.matchAll(re)) {
            if (m[0]) matches.push({ line, col: m.index, end: m.index + m[0].length, text, from: 0 });
          }
        });
        if (matches.length === 0) continue;
        summary.files++;
        summary.matches += matches.length;
        onBatch.onmessage({ files: [{ rel, matches }] });
      }
      return summary as T;
    }
    case "file_search_cancel":
      return undefined as T;
    case "installed_fonts":
      return [
        { family: "Andale Mono", weights: [400], monospace: true },
        { family: "Helvetica Neue", weights: [100, 200, 300, 400, 500, 700], monospace: false },
        { family: "Menlo", weights: [400, 700], monospace: true },
        { family: "SF Mono", weights: [300, 400, 500, 600, 700, 800], monospace: true },
      ] as T;
    case "file_create": {
      const { root, rel } = fileArgs(args);
      if (relsOf(root).some((p) => p === rel || p.startsWith(`${rel}/`))) {
        throw fileError("Refused", `${rel}: something with this name is already there`);
      }
      fileStore(root).set(fileKey(root, rel), "");
      return mockVersion("") as T;
    }
    case "file_count": {
      const { root, rel } = fileArgs(args);
      return relsOf(root).filter((p) => p.startsWith(`${rel}/`)).length as T;
    }
    case "file_delete_tree": {
      const { root, rel } = fileArgs(args);
      if (!rel) throw fileError("Refused", "the root itself cannot be changed");
      if (mockMoveTree(root, rel, null) === 0) throw fileError("NotFound", `${rel} does not exist`);
      mockEmit("files:removed", { root, rel });
      return undefined as T;
    }
    // No file system to watch in a browser; subscribing is accepted and
    // nothing ever changes underneath.
    case "file_watch":
    case "file_unwatch":
      return undefined as T;
    case "editor_recovery_save": {
      const key = `${String(args.root)}\0${String(args.rel)}`;
      recoveries.set(key, {
        root: args.root,
        rel: args.rel,
        base_version: args.base ?? null,
        content: args.content,
        saved_at: new Date().toISOString(),
      });
      return undefined as T;
    }
    case "editor_recovery_load":
      return (recoveries.get(`${String(args.root)}\0${String(args.rel)}`) ?? null) as T;
    case "editor_recovery_delete":
      recoveries.delete(`${String(args.root)}\0${String(args.rel)}`);
      return undefined as T;
    // The Mac clipboard for Vi (ED3, V3): a string in the mock.
    case "clipboard_read":
      return mockClipboard as T;
    case "clipboard_write":
      mockClipboard = String(args.text);
      return undefined as T;
    case "file_read_image":
      if (args.root === "worktree:1" && args.rel === MOCK_IMAGE_PATH) {
        return { rel: MOCK_IMAGE_PATH, mime: "image/png", base64: MOCK_IMAGES.worktree } as T;
      }
      // The workspace has the same picture, for the file panel's image view.
      if (args.root === "workspace" && args.rel === MOCK_IMAGE_PATH) {
        return { rel: MOCK_IMAGE_PATH, mime: "image/png", base64: MOCK_IMAGES.worktree } as T;
      }
      throw fileError("NotFound", "devmock has no images for the file service");
    case "file_pick":
      if (nextPick && !args.folder) {
        const picked = { ...nextPick, folder: false, path: `/mock/${nextPick.rel}` };
        nextPick = null;
        return picked as T;
      }
      // No native dialog in a browser: pretend the first vault note was picked.
      return args.folder
        ? ({ root: "workspace", rel: "", folder: true, path: "/mock/vault" } as T)
        : ({ root: "workspace", rel: [...files.keys()][0] ?? "", folder: false, path: "/mock/vault" } as T);
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
    case "list_installed_apps":
      return {
        apps: [
          { path: "/Applications/Safari.app", name: "Safari" },
          { path: "/Applications/Notes.app", name: "Notes" },
          { path: "/Applications/Slack.app", name: "Slack" },
          { path: "/System/Applications/Utilities/Terminal.app", name: "Terminal" },
        ],
        truncated: false,
      } satisfies InstalledAppsResult as T;
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
