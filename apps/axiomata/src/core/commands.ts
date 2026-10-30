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

import { tChat } from "../i18n/chat";

import { emit } from "./bus";
import { listHandbook, openHandbook, pageForTopic, type HandbookEntry } from "./handbook";
import { busy, newSession, planMode, send, sessionId, turnEffort, turnModel, turns } from "./chat";
import { createInstance, destroyInstance } from "./lifecycle";
import { loadModelCatalog, modelCatalog, providerOptions } from "./modelCatalog";
import { getModule, invokeAction, listModules } from "./registry";
import { openFilePanel } from "./staging";
import { instances } from "./stores";
import { THEMES, applyTheme, isTheme } from "./themes";
import { type ConfigView, type HarnessId, type Skill, invokeBackend, isEffortLevel } from "./backend";
import { defaultLabel, effortState, levelsFor, reasoningOf, unlistedNote } from "./effortLevels";
import {
  conversationsPanel,
  costPanel,
  effortPanel,
  helpPanel,
  modelPanel,
  skillsPanel,
  textPanel,
  type Panel,
} from "./slashLists";

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
  /** Short, toast-sized; empty when the panel says it all. */
  message: string;
  /** What opens above the bar — a list to pick from or an answer (#109); never written into the conversation. */
  panel?: Panel;
}

/** What the chat's harness offers, refreshed by the bar (`refreshEngineCommands`). */
export const engineCommands = writable<EngineCommand[]>([]);

/** The chat's harness as last read with its commands; `null` before that. Offers the Glut-only commands. */
export const chatHarness = writable<HarnessId | null>(null);

/** App commands only one harness reads, by name: the others leave them out of the suggestions and `/help`. */
const HARNESS_ONLY: Record<string, HarnessId> = { effort: "axh" };

/** One command as `chat_commands` sends it. */
interface EngineCommandWire {
  name: string;
  source: EngineSource;
  hint: string | null;
  description: string;
}

/**
 * Reads the chat harness's commands again. A harness that cannot answer just now keeps the last
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

/** Reads which harness the chat runs on, for the commands only one harness reads; read with the engine's commands. */
export async function refreshChatHarness(): Promise<void> {
  try {
    chatHarness.set((await invokeBackend<ConfigView>("get_config")).agents.chat_harness);
  } catch {
    // Kept as it was: unknown, every command stays offered, and `/effort` itself says when the harness is another.
  }
}

/** One earlier conversation as `chat_sessions` sends it. */
interface ChatSessionWire {
  id: string;
  title: string;
  updated_at: string;
  turns: number;
}

type AppCommand = Omit<CommandEntry, "source" | "description"> & { describe: () => string };

/** The app's commands; a description is read when shown, so it follows the language (`i18n/chat.ts`). */
const CONVERSATION: AppCommand[] = [
  { name: "help", level: 1, hint: "[topic]", describe: () => tChat("cmdHelp") },
  { name: "clear", level: 1, describe: () => tChat("cmdClear") },
  { name: "compact", level: 1, describe: () => tChat("cmdCompact") },
  { name: "model", level: 1, hint: "[model]", describe: () => tChat("cmdModel") },
  { name: "effort", level: 1, hint: "[level]", describe: () => tChat("cmdEffort") },
  { name: "plan", level: 1, describe: () => tChat("cmdPlan") },
  { name: "build", level: 1, describe: () => tChat("cmdBuild") },
  { name: "skills", level: 1, hint: "[run <name>]", describe: () => tChat("cmdSkills") },
  { name: "cost", level: 1, describe: () => tChat("cmdCost") },
  { name: "resume", level: 1, hint: "[session]", describe: () => tChat("cmdResume") },
  { name: "undo", level: 1, describe: () => tChat("cmdUndo") },
  { name: "do", level: 1, hint: "<text>", describe: () => tChat("cmdDo") },
];

const CONTROL: AppCommand[] = [
  { name: "tile", level: 2, hint: "add|remove <type> · <type> <action> [json]", describe: () => tChat("cmdTile") },
  { name: "theme", level: 2, hint: "<id>", describe: () => tChat("cmdTheme") },
  { name: "open", level: 2, hint: "<path>", describe: () => tChat("cmdOpen") },
  { name: "newfile", level: 2, hint: "<path>", describe: () => tChat("cmdNewfile") },
  { name: "brain", level: 2, hint: "[path | ? query]", describe: () => tChat("cmdBrain") },
];

const APP_NAMES = new Set([...CONVERSATION, ...CONTROL].map((c) => c.name));

/** The old spellings (before #108) and what they are now; answered with a hint for one release (S3). */
const MOVED: Record<string, string> = { add: "/tile add", remove: "/tile remove", skill: "/skills run" };

const PREFIXES: readonly EngineSource[] = ["glut", "oc", "cc"];

/** Every command the bar knows, in the order `/help` and the suggestion list show them. */
export function commandEntries(
  engine: readonly EngineCommand[] = get(engineCommands),
  harness: HarnessId | null = get(chatHarness),
): CommandEntry[] {
  const offered = (c: AppCommand): boolean => !harness || (HARNESS_ONLY[c.name] ?? harness) === harness;
  const app: CommandEntry[] = [...CONVERSATION, ...CONTROL]
    .filter(offered)
    .map(({ describe, ...c }) => ({ ...c, description: describe(), source: "app" }));
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
    return rest
      ? { kind: "instruct", message: rest }
      : { kind: "unknown", name, message: tChat("usage", { usage: "/do <text>" }) };
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
const whileBusy = (): CommandResult => ({ ok: false, message: tChat("whileBusy") });

function unknownMessage(name: string, engine: readonly EngineCommand[]): string {
  const moved = MOVED[name] ?? (getModule(name) ? `/tile ${name}` : undefined);
  if (moved) return tChat("movedTo", { name, now: moved });
  const near = nearest(
    name,
    commandEntries(engine).map((c) => c.name),
  );
  return near.length > 0
    ? tChat("didYouMean", { name, near: near.map((n) => `/${n}`).join(", ") })
    : tChat("unknownHint", { name });
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
 * The `/help` list: the commands by level, then the handbook's pages. The `/tile` and `/theme` lines say their types
 * and themes in their hints; the engine's commands arrive with the chat.
 */
async function helpList(): Promise<Panel> {
  let pages: HandbookEntry[] = [];
  try {
    pages = await listHandbook();
  } catch {
    // Without the handbook the commands are still listed.
  }
  const entries = commandEntries().map((c) =>
    c.name === "tile"
      ? { ...c, hint: `${c.hint ?? ""} — ${listTypes()}` }
      : c.name === "theme"
        ? { ...c, hint: `<${THEMES.map((t) => t.id).join("|")}>` }
        : c,
  );
  return helpPanel(entries, pages);
}

/**
 * `/help <topic>` (chat-knows-app CH4): the handbook page the topic means opens in the Help panel; a topic that fits
 * no page — or several — lists the pages instead.
 */
async function helpTopic(topic: string): Promise<CommandResult> {
  let entries: HandbookEntry[];
  try {
    entries = await listHandbook();
  } catch (err) {
    return { ok: false, message: tChat("handbookUnreadable", { error: String(err) }) };
  }
  const page = pageForTopic(topic, entries);
  if (page) {
    openHandbook(page);
    const title = entries.find((e) => e.name === page)?.title ?? page;
    return { ok: true, message: tChat("handbookOpened", { title }) };
  }
  return { ok: false, message: tChat("noHandbookPage", { topic }), panel: await helpList() };
}

function listTypes(): string {
  return listModules()
    .filter((m) => !m.stageOnly)
    .map((m) => m.type)
    .join(", ");
}

export async function runCommand(name: string, args: string[]): Promise<CommandResult> {
  switch (name) {
    case "help":
      if (args.length > 0) return helpTopic(args.join(" "));
      return { ok: true, message: "", panel: await helpList() };

    case "clear":
      if (get(busy)) return whileBusy();
      newSession();
      return { ok: true, message: tChat("newConversation") };

    case "plan":
      planMode.set(true);
      return { ok: true, message: tChat("planOn") };

    case "build":
      planMode.set(false);
      return { ok: true, message: tChat("buildOn") };

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
      if (!isTheme(id)) {
        return { ok: false, message: tChat("unknownTheme", { id, themes: THEMES.map((t) => t.id).join(", ") }) };
      }
      applyTheme(id);
      return { ok: true, message: tChat("themeSet", { id }) };
    }

    case "open": {
      const path = args[0];
      if (!path) return { ok: false, message: tChat("usage", { usage: "/open <path>" }) };
      openFilePanel(path, "read");
      return { ok: true, message: tChat("opened", { path }) };
    }

    case "newfile": {
      const path = args[0];
      if (!path) return { ok: false, message: tChat("usage", { usage: "/newfile <path>" }) };
      try {
        await invokeBackend("write_workspace_file", { rel: path, content: "" });
      } catch (err) {
        return { ok: false, message: String(err) };
      }
      openFilePanel(path, "edit");
      return { ok: true, message: tChat("created", { path }) };
    }

    case "brain": {
      const text = args.join(" ").trim();
      if (text.startsWith("?")) {
        emit("open-second-brain", { focus: null, query: text.slice(1).trim() });
      } else {
        emit("open-second-brain", { focus: text ? `file:${text}` : null });
      }
      return { ok: true, message: tChat("brainOpened") };
    }

    default:
      return { ok: false, message: tChat("unknownCommand", { name }) };
  }
}

/** The longest model id `/model` takes; the core checks the same alphabet (`agents::valid_chat_name`). */
const MODEL_ID = /^[A-Za-z0-9._:/@+][A-Za-z0-9._:/@+-]{0,199}$/;
/**
 * `/model` lists the provider's models in the panel, the one in use marked (typing filters them); `/model <id>`
 * switches, `/model default` goes back.
 */
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
    const models = get(modelCatalog)?.providers[provider]?.models ?? [];
    return {
      ok: true,
      message: "",
      panel: modelPanel(models, { turn: current, configured, provider }, (id) => MODEL_ID.test(id)),
    };
  }
  if (wanted === "default") {
    turnModel.set(null);
    return { ok: true, message: tChat("modelConfigured", { model: configured }) };
  }
  if (!MODEL_ID.test(wanted)) return { ok: false, message: tChat("notModelId", { id: wanted }) };
  if (options.length > 0 && !options.some((o) => o.value === wanted)) {
    return { ok: false, message: tChat("notAmong", { id: wanted, provider }) };
  }
  turnModel.set(wanted);
  return { ok: true, message: tChat("modelSet", { model: wanted }) };
}

/** `/effort` lists the levels the chat model takes in the panel, the one in use marked; `/effort <level>` switches,
 *  `/effort default` goes back. Only Glut reads it: on another harness the command says so and changes nothing. */
async function effort(args: string[]): Promise<CommandResult> {
  let config: ConfigView;
  try {
    config = await invokeBackend<ConfigView>("get_config");
  } catch (err) {
    return { ok: false, message: String(err) };
  }
  if (config.agents.chat_harness !== "axh") {
    return { ok: false, message: tChat("effortGlutOnly") };
  }
  if (get(modelCatalog) === null) {
    try {
      await loadModelCatalog();
    } catch {
      // Without a catalog every level is offered; the provider takes a near one where it knows fewer.
    }
  }
  const provider = config.agents.chat_provider;
  const model = get(turnModel) ?? config.agents.providers[provider]?.chat_model ?? "";
  const reasoning = reasoningOf(get(modelCatalog), provider, model);
  const configured = config.agents.chat_effort;
  const wanted = args[0]?.toLowerCase();
  if (!wanted) {
    const turn = get(turnEffort);
    const state = effortState(turn, configured, reasoning);
    const note = unlistedNote(reasoning, state.level);
    const always = reasoning?.mandatory ? ` · ${tChat("effortAlways")}` : "";
    const silent = reasoning ? "" : ` · ${tChat("effortCatalogSilent")}`;
    return {
      ok: true,
      message: "",
      panel: effortPanel(
        levelsFor(reasoning),
        state.level,
        defaultLabel(reasoning),
        `${state.explanation}${always}${silent}${note ? ` · ${note}` : ""}`,
        turn !== null,
      ),
    };
  }
  if (wanted === "default") {
    turnEffort.set(null);
    return {
      ok: true,
      message: configured
        ? tChat("effortConfigured", { level: configured })
        : tChat("effortBack", { label: defaultLabel(reasoning) }),
    };
  }
  if (!isEffortLevel(wanted)) {
    const levels = levelsFor(reasoning).join(", ");
    return { ok: false, message: tChat("notEffortLevel", { level: args[0] ?? "", levels }) };
  }
  turnEffort.set(wanted);
  const note = unlistedNote(reasoning, wanted);
  return {
    ok: true,
    message: note ? tChat("effortSetNote", { level: wanted, note }) : tChat("effortSet", { level: wanted }),
  };
}

/** `/compact`: the conversation's harness summarises it, the next turns build on the summary. */
async function compact(): Promise<CommandResult> {
  if (get(busy)) return whileBusy();
  if (!get(sessionId)) return { ok: false, message: tChat("nothingToCompact") };
  const reply = await send("", "chat", { kind: "compact" });
  return reply && !reply.is_error
    ? { ok: true, message: tChat("compacted") }
    : { ok: false, message: tChat("notCompacted") };
}

/** `/resume` lists the earlier conversations of the chat's harness in the panel; `/resume <id>` goes on with one. */
async function resume(args: string[]): Promise<CommandResult> {
  if (get(busy)) return whileBusy();
  let list: ChatSessionWire[];
  try {
    list = await invokeBackend<ChatSessionWire[]>("chat_sessions");
  } catch (err) {
    return { ok: false, message: String(err) };
  }
  const wanted = args[0];
  if (!wanted) {
    if (list.length === 0) return { ok: true, message: tChat("noEarlier") };
    return { ok: true, message: "", panel: conversationsPanel(list, get(sessionId)) };
  }
  const found = list.find((s) => s.id === wanted);
  if (!found) return { ok: false, message: tChat("noSuchConversation", { id: wanted }) };
  newSession();
  sessionId.set(found.id);
  return { ok: true, message: tChat("resumed", { title: plain(found.title || found.id) }) };
}

/** `/undo`: the file changes of the conversation's latest turn, undone as its harness can. */
async function undo(): Promise<CommandResult> {
  if (get(busy)) return whileBusy();
  const id = get(sessionId);
  if (!id) return { ok: false, message: tChat("nothingToUndo") };
  try {
    const done = await invokeBackend<string>("chat_undo", { sessionId: id });
    return { ok: true, message: tChat("undone"), panel: textPanel(tChat("panelUndo"), done) };
  } catch (err) {
    return { ok: false, message: String(err) };
  }
}

/** `/cost`: what this conversation's replies cost, from the transcript (every harness reports a turn's cost), as a
 *  small table in the panel (Q9). */
function costOfConversation(): CommandResult {
  const replies = get(turns).filter((t) => t.role === "assistant" || t.role === "error");
  if (replies.length === 0) return { ok: true, message: tChat("noTurns") };
  const priced = replies.filter((t) => typeof t.costUsd === "number");
  const total = priced.reduce((sum, t) => sum + (t.costUsd ?? 0), 0);
  const unpriced = replies.length - priced.length;
  return { ok: true, message: "", panel: costPanel(replies.length, total, unpriced) };
}

async function skills(args: string[]): Promise<CommandResult> {
  if (args[0] === "run") {
    if (!args[1]) return { ok: false, message: tChat("usage", { usage: "/skills run <name>" }) };
    try {
      const run = await invokeBackend<{ status: string; duration_ms: number }>("run_skill", { name: args[1] });
      return {
        ok: run.status === "success",
        message: tChat("skillRan", { name: args[1], status: run.status, ms: run.duration_ms }),
      };
    } catch (err) {
      return { ok: false, message: String(err) };
    }
  }
  if (args.length > 0) return { ok: false, message: tChat("usage", { usage: "/skills [run <name>]" }) };
  try {
    const list = await invokeBackend<Skill[]>("list_skills");
    if (list.length === 0) return { ok: true, message: tChat("noSkills") };
    return { ok: true, message: "", panel: skillsPanel(list) };
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
        message: tChat("usage", { usage: `/tile add|remove <type> · /tile <type> <action> — ${listTypes()}` }),
      };

    case "add": {
      const type = rest[0] ?? "";
      const r = createInstance(type);
      return r.ok
        ? { ok: true, message: tChat("added", { title: getModule(type)?.title ?? type }) }
        : { ok: false, message: r.reason };
    }

    case "remove": {
      const key = rest[0] ?? "";
      const hit = get(instances).find((i) => i.type === key || (key.length >= 4 && i.id.startsWith(key)));
      if (!hit) return { ok: false, message: tChat("noTileMatches", { key }) };
      destroyInstance(hit.id);
      return { ok: true, message: tChat("removed", { title: getModule(hit.type)?.title ?? hit.type }) };
    }

    default:
      return moduleAction(first, rest);
  }
}

/** `/tile <type> <action> [json]` on the first instance of that type. */
async function moduleAction(type: string, args: string[]): Promise<CommandResult> {
  const def = getModule(type);
  if (!def) return { ok: false, message: tChat("unknownTileType", { type, types: listTypes() }) };
  const inst = get(instances).find((i) => i.type === type);
  if (!inst) return { ok: false, message: tChat("noTileOnCanvas", { title: def.title, type }) };
  const action = args[0];
  if (!action) {
    const names = (def.actions ?? []).map((a) => a.name).join(", ") || tChat("noActions");
    return { ok: false, message: tChat("usage", { usage: `/tile ${type} <action> [json] — ${names}` }) };
  }
  let params: unknown = {};
  const raw = args.slice(1).join(" ");
  if (raw) {
    try {
      params = JSON.parse(raw);
    } catch {
      return { ok: false, message: tChat("paramsJson", { example: '{"path":"notes/inbox.md"}' }) };
    }
  }
  try {
    const out = await invokeAction(inst.id, action, params);
    const message = tChat("tileDone", { type, action });
    if (out === undefined) return { ok: true, message };
    const title = tChat("panelTile", { type, action });
    return { ok: true, message, panel: textPanel(title, JSON.stringify(out, null, 2), true) };
  } catch (err) {
    return { ok: false, message: String(err) };
  }
}
