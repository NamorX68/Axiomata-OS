/**
 * What the chat may operate in the app (`docs/plans/chat-knows-app.md`, CH5, Q8), as shell actions — they work
 * whether or not a tile is mounted, and the manifest lists them for the chat (`core/registry.ts`).
 *
 * - **At once:** open a view (the Studio or one project of it, the Kanban or one card, the Second Brain with a `#tag`
 *   or a search, the handbook, a Settings tab) and change the display (the Second Brain's layout, the theme, the glass
 *   knobs, the motion level). Nothing of it changes data.
 * - **After asking** (`chat: "ask"`, `core/chatAsks.ts`): start a skill, create or change a routine, start a card. The
 *   bridge asks the owner in the app; only a yes runs the action.
 * - **Never** — deleting, keys and secrets, steering the Studio's agents, the owner's steps on the board — has no
 *   action here at all, and the CLI refuses it for a chat turn (`axiomata-cli`'s `chat_turn_refusal`).
 */

import { invokeBackend, type BoardCard, type BoardPlan, type IdeProject, type Routine, type Skill } from "../core/backend";
import { BRAIN_LAYOUTS, brainView } from "../core/brainView";
import { emit } from "../core/bus";
import { MAX_GROUND_BLUR, MAX_GROUND_DIM } from "../core/glass";
import { openHandbook } from "../core/handbook";
import { MOTION_LEVELS, motionSetting, type MotionLevel } from "../core/motion";
import { registerShellAction } from "../core/registry";
import { SETTINGS_TABS } from "../core/settingsTabs";
import { glassBlur, glassDim } from "../core/stores";
import { THEMES, applyTheme, isTheme } from "../core/themes";
import type { ModuleAction, PreparedAsk } from "../core/types";
import { startableProjects } from "../ide/cardStart";
import { requestMode } from "../ide/modeRequest";
import { showPlanInFlow } from "../ide/planRequest";
import { requestProject } from "../ide/projectRequest";
import { listProjects } from "../ide/projects";
import { openKanban } from "./kanbanApp";

/** The views `open_view` opens. */
export const CHAT_VIEWS = ["studio", "kanban", "second-brain", "handbook", "settings"] as const;
type ChatView = (typeof CHAT_VIEWS)[number];

const STUDIO_MODES = ["editor", "agents", "flow"] as const;

/** A whole number from a param, else `undefined`. */
function int(value: unknown): number | undefined {
  return typeof value === "number" && Number.isInteger(value) ? value : undefined;
}

/** A trimmed string from a param, else `undefined`. */
function text(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

/** A `#tag` as the Second Brain filters by it: one word of letters, digits, `/`, `-` and `_`. */
function tagFilter(value: unknown): string | undefined {
  const tag = text(value)?.replace(/^#/, "").toLowerCase();
  if (tag === undefined) return undefined;
  if (!/^[\p{L}\p{N}_/-]+$/u.test(tag)) throw new Error(`"${tag}" is no tag`);
  return `#${tag}`;
}

const openView: ModuleAction = {
  name: "open_view",
  chat: "direct",
  description:
    "Open a view for the owner: studio (optionally a project id and a mode editor|agents|flow), kanban (optionally a " +
    "card id), second-brain (optionally a tag to filter by, or a search query), handbook (optionally a page name), " +
    "settings (optionally a tab: " +
    SETTINGS_TABS.map((tab) => tab.id).join("|") +
    "). Changes nothing.",
  params: {
    type: "object",
    properties: {
      view: { type: "string", enum: [...CHAT_VIEWS] },
      project: { type: "integer" },
      mode: { type: "string", enum: [...STUDIO_MODES] },
      card: { type: "integer" },
      tag: { type: "string" },
      query: { type: "string" },
      page: { type: "string" },
      tab: { type: "string" },
    },
    required: ["view"],
  },
  run: async (params) => {
    const p = (params ?? {}) as Record<string, unknown>;
    const view = p.view as ChatView;
    switch (view) {
      case "studio": {
        const project = int(p.project);
        const mode = STUDIO_MODES.find((m) => m === p.mode);
        if (project !== undefined) requestProject(project);
        if (mode) requestMode(mode);
        emit("shell:studio");
        return { opened: "studio", project: project ?? null, mode: mode ?? null };
      }
      case "kanban":
        await openKanban({ cardId: int(p.card) });
        return { opened: "kanban", card: int(p.card) ?? null };
      case "second-brain": {
        const filter = tagFilter(p.tag) ?? text(p.query);
        emit("open-second-brain", filter ? { focus: null, query: filter } : {});
        return { opened: "second-brain", filter: filter ?? null };
      }
      case "handbook":
        openHandbook(text(p.page));
        return { opened: "handbook", page: text(p.page) ?? null };
      case "settings": {
        const tab = SETTINGS_TABS.find((t) => t.id === p.tab)?.id;
        emit("shell:settings", tab ? { tab } : {});
        return { opened: "settings", tab: tab ?? null };
      }
      default:
        throw new Error(`unknown view "${String(p.view)}"; one of ${CHAT_VIEWS.join(", ")}`);
    }
  },
};

const setDisplay: ModuleAction = {
  name: "set_display",
  chat: "direct",
  description:
    "Change how the app looks, any of: theme (" +
    THEMES.map((t) => t.id).join("|") +
    "), brainLayout (" +
    BRAIN_LAYOUTS.map((l) => l.id).join("|") +
    `), glassBlur (0–${MAX_GROUND_BLUR} px), glassDim (0–${MAX_GROUND_DIM} %), motion (` +
    MOTION_LEVELS.map((m) => m.id).join("|") +
    "). Changes no data.",
  params: {
    type: "object",
    properties: {
      theme: { type: "string" },
      brainLayout: { type: "string", enum: BRAIN_LAYOUTS.map((l) => l.id) },
      glassBlur: { type: "number" },
      glassDim: { type: "number" },
      motion: { type: "string", enum: MOTION_LEVELS.map((m) => m.id) },
    },
  },
  run: async (params) => {
    const p = (params ?? {}) as Record<string, unknown>;
    // Everything is checked before anything changes: a bad value changes nothing.
    const theme = text(p.theme);
    if (theme !== undefined && !isTheme(theme)) throw new Error(`unknown theme "${theme}"`);
    const layout = BRAIN_LAYOUTS.find((l) => l.id === p.brainLayout)?.id;
    if (p.brainLayout !== undefined && !layout) throw new Error(`unknown layout "${String(p.brainLayout)}"`);
    const motion = MOTION_LEVELS.find((m) => m.id === p.motion)?.id as MotionLevel | undefined;
    if (p.motion !== undefined && !motion) throw new Error(`unknown motion level "${String(p.motion)}"`);
    const knob = (value: unknown, max: number, name: string): number | undefined => {
      if (value === undefined) return undefined;
      if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > max) {
        throw new Error(`${name} must be a number from 0 to ${max}`);
      }
      return Math.round(value);
    };
    const blur = knob(p.glassBlur, MAX_GROUND_BLUR, "glassBlur");
    const dim = knob(p.glassDim, MAX_GROUND_DIM, "glassDim");
    if (theme !== undefined) applyTheme(theme);
    if (layout) brainView.update((view) => ({ ...view, layout }));
    if (blur !== undefined) glassBlur.set(blur);
    if (dim !== undefined) glassDim.set(dim);
    if (motion) motionSetting.set(motion);
    return {
      theme: theme ?? null,
      brainLayout: layout ?? null,
      glassBlur: blur ?? null,
      glassDim: dim ?? null,
      motion: motion ?? null,
    };
  },
};

/** The skill named in the params, checked against the skills there are. */
async function knownSkill(params: unknown): Promise<Skill> {
  const name = text((params as { skill?: unknown } | null)?.skill);
  if (!name) throw new Error("start_skill needs a skill name");
  const skill = (await invokeBackend<Skill[]>("list_skills")).find((entry) => entry.name === name);
  if (!skill) throw new Error(`no skill named "${name}"`);
  return skill;
}

/** The question for running a skill: its name, and what it says it does and where it runs. */
export async function prepareSkillRun(params: unknown): Promise<PreparedAsk> {
  const skill = await knownSkill(params);
  return {
    question: `Run the skill ${skill.name}`,
    detail: `${skill.description}\nRuns on ${skill.harness}; its instructions are ~/.axiomata/skills/${skill.name}/SKILL.md.`,
    params: { skill: skill.name },
  };
}

const startSkill: ModuleAction = {
  name: "start_skill",
  description: "Run a skill by name (a billed agent run).",
  params: { type: "object", properties: { skill: { type: "string" } }, required: ["skill"] },
  chat: "ask",
  ask: prepareSkillRun,
  run: async (params) => invokeBackend("run_skill", { name: (await knownSkill(params)).name }),
};

/** A routine as the params give it: name, six- or seven-field cron, and a skill or a prompt. */
export interface RoutineParams {
  id?: number;
  name: string;
  cron: string;
  skill?: string;
  prompt?: string;
}

export function routineParams(params: unknown): RoutineParams {
  const p = (params ?? {}) as Record<string, unknown>;
  const name = text(p.name);
  const cron = text(p.cron);
  if (!name || !cron) throw new Error("a routine needs a name and a cron (six or seven fields, seconds first)");
  const skill = text(p.skill);
  const prompt = text(p.prompt);
  if (!skill === !prompt) throw new Error("a routine runs either a skill or a prompt");
  return { id: int(p.id), name, cron, skill, prompt };
}

/** A routine's target as the owner reads it. */
function runsWhat(routine: { skill?: string; prompt?: string }): string {
  return routine.skill ? `the skill ${routine.skill}` : "a prompt (below)";
}

/**
 * The question for creating (`id` unset) or replacing a routine: name, schedule, what it runs — a prompt in full,
 * since it becomes an unattended run — and, for a change, the routine it replaces. The params it runs with are the
 * ones checked here.
 */
export async function prepareRoutine(params: unknown): Promise<PreparedAsk> {
  const r = routineParams(params);
  const lines: string[] = [];
  if (r.prompt) lines.push(`Prompt: ${r.prompt}`);
  let verb = "Create";
  if (r.id !== undefined) {
    const current = (await invokeBackend<Routine[]>("list_routines")).find((routine) => routine.id === r.id);
    if (!current) throw new Error(`no routine #${r.id}`);
    verb = `Replace routine #${r.id} “${current.name}” with`;
    const old = current.target.type === "skill" ? `the skill ${current.target.value}` : `the prompt “${current.target.value}”`;
    lines.push(`It replaces: “${current.name}” (${current.cron_expr}) running ${old}.`);
  }
  return {
    question: `${verb} the routine “${r.name}” (${r.cron}) running ${runsWhat(r)}`,
    detail: lines.join("\n"),
    params: r,
  };
}

const saveRoutine: ModuleAction = {
  name: "save_routine",
  description:
    "Create a routine, or change the one with id (a full replace of name, cron, skill/prompt; its on/off state stays). " +
    "cron has six or seven fields, seconds first (0 0 9 * * * is 09:00 daily); give a skill name or a prompt.",
  params: {
    type: "object",
    properties: {
      id: { type: "integer" },
      name: { type: "string" },
      cron: { type: "string" },
      skill: { type: "string" },
      prompt: { type: "string" },
    },
    required: ["name", "cron"],
  },
  chat: "ask",
  ask: prepareRoutine,
  run: async (params) => {
    const r = routineParams(params);
    const target = r.skill ? { type: "skill", value: r.skill } : { type: "prompt", value: r.prompt ?? "" };
    if (r.id === undefined) {
      return invokeBackend("add_routine", {
        new: { name: r.name, cron_expr: r.cron, target, backend: null, enabled: true },
      });
    }
    // `update_routine` replaces every field: the routine's own on/off state is read back so a change keeps it.
    const current = (await invokeBackend<Routine[]>("list_routines")).find((routine) => routine.id === r.id);
    if (!current) throw new Error(`no routine #${r.id}`);
    return invokeBackend("update_routine", {
      id: r.id,
      new: { name: r.name, cron_expr: r.cron, target, backend: current.backend, enabled: current.enabled },
    });
  },
};

/** The card to start and the project it runs in: a plain open card in no plan, and a project whose folder exists. */
async function cardToStart(params: unknown): Promise<{ card: BoardCard; project: IdeProject }> {
  const p = (params ?? {}) as Record<string, unknown>;
  const id = int(p.card);
  if (id === undefined) throw new Error("start_card needs a card id");
  const boards = await invokeBackend<{ id: number }[]>("list_boards");
  let card: BoardCard | undefined;
  for (const board of boards) {
    const cards = await invokeBackend<BoardCard[]>("list_board_cards", { boardId: board.id, includeArchived: false });
    card = cards.find((entry) => entry.id === id);
    if (card) break;
  }
  if (!card) throw new Error(`no card #${id}`);
  if (card.plan_id !== null) throw new Error(`card #${id} is a step of plan #${card.plan_id}; its plan runs it`);
  const projects = startableProjects(await listProjects());
  const wanted = int(p.project);
  const project = wanted === undefined ? projects[0] : projects.find((entry) => entry.id === wanted);
  if (!project) throw new Error(wanted === undefined ? "there is no project a card could run in" : `no project #${wanted}`);
  return { card, project };
}

const startCard: ModuleAction = {
  name: "start_card",
  description:
    "Start a card as a plan (what Start on the card does): it becomes a plan's goal in a project (the one opened last " +
    "unless project is given), and the Studio opens on the plan in the Flow, where the owner starts its planner.",
  params: {
    type: "object",
    properties: { card: { type: "integer" }, project: { type: "integer" } },
    required: ["card"],
  },
  chat: "ask",
  // The project is settled now and the card's text is shown in full — it becomes the planner's task —; a card whose
  // text changed before the yes is not started.
  ask: async (params) => {
    const { card, project } = await cardToStart(params);
    return {
      question: `Start card #${card.id} “${card.title}” as a plan in the project ${project.name}`,
      detail: card.body,
      params: { card: card.id, project: project.id, title: card.title, body: card.body },
    };
  },
  run: async (params) => {
    const shown = (params ?? {}) as { title?: unknown; body?: unknown };
    const { card, project } = await cardToStart(params);
    if (shown.title !== undefined && (shown.title !== card.title || shown.body !== card.body)) {
      throw new Error(`card #${card.id} changed since you were asked; nothing was started`);
    }
    const plan = await invokeBackend<BoardPlan>("start_card_plan", { cardId: card.id, projectId: project.id });
    showPlanInFlow(project.id, { boardId: plan.board_id, planId: plan.id });
    emit("shell:studio");
    return { plan: plan.id };
  },
};

/** The chat's actions, in the order the manifest lists them. */
export const CHAT_ACTIONS: readonly ModuleAction[] = [openView, setDisplay, startSkill, saveRoutine, startCard];

export function registerChatActions(): void {
  for (const action of CHAT_ACTIONS) registerShellAction(action);
}

