/**
 * The panel above the chat bar (`docs/plans/i18n.md`, #109, Q6–Q9, Q18): what a `/` command shows instead of writing
 * Markdown into the conversation. A **list** (`/model`, `/skills`, `/resume`, `/help`, `/effort`) has rows the owner
 * filters by typing and takes with ⏎ or a click — each row names the command line it runs or puts into the bar; an
 * **info** panel (`/cost`, `/undo`, a tile action's output) only says something. Pure: the rows are built from the
 * data the commands read, so they are tested without a backend or a DOM.
 */

import { tChat } from "../i18n/chat";
import { formatDateTime, formatNumber } from "./format";

/** One row of a panel. */
export interface PanelRow {
  /** Unique within the panel. */
  id: string;
  /** The cells, in the panel's column order. */
  cells: string[];
  /** A group heading the row stands under ("Conversation", "Handbook" …); rows of one group follow each other. */
  group?: string;
  /** The command line a pick runs (`run`) or puts into the bar to finish; none for an info row. */
  line?: string;
  run?: boolean;
  /** A pick asks first ("Run …? ⏎ yes"): a skill run costs money (Q7). */
  confirm?: string;
  /** The row the conversation uses now. */
  current?: boolean;
}

export interface Panel {
  kind: "list" | "info";
  /** The command the list belongs to: typing `/<command> <words>` filters it. */
  command?: string;
  title: string;
  /** One line under the title: what is in use now. */
  note?: string;
  columns: string[];
  rows: PanelRow[];
  /** An info panel's text instead of rows (a tile action's JSON, what `/undo` did). */
  text?: string;
  /** The text is code. */
  code?: boolean;
  /** A row "Use ‹typed›" when nothing matches — `/model` takes an id its list does not show (Q18). */
  takesTyped?: (typed: string) => string | null;
}

/** The commands whose list opens while their argument is typed. */
export const LIST_COMMANDS = ["model", "skills", "resume", "help", "effort"] as const;
export type ListCommand = (typeof LIST_COMMANDS)[number];

/**
 * The list command and filter a bar line asks for: `/model deep` → model, "deep"; `/skills run news` → skills, "news".
 * `null` while the line is no list command with a space after it.
 */
export function listQuery(line: string): { command: ListCommand; filter: string } | null {
  const m = /^\/([a-zA-Z]+)\s(.*)$/s.exec(line);
  if (!m) return null;
  const command = (m[1] ?? "").toLowerCase() as ListCommand;
  if (!LIST_COMMANDS.includes(command)) return null;
  let filter = (m[2] ?? "").trim();
  if (command === "skills") filter = filter.replace(/^run\b\s*/, "");
  return { command, filter };
}

/** The rows matching `filter`: every word must occur in one of the row's cells (case ignored). */
export function filterRows(rows: readonly PanelRow[], filter: string): PanelRow[] {
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...rows];
  return rows.filter((row) => {
    const text = row.cells.join(" ").toLowerCase();
    return words.every((w) => text.includes(w));
  });
}

/**
 * The rows a panel shows for `filter`: a row whose command ends in exactly what is typed comes first (`/model gpt-4o`
 * takes gpt-4o, not gpt-4o-mini; `/help kanban` the page), and where the list takes an unlisted entry and none ends so,
 * a last row "Use ‹typed›" (Q18).
 */
export function shownRows(panel: Panel, filter: string): PanelRow[] {
  const rows = filterRows(panel.rows, filter);
  const typed = filter.trim();
  if (!typed) return rows;
  const exact = rows.find((r) => r.line?.trim().split(/\s+/).pop()?.toLowerCase() === typed.toLowerCase());
  if (exact) return [exact, ...rows.filter((r) => r !== exact)];
  const line = panel.takesTyped ? panel.takesTyped(typed) : null;
  return line ? [...rows, { id: "typed", cells: [tChat("useTyped", { id: typed })], line, run: true }] : rows;
}

// ---------------------------------------------------------------- the lists

/** A model as the catalog has it. */
export interface ModelRowInput {
  id: string;
  name: string;
  context_length?: number;
  price?: { input_per_m: number; output_per_m: number };
}

/** "$0.19 / $0.75" — what a million tokens in and out cost. */
function price(p: ModelRowInput["price"]): string {
  if (!p) return "—";
  const usd = (n: number) => `$${formatNumber(n, { maximumFractionDigits: 2 })}`;
  return `${usd(p.input_per_m)} / ${usd(p.output_per_m)}`;
}

/** "128k", "1M". */
function context(tokens: number | undefined): string {
  if (!tokens) return "—";
  if (tokens >= 1_000_000) return `${formatNumber(tokens / 1_000_000, { maximumFractionDigits: 1 })}M`;
  return `${Math.round(tokens / 1000)}k`;
}

/** `/model`: the chat provider's models, the one in use marked; a pick switches this conversation's model. */
export function modelPanel(
  models: readonly ModelRowInput[],
  inUse: { turn: string | null; configured: string; provider: string },
  validId: (id: string) => boolean,
): Panel {
  const now = inUse.turn ?? inUse.configured;
  const rows: PanelRow[] = models.map((m) => ({
    id: m.id,
    cells: [m.name && m.name !== m.id ? `${m.name} · ${m.id}` : m.id, price(m.price), context(m.context_length)],
    line: `/model ${m.id}`,
    run: true,
    current: m.id === now,
  }));
  if (inUse.turn) {
    rows.unshift({
      id: "default",
      cells: [tChat("modelDefault", { model: inUse.configured }), "", ""],
      line: "/model default",
      run: true,
    });
  }
  return {
    kind: "list",
    command: "model",
    title: tChat("panelModels"),
    note: inUse.turn
      ? tChat("modelNoteTurn", { model: inUse.turn, configured: inUse.configured })
      : tChat("modelNoteConfigured", { model: inUse.configured, provider: inUse.provider }),
    columns: [tChat("colModel"), tChat("colPrice"), tChat("colContext")],
    rows,
    takesTyped: (typed) => (validId(typed) ? `/model ${typed}` : null),
  };
}

/** A skill as `list_skills` sends it. */
export interface SkillRowInput {
  name: string;
  description: string;
  harness: string;
}

/** `/skills`: every skill; a pick asks, then runs it (it costs money). */
export function skillsPanel(skills: readonly SkillRowInput[]): Panel {
  return {
    kind: "list",
    command: "skills",
    title: tChat("panelSkills"),
    columns: [tChat("colSkill"), tChat("colDescription"), tChat("colHarness")],
    rows: skills.map((s) => ({
      id: s.name,
      cells: [s.name, s.description, s.harness],
      line: `/skills run ${s.name}`,
      run: true,
      confirm: tChat("runAsk", { name: s.name }),
    })),
  };
}

/** An earlier conversation as `chat_sessions` sends it. */
export interface ConversationRowInput {
  id: string;
  title: string;
  updated_at: string;
  turns: number;
}

/** `/resume`: the earlier conversations, newest first as the harness lists them; a pick goes on with one. */
export function conversationsPanel(list: readonly ConversationRowInput[], current: string | null): Panel {
  return {
    kind: "list",
    command: "resume",
    title: tChat("panelConversations"),
    columns: [tChat("colConversation"), tChat("colWhen"), tChat("colTurns")],
    rows: list.map((s) => ({
      id: s.id,
      cells: [s.title || s.id, formatDateTime(s.updated_at), tChat("turns", { n: s.turns })],
      line: `/resume ${s.id}`,
      run: true,
      current: s.id === current,
    })),
  };
}

/** A command as the help list shows it. */
export interface HelpCommandInput {
  name: string;
  hint?: string;
  description: string;
  level: 1 | 2 | 3;
  source: string;
}

/** A handbook page as `listHandbook` gives it. */
export interface HelpPageInput {
  name: string;
  title: string;
  summary: string;
}

/**
 * `/help`: the commands by level — a pick puts the command into the bar to finish — then the handbook's pages, which
 * a pick opens (Q7).
 */
export function helpPanel(commands: readonly HelpCommandInput[], pages: readonly HelpPageInput[]): Panel {
  const group = (level: 1 | 2 | 3) =>
    level === 1 ? tChat("groupConversation") : level === 2 ? tChat("groupApp") : tChat("groupHarness");
  const rows: PanelRow[] = commands.map((c) => ({
    id: `cmd:${c.source}:${c.name}`,
    cells: [`/${c.name}${c.hint ? ` ${c.hint}` : ""}`, c.description, c.level === 3 ? c.source : ""],
    group: group(c.level),
    line: `/${c.name} `,
    run: false,
  }));
  if (!commands.some((c) => c.level === 3)) {
    rows.push({ id: "harness-later", cells: [tChat("harnessListLater"), "", ""], group: tChat("groupHarness") });
  }
  for (const page of pages) {
    rows.push({
      id: `page:${page.name}`,
      cells: [`/help ${page.name}`, `${page.title}: ${page.summary}`, ""],
      group: tChat("groupHandbook"),
      line: `/help ${page.name}`,
      run: true,
    });
  }
  return {
    kind: "list",
    command: "help",
    title: tChat("panelHelp"),
    columns: [tChat("colCommand"), tChat("colDescription"), tChat("colHarness")],
    rows,
  };
}

/** `/effort`: the levels the chat model takes, the one in use marked; a pick sets it for this conversation. */
export function effortPanel(
  levels: readonly string[],
  inUse: string | null,
  defaultLabel: string,
  explanation: string,
  turnSet: boolean,
): Panel {
  const rows: PanelRow[] = levels.map((level) => ({
    id: level,
    cells: [level],
    line: `/effort ${level}`,
    run: true,
    current: level === inUse,
  }));
  if (turnSet) rows.unshift({ id: "default", cells: [defaultLabel], line: "/effort default", run: true });
  return {
    kind: "list",
    command: "effort",
    title: tChat("panelEffort"),
    note: explanation,
    columns: [tChat("colLevel")],
    rows,
  };
}

/** `/cost`: turns and money of this conversation, as a small table (Q9). */
export function costPanel(turns: number, totalUsd: number, unpriced: number): Panel {
  const rows: PanelRow[] = [
    { id: "turns", cells: [tChat("costTurns"), formatNumber(turns)] },
    { id: "total", cells: [tChat("costTotal"), `$${formatNumber(totalUsd, { maximumFractionDigits: 4 })}`] },
  ];
  if (unpriced > 0) rows.push({ id: "unpriced", cells: [tChat("costUnpriced"), formatNumber(unpriced)] });
  return { kind: "info", title: tChat("panelCost"), columns: [tChat("colWhat"), tChat("colValue")], rows };
}

/** A panel that only says a text: what `/undo` did, a tile action's output. */
export function textPanel(title: string, text: string, code = false): Panel {
  return { kind: "info", title, columns: [], rows: [], text, code };
}
