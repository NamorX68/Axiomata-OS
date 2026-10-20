/**
 * The `/` command router behind the assistant bar, in three levels (`docs/plans/slash-commands.md`, S1):
 *
 * 1. **Conversation** — the same name on every chat harness, the app maps each to the engine:
 *    /help /clear /compact /model /effort /plan /build /skills /cost /resume /undo /do (`/effort` on Glut only)
 * 2. **App control** — /tile (add, remove, module actions), /theme, /open, /newfile, /brain
 * 3. **Engine commands** — what the chat's harness itself offers (`engineCommands`, filled by `chat_commands`):
 *    under their own name, or `/glut:` `/oc:` `/cc:` + name where a level 1/2 name is taken (S2).
 *
 * Text without a leading `/` is a chat turn; `/do <text>` is a one-shot agent instruction. Anything else after `/`
 * is refused with the nearest names (S3) — a typo never costs a turn.
 */

import { get, writable } from "svelte/store";

import { emit } from "./bus";
import { busy, newSession, planMode, send, sessionId, turnEffort, turnModel, turns } from "./chat";
import { createInstance, destroyInstance } from "./lifecycle";
import { loadModelCatalog, modelCatalog, providerOptions } from "./modelCatalog";
import { getModule, invokeAction, listModules } from "./registry";
import { openFilePanel } from "./staging";
import { instances } from "./stores";
import { THEMES, applyTheme, isTheme } from "./themes";
import { type ConfigView, EFFORT_LEVELS, type Skill, invokeBackend, isEffortLevel } from "./backend";

/** Where a command comes from: the app itself, or the chat's harness. */
export type CommandSource = "app" | "glut" | "oc" | "cc";
export type EngineSource = Exclude<CommandSource, "app">;

/** One command an engine offers, as `chat_commands` reports it. Names and descriptions are data, shown as text. */
export interface EngineCommand {
  name: string;
  source: EngineSource;
  hint?: string;
  description: string;
}

/** One line of the suggestion list and of `/help`. `name` is what the owner types (with its prefix if it clashed). */
export interface CommandEntry {
  name: string;
  source: CommandSource;
  level: 1 | 2 | 3;
  hint?: string;
  description: string;
}

export type Routed =
  | { kind: "chat"; message: string }
  | { kind: "instruct"; message: string }
  | { kind: "command"; name: string; args: string[] }
  /** An engine command: `name` is the engine's own (no prefix), `args` the rest of the line as typed. */
  | { kind: "engine"; name: string; source: EngineSource; args: string }
  | { kind: "unknown"; name: string; message: string };

export interface CommandResult {
  ok: boolean;
  /** Short, toast-sized. */
  message: string;
  /** Optional Markdown for the chat panel (e.g. /help, action output). */
  detail?: string;
}

/** What the chat's harness offers, refreshed by the bar (`refreshEngineCommands`). */
export const engineCommands = writable<EngineCommand[]>([]);

/** One command as `chat_commands` sends it. */
interface EngineCommandWire {
  name: string;
  source: EngineSource;
  hint: string | null;
  description: string;
}

/**
 * Reads the chat harness's commands again. A harness that cannot answer just now (Opencode not up yet) keeps the last
 * list: that is no reason to forget it.
 */
export async function refreshEngineCommands(): Promise<void> {
  try {
    const list = await invokeBackend<EngineCommandWire[]>("chat_commands");
    engineCommands.set(
      list.map(({ name, source, hint, description }) => ({ name, source, description, ...(hint ? { hint } : {}) })),
    );
  } catch {
    // Kept as it was; `/help` says the list arrives with the chat.
  }
}

/** One earlier conversation as `chat_sessions` sends it. */
interface ChatSessionWire {
  id: string;
  title: string;
  updated_at: string;
  turns: number;
}

type AppCommand = Omit<CommandEntry, "source">;

const CONVERSATION: AppCommand[] = [
  { name: "help", level: 1, description: "List every command" },
  { name: "clear", level: 1, description: "Start a new conversation" },
  { name: "compact", level: 1, description: "Summarise the conversation so far to free context" },
  { name: "model", level: 1, hint: "[model]", description: "Show or change this conversation's model" },
  {
    name: "effort",
    level: 1,
    hint: "[level]",
    description: "Show or change how hard Glut's model reasons in this conversation",
  },
  { name: "plan", level: 1, description: "Plan mode: the agent reads and plans, changes nothing" },
  { name: "build", level: 1, description: "Build mode: the agent may carry the plan out" },
  { name: "skills", level: 1, hint: "[run <name>]", description: "List the skills, or run one" },
  { name: "cost", level: 1, description: "Turns and money of this conversation" },
  { name: "resume", level: 1, hint: "[session]", description: "List earlier conversations, or continue one" },
  { name: "undo", level: 1, description: "Undo the file changes of the last turn" },
  { name: "do", level: 1, hint: "<text>", description: "A one-shot instruction to the agent, outside the chat" },
];

const CONTROL: AppCommand[] = [
  {
    name: "tile",
    level: 2,
    hint: "add|remove <type> · <type> <action> [json]",
    description: "Tiles and their actions",
  },
  { name: "theme", level: 2, hint: "<id>", description: "Switch the theme" },
  { name: "open", level: 2, hint: "<path>", description: "Open a workspace file (.md or .html)" },
  { name: "newfile", level: 2, hint: "<path>", description: "Create an empty workspace file and open it" },
  { name: "brain", level: 2, hint: "[path | ? query]", description: "Open the Second Brain (on a file, or searching)" },
];

const APP_NAMES = new Set([...CONVERSATION, ...CONTROL].map((c) => c.name));

/** The old spellings (before #108) and what they are now; answered with a hint for one release (S3). */
const MOVED: Record<string, string> = { add: "/tile add", remove: "/tile remove", skill: "/skills run" };

const PREFIXES: readonly EngineSource[] = ["glut", "oc", "cc"];

/** Every command the bar knows, in the order `/help` and the suggestion list show them. */
export function commandEntries(engine: readonly EngineCommand[] = get(engineCommands)): CommandEntry[] {
  const app: CommandEntry[] = [...CONVERSATION, ...CONTROL].map((c) => ({ ...c, source: "app" }));
  const own: CommandEntry[] = engine.map((c) => ({
    ...c,
    level: 3,
    name: APP_NAMES.has(c.name) ? `${c.source}:${c.name}` : c.name,
  }));
  return [...app, ...own];
}

/** Classifies raw bar input against the app's commands and the engine's. */
export function route(input: string, engine: readonly EngineCommand[] = get(engineCommands)): Routed | null {
  const text = input.trim();
  if (!text) return null;
  if (!text.startsWith("/")) return { kind: "chat", message: text };
  const body = text.slice(1);
  const head = body.split(/\s/, 1)[0] ?? "";
  const rest = body.slice(head.length).trim();
  const name = head.toLowerCase();
  if (name === "do") {
    return rest ? { kind: "instruct", message: rest } : { kind: "unknown", name, message: "usage: /do <text>" };
  }
  if (APP_NAMES.has(name)) return { kind: "command", name, args: rest ? rest.split(/\s+/) : [] };
  const own = engineCommand(name, engine);
  if (own) return { kind: "engine", name: own.name, source: own.source, args: rest };
  return { kind: "unknown", name, message: unknownMessage(name, engine) };
}

/**
 * The engine command `typed` (lower case) names: its own name unless an app name took it, or `<source>:<name>`
 * always. Matched without case, as the app's own names are; the engine gets its name as it spells it.
 */
function engineCommand(typed: string, engine: readonly EngineCommand[]): EngineCommand | undefined {
  const same = (c: EngineCommand, name: string): boolean => c.name.toLowerCase() === name;
  for (const source of PREFIXES) {
    const prefix = `${source}:`;
    if (typed.startsWith(prefix)) {
      const name = typed.slice(prefix.length);
      return engine.find((c) => c.source === source && same(c, name));
    }
  }
  if (APP_NAMES.has(typed)) return undefined;
  return engine.find((c) => same(c, typed));
}

/** Text from a harness or a file made safe to stand in Markdown: no code span, link or tag of its own. */
function plain(text: string): string {
  return text.replace(/[`[\]<>]/g, "");
}

/** The answer for a command that must wait while the agent works: it would change what the turn works on. */
const WHILE_BUSY: CommandResult = { ok: false, message: "The agent is working — try again when it is done." };

function unknownMessage(name: string, engine: readonly EngineCommand[]): string {
  const moved = MOVED[name] ?? (getModule(name) ? `/tile ${name}` : undefined);
  if (moved) return `/${name} is now ${moved}`;
  const near = nearest(
    name,
    commandEntries(engine).map((c) => c.name),
  );
  return near.length > 0
    ? `unknown command /${name} — did you mean ${near.map((n) => `/${n}`).join(", ")}?`
    : `unknown command /${name} — /help lists them; /do <text> sends an instruction`;
}

/** Up to three names that start with `typed` or are a small edit away from it. */
export function nearest(typed: string, names: readonly string[]): string[] {
  const starting = names.filter((n) => n.startsWith(typed));
  if (starting.length > 0) return starting.slice(0, 3);
  return names
    .map((n) => ({ n, d: distance(typed, n) }))
    .filter(({ d }) => d <= 2)
    .sort((a, b) => a.d - b.d)
    .slice(0, 3)
    .map(({ n }) => n);
}

/** Levenshtein distance; names are short, so the plain table is enough. */
function distance(a: string, b: string): number {
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const next = [i];
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      next[j] = Math.min((row[j] ?? 0) + 1, (next[j - 1] ?? 0) + 1, (row[j - 1] ?? 0) + cost);
    }
    row = next;
  }
  return row[b.length] ?? 0;
}

/**
 * The entries the suggestion list shows for the bar's text: while the first word is being typed after `/`, every
 * command whose name starts with it; once a space follows, none.
 */
export function suggestions(input: string, engine: readonly EngineCommand[] = get(engineCommands)): CommandEntry[] {
  if (!input.startsWith("/") || /\s/.test(input)) return [];
  const typed = input.slice(1).toLowerCase();
  return commandEntries(engine).filter((c) => c.name.toLowerCase().startsWith(typed));
}

/**
 * The `/help` text. A function, not a constant: the `/tile` line lists the registered module types, which register
 * after this module is imported (`main.ts`), and the engine's commands arrive with the chat.
 */
export function helpText(engine: readonly EngineCommand[] = get(engineCommands)): string {
  const line = (c: CommandEntry): string =>
    `- \`/${c.name}${c.hint ? ` ${c.hint}` : ""}\` — ${c.description}${c.level === 3 ? ` *(${c.source})*` : ""}`;
  const entries = commandEntries(engine);
  const own = entries.filter((c) => c.level === 3);
  return [
    "**Conversation**",
    "",
    ...entries.filter((c) => c.level === 1).map(line),
    "",
    `**App** — tile types: ${listTypes()}; themes: ${THEMES.map((t) => t.id).join(" · ")}`,
    "",
    ...entries.filter((c) => c.level === 2).map(line),
    "",
    own.length > 0
      ? "**The chat's harness**"
      : "**The chat's harness** — its commands appear once the chat knows them",
    ...(own.length > 0 ? ["", ...own.map(line)] : []),
    "",
    "No slash — chat with the agent.",
  ].join("\n");
}

function listTypes(): string {
  return listModules()
    .filter((m) => !m.stageOnly)
    .map((m) => `\`${m.type}\``)
    .join(", ");
}

export async function runCommand(name: string, args: string[]): Promise<CommandResult> {
  switch (name) {
    case "help":
      return { ok: true, message: "See the chat panel.", detail: helpText() };

    case "clear":
      if (get(busy)) return WHILE_BUSY;
      newSession();
      return { ok: true, message: "New conversation." };

    case "plan":
      planMode.set(true);
      return { ok: true, message: "Plan mode: the agent reads and plans, changes nothing." };

    case "build":
      planMode.set(false);
      return { ok: true, message: "Build mode." };

    case "cost":
      return costOfConversation();

    case "model":
      return model(args);

    case "effort":
      return effort(args);

    case "compact":
      return compact();

    case "resume":
      return resume(args);

    case "undo":
      return undo();

    case "skills":
      return skills(args);

    case "tile":
      return tile(args);

    case "theme": {
      const id = args[0] ?? "";
      if (!isTheme(id)) return { ok: false, message: `unknown theme "${id}" (${THEMES.map((t) => t.id).join(", ")})` };
      applyTheme(id);
      return { ok: true, message: `Theme: ${id}.` };
    }

    case "open": {
      const path = args[0];
      if (!path) return { ok: false, message: "usage: /open <path>" };
      openFilePanel(path, "read");
      return { ok: true, message: `Opened ${path}.` };
    }

    case "newfile": {
      const path = args[0];
      if (!path) return { ok: false, message: "usage: /newfile <path>" };
      try {
        await invokeBackend("write_workspace_file", { rel: path, content: "" });
      } catch (err) {
        return { ok: false, message: String(err) };
      }
      openFilePanel(path, "edit");
      return { ok: true, message: `Created ${path}.` };
    }

    case "brain": {
      const text = args.join(" ").trim();
      if (text.startsWith("?")) {
        emit("open-second-brain", { focus: null, query: text.slice(1).trim() });
      } else {
        emit("open-second-brain", { focus: text ? `file:${text}` : null });
      }
      return { ok: true, message: "Second Brain opened." };
    }

    default:
      return { ok: false, message: `unknown command /${name}` };
  }
}

/** The longest model id `/model` takes; the core checks the same alphabet (`agents::valid_chat_name`). */
const MODEL_ID = /^[A-Za-z0-9._:/@+][A-Za-z0-9._:/@+-]{0,199}$/;
/** How many models `/model` lists; the rest are found by typing one. */
const MAX_LISTED_MODELS = 40;

/** `/model` shows this conversation's model and the provider's; `/model <id>` switches, `/model default` goes back. */
async function model(args: string[]): Promise<CommandResult> {
  let config: ConfigView;
  try {
    config = await invokeBackend<ConfigView>("get_config");
  } catch (err) {
    return { ok: false, message: String(err) };
  }
  const provider = config.agents.chat_provider;
  const configured = config.agents.providers[provider]?.chat_model ?? "";
  if (get(modelCatalog) === null) {
    try {
      await loadModelCatalog();
    } catch {
      // Without a catalog every well-formed id is taken; the provider says when it knows none.
    }
  }
  const options = providerOptions(get(modelCatalog), provider);
  const wanted = args[0];
  if (!wanted) {
    const current = get(turnModel);
    const lines = options
      .slice(0, MAX_LISTED_MODELS)
      .map((o) => `- \`${plain(o.value)}\`${o.hint ? ` — ${plain(o.hint)}` : ""}`);
    const more = options.length > MAX_LISTED_MODELS ? `\n\n…and ${options.length - MAX_LISTED_MODELS} more.` : "";
    const head = current
      ? `**Model** — \`${current}\` for this conversation ` +
        `(configured: \`${configured}\`, \`/model default\` goes back)`
      : `**Model** — \`${configured}\`, the configured chat model of ${provider}`;
    return {
      ok: true,
      message: `Model: ${current ?? configured}`,
      detail: lines.length > 0 ? `${head}\n\n${lines.join("\n")}${more}` : head,
    };
  }
  if (wanted === "default") {
    turnModel.set(null);
    return { ok: true, message: `Model: ${configured} (configured).` };
  }
  if (!MODEL_ID.test(wanted)) return { ok: false, message: `"${wanted}" is not a model id` };
  if (options.length > 0 && !options.some((o) => o.value === wanted)) {
    return { ok: false, message: `${wanted} is not among ${provider}'s models — /model lists them` };
  }
  turnModel.set(wanted);
  return { ok: true, message: `Model for this conversation: ${wanted}.` };
}

/** `/effort` shows this conversation's level and the configured one; `/effort <level>` switches, `/effort default`
 *  goes back. Only Glut reads it: on another harness the command says so and changes nothing. */
async function effort(args: string[]): Promise<CommandResult> {
  let config: ConfigView;
  try {
    config = await invokeBackend<ConfigView>("get_config");
  } catch (err) {
    return { ok: false, message: String(err) };
  }
  if (config.agents.chat_harness !== "axh") {
    return { ok: false, message: "/effort is Glut's: the chat runs on another harness." };
  }
  const configured = config.agents.chat_effort;
  const configuredText = configured ?? "Glut's own setting, else the provider's default";
  const wanted = args[0]?.toLowerCase();
  if (!wanted) {
    const current = get(turnEffort);
    const head = current
      ? `**Effort** — \`${current}\` for this conversation (configured: ${configuredText}, \`/effort default\` goes back)`
      : `**Effort** — ${configured ? `\`${configured}\`, the configured chat effort` : configuredText}`;
    return {
      ok: true,
      message: `Effort: ${current ?? configured ?? "default"}`,
      detail: `${head}\n\nLevels: ${EFFORT_LEVELS.map((level) => `\`${level}\``).join(" ")}`,
    };
  }
  if (wanted === "default") {
    turnEffort.set(null);
    return { ok: true, message: `Effort: ${configured ?? "default"} (configured).` };
  }
  if (!isEffortLevel(wanted)) {
    return { ok: false, message: `"${args[0]}" is no effort level — ${EFFORT_LEVELS.join(", ")}` };
  }
  turnEffort.set(wanted);
  return { ok: true, message: `Effort for this conversation: ${wanted}.` };
}

/** `/compact`: the conversation's harness summarises it, the next turns build on the summary. */
async function compact(): Promise<CommandResult> {
  if (get(busy)) return WHILE_BUSY;
  if (!get(sessionId)) return { ok: false, message: "Nothing to compact yet." };
  const reply = await send("", "chat", { kind: "compact" });
  return reply && !reply.is_error ? { ok: true, message: "Compacted." } : { ok: false, message: "Not compacted." };
}

/** `/resume` lists the earlier conversations of the chat's harness; `/resume <id>` goes on with one. */
async function resume(args: string[]): Promise<CommandResult> {
  if (get(busy)) return WHILE_BUSY;
  let list: ChatSessionWire[];
  try {
    list = await invokeBackend<ChatSessionWire[]>("chat_sessions");
  } catch (err) {
    return { ok: false, message: String(err) };
  }
  const wanted = args[0];
  if (!wanted) {
    if (list.length === 0) return { ok: true, message: "No earlier conversations." };
    const lines = list.map((s) => {
      const when = s.updated_at.slice(0, 16).replace("T", " ");
      return `- \`/resume ${s.id}\` — ${s.title ? `${plain(s.title)} · ` : ""}${when} · ${s.turns} turns`;
    });
    return {
      ok: true,
      message: `${list.length} conversations — see the chat panel.`,
      detail: `**Earlier conversations**\n\n${lines.join("\n")}`,
    };
  }
  const found = list.find((s) => s.id === wanted);
  if (!found) return { ok: false, message: `no earlier conversation ${wanted} — /resume lists them` };
  newSession();
  sessionId.set(found.id);
  return {
    ok: true,
    message: "Conversation resumed.",
    detail:
      `Resumed **${plain(found.title || found.id)}** — ` +
      "its earlier messages are not shown here; the agent has them.",
  };
}

/** `/undo`: the file changes of the conversation's latest turn, undone as its harness can. */
async function undo(): Promise<CommandResult> {
  if (get(busy)) return WHILE_BUSY;
  const id = get(sessionId);
  if (!id) return { ok: false, message: "Nothing to undo: no conversation yet." };
  try {
    const done = await invokeBackend<string>("chat_undo", { sessionId: id });
    return { ok: true, message: "Undone.", detail: done };
  } catch (err) {
    return { ok: false, message: String(err) };
  }
}

/** `/cost`: what this conversation's replies cost, from the transcript (every harness reports a turn's cost). */
function costOfConversation(): CommandResult {
  const replies = get(turns).filter((t) => t.role === "assistant" || t.role === "error");
  if (replies.length === 0) return { ok: true, message: "No turns in this conversation yet." };
  const priced = replies.filter((t) => typeof t.costUsd === "number");
  const total = priced.reduce((sum, t) => sum + (t.costUsd ?? 0), 0);
  const unpriced = replies.length - priced.length;
  const note = unpriced > 0 ? ` (${unpriced} without a price)` : "";
  const message = `${replies.length} turns · $${total.toFixed(4)}${note}`;
  return { ok: true, message };
}

async function skills(args: string[]): Promise<CommandResult> {
  if (args[0] === "run") {
    if (!args[1]) return { ok: false, message: "usage: /skills run <name>" };
    try {
      const run = await invokeBackend<{ status: string; duration_ms: number }>("run_skill", { name: args[1] });
      return { ok: run.status === "success", message: `/${args[1]}: ${run.status} (${run.duration_ms} ms)` };
    } catch (err) {
      return { ok: false, message: String(err) };
    }
  }
  if (args.length > 0) return { ok: false, message: "usage: /skills [run <name>]" };
  try {
    const list = await invokeBackend<Skill[]>("list_skills");
    if (list.length === 0) return { ok: true, message: "No skills yet." };
    const lines = list.map((s) => `- \`${s.name}\` — ${s.description} *(${s.harness})*`);
    return {
      ok: true,
      message: `${list.length} skills — see the chat panel.`,
      detail: `**Skills**\n\n${lines.join("\n")}`,
    };
  } catch (err) {
    return { ok: false, message: String(err) };
  }
}

/** `/tile add <type>`, `/tile remove <type|id>`, `/tile <type> <action> [json]`. */
async function tile(args: string[]): Promise<CommandResult> {
  const [first, ...rest] = args;
  switch (first) {
    case undefined:
      return {
        ok: false,
        message: `usage: /tile add|remove <type> · /tile <type> <action> — types: ${listTypes()}`,
      };

    case "add": {
      const type = rest[0] ?? "";
      const r = createInstance(type);
      return r.ok
        ? { ok: true, message: `Added ${getModule(type)?.title ?? type}.` }
        : { ok: false, message: r.reason };
    }

    case "remove": {
      const key = rest[0] ?? "";
      const hit = get(instances).find((i) => i.type === key || (key.length >= 4 && i.id.startsWith(key)));
      if (!hit) return { ok: false, message: `no tile matches "${key}"` };
      destroyInstance(hit.id);
      return { ok: true, message: `Removed ${getModule(hit.type)?.title ?? hit.type}.` };
    }

    default:
      return moduleAction(first, rest);
  }
}

/** `/tile <type> <action> [json]` on the first instance of that type. */
async function moduleAction(type: string, args: string[]): Promise<CommandResult> {
  const def = getModule(type);
  if (!def) return { ok: false, message: `unknown tile type "${type}" — types: ${listTypes()}` };
  const inst = get(instances).find((i) => i.type === type);
  if (!inst) return { ok: false, message: `no "${def.title}" tile on the canvas — /tile add ${type} first` };
  const action = args[0];
  if (!action) {
    const names = (def.actions ?? []).map((a) => `\`${a.name}\``).join(", ") || "none";
    return { ok: false, message: `usage: /tile ${type} <action> [json] — actions: ${names}` };
  }
  let params: unknown = {};
  const raw = args.slice(1).join(" ");
  if (raw) {
    try {
      params = JSON.parse(raw);
    } catch {
      return { ok: false, message: "params must be JSON, e.g. {\"path\":\"notes/inbox.md\"}" };
    }
  }
  try {
    const out = await invokeAction(inst.id, action, params);
    const detail = out === undefined ? undefined : "```json\n" + JSON.stringify(out, null, 2) + "\n```";
    return { ok: true, message: `/tile ${type} ${action} done.`, detail };
  } catch (err) {
    return { ok: false, message: String(err) };
  }
}
