/**
 * What the chat may operate in the app (`docs/plans/chat-knows-app.md`, CH5, Q8), as shell actions — they work
 * whether or not a tile is mounted, and the manifest lists them for the chat (`core/registry.ts`).
 *
 * - **At once:** open a view (the Studio or one project of it, the Kanban or one card, the Second Brain with a `#tag`
 *   or a search, the handbook, a Settings tab) and change the display (the Second Brain's layout, the theme, the glass
 *   knobs, the motion level). Nothing of it changes data.
 * - **After asking** (`chat: "ask"`, `core/chatAsks.ts`): start a skill, create or change a routine, start a card. The
 *   bridge asks the owner in the app; only a yes runs the action.
 * - **At once unless something would be lost** (`chat: "guarded"`): close a view (`open_views` lists them). A file
 *   with unsaved text, a Kanban field typed but not sent, a terminal running a program — then the owner is asked,
 *   told exactly what would be lost, and the view closes through its own close path, guard included.
 * - **Never** — deleting, keys and secrets, steering the Studio's agents (its panes are not closed either), the
 *   owner's steps on the board — has no action here at all, and the CLI refuses it for a chat turn
 *   (`axiomata-cli`'s `chat_turn_refusal`).
 */

import {
  invokeBackend,
  type BoardCard,
  type BoardPlan,
  type IdeProject,
  type Routine,
  type Skill,
} from "../core/backend";
import { BRAIN_LAYOUTS, brainView } from "../core/brainView";
import { emit } from "../core/bus";
import { MAX_GROUND_BLUR, MAX_GROUND_DIM } from "../core/glass";
import { listHandbook, openHandbook } from "../core/handbook";
import { createInstance } from "../core/lifecycle";
import { MOTION_LEVELS, motionSetting, type MotionLevel } from "../core/motion";
import { openShellViews, terminalSession } from "../core/openViews";
import { getModule, moduleTitle, registerShellAction } from "../core/registry";
import { SETTINGS_TABS } from "../core/settingsTabs";
import {
  FILE_PANEL,
  openFilePanel,
  pendingLoss,
  requestClose,
  staged,
} from "../core/staging";
import { glassBlur, glassDim, instances, removeInstance } from "../core/stores";
import { THEMES, applyTheme, isTheme } from "../core/themes";
import type { ModuleAction, PreparedAsk } from "../core/types";
import { tChat } from "../i18n/chat";
import { startableProjects } from "../ide/cardStart";
import { requestMode } from "../ide/modeRequest";
import { showPlanInFlow } from "../ide/planRequest";
import { requestProject } from "../ide/projectRequest";
import { listProjects } from "../ide/projects";
import { get } from "svelte/store";

import { openKanban } from "./kanbanApp";

/** The views `open_view` opens. */
export const CHAT_VIEWS = [
  "studio",
  "kanban",
  "second-brain",
  "handbook",
  "settings",
  "terminal",
  "file",
] as const;
type ChatView = (typeof CHAT_VIEWS)[number];

const STUDIO_MODES = ["editor", "agents", "flow"] as const;

/** A whole number from a param, else `undefined`. */
function int(value: unknown): number | undefined {
  return typeof value === "number" && Number.isInteger(value)
    ? value
    : undefined;
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
    "), terminal (a new terminal tile with the owner's shell, nothing typed into it), file (a path in the vault, " +
    "to read). Changes nothing.",
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
      path: { type: "string" },
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
        return {
          opened: "studio",
          project: project ?? null,
          mode: mode ?? null,
        };
      }
      case "kanban": {
        const card = int(p.card);
        // A card opens on its own board, not the one used last.
        const boardId =
          card === undefined ? undefined : (await findCard(card)).board_id;
        await openKanban({ boardId, cardId: card });
        return { opened: "kanban", card: card ?? null };
      }
      case "second-brain": {
        const filter = tagFilter(p.tag) ?? text(p.query);
        emit("open-second-brain", filter ? { focus: null, query: filter } : {});
        return { opened: "second-brain", filter: filter ?? null };
      }
      case "handbook": {
        const page = text(p.page);
        if (
          page &&
          !(await listHandbook()).some((entry) => entry.name === page)
        ) {
          throw new Error(`the handbook has no page "${page}"`);
        }
        openHandbook(page);
        return { opened: "handbook", page: page ?? null };
      }
      case "settings": {
        const tab = SETTINGS_TABS.find((t) => t.id === p.tab)?.id;
        emit("shell:settings", tab ? { tab } : {});
        return { opened: "settings", tab: tab ?? null };
      }
      case "terminal": {
        const created = createInstance("terminal");
        if (!created.ok) throw new Error(created.reason);
        return { opened: "terminal", id: created.instance.id };
      }
      case "file": {
        const path = text(p.path);
        if (!path) throw new Error("file needs a path");
        if (!openFilePanel(path, "read"))
          throw new Error(`could not open "${path}"`);
        return { opened: "file", path };
      }
      default:
        throw new Error(
          `unknown view "${String(p.view)}"; one of ${CHAT_VIEWS.join(", ")}`,
        );
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
    if (theme !== undefined && !isTheme(theme))
      throw new Error(`unknown theme "${theme}"`);
    const layout = BRAIN_LAYOUTS.find((l) => l.id === p.brainLayout)?.id;
    if (p.brainLayout !== undefined && !layout)
      throw new Error(`unknown layout "${String(p.brainLayout)}"`);
    const motion = MOTION_LEVELS.find((m) => m.id === p.motion)?.id as
      MotionLevel | undefined;
    if (p.motion !== undefined && !motion)
      throw new Error(`unknown motion level "${String(p.motion)}"`);
    const knob = (
      value: unknown,
      max: number,
      name: string,
    ): number | undefined => {
      if (value === undefined) return undefined;
      if (
        typeof value !== "number" ||
        !Number.isFinite(value) ||
        value < 0 ||
        value > max
      ) {
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
  const skill = (await invokeBackend<Skill[]>("list_skills")).find(
    (entry) => entry.name === name,
  );
  if (!skill) throw new Error(`no skill named "${name}"`);
  return skill;
}

/** The question for running a skill: its name, and what it says it does and where it runs. */
export async function prepareSkillRun(params: unknown): Promise<PreparedAsk> {
  const skill = await knownSkill(params);
  return {
    question: tChat("askRunSkill", { name: skill.name }),
    detail: `${skill.description}\n${tChat("askSkillWhere", { harness: skill.harness, name: skill.name })}`,
    params: { skill: skill.name },
  };
}

const startSkill: ModuleAction = {
  name: "start_skill",
  description: "Run a skill by name (a billed agent run).",
  params: {
    type: "object",
    properties: { skill: { type: "string" } },
    required: ["skill"],
  },
  chat: "ask",
  ask: prepareSkillRun,
  run: async (params) =>
    invokeBackend("run_skill", { name: (await knownSkill(params)).name }),
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
  if (!name || !cron)
    throw new Error(
      "a routine needs a name and a cron (six or seven fields, seconds first)",
    );
  const skill = text(p.skill);
  const prompt = text(p.prompt);
  if (!skill === !prompt)
    throw new Error("a routine runs either a skill or a prompt");
  return { id: int(p.id), name, cron, skill, prompt };
}

/** A routine's target as the owner reads it. */
function runsWhat(routine: { skill?: string; prompt?: string }): string {
  return routine.skill
    ? tChat("askRunsSkill", { name: routine.skill })
    : tChat("askRunsPromptBelow");
}

/**
 * The question for creating (`id` unset) or replacing a routine: name, schedule, what it runs — a prompt in full,
 * since it becomes an unattended run — and, for a change, the routine it replaces. The params it runs with are the
 * ones checked here.
 */
export async function prepareRoutine(params: unknown): Promise<PreparedAsk> {
  const r = routineParams(params);
  const lines: string[] = [];
  if (r.prompt) lines.push(tChat("askPromptLine", { prompt: r.prompt }));
  const asked = { name: r.name, cron: r.cron, what: runsWhat(r) };
  let question = tChat("askCreateRoutine", asked);
  if (r.id !== undefined) {
    const current = (await invokeBackend<Routine[]>("list_routines")).find(
      (routine) => routine.id === r.id,
    );
    if (!current) throw new Error(`no routine #${r.id}`);
    question = tChat("askReplaceRoutine", {
      ...asked,
      id: r.id,
      old: current.name,
    });
    const old =
      current.target.type === "skill"
        ? tChat("askRunsSkill", { name: current.target.value })
        : tChat("askRunsPrompt", { prompt: current.target.value });
    lines.push(
      tChat("askReplaces", {
        name: current.name,
        cron: current.cron_expr,
        what: old,
      }),
    );
  }
  return {
    question,
    detail: lines.join("\n"),
    params: r,
  };
}

const saveRoutine: ModuleAction = {
  name: "save_routine",
  description:
    "Create a routine, or change the one with id (a full replace of name, cron, skill/prompt; its on/off state " +
    "stays). " +
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
    const target = r.skill
      ? { type: "skill", value: r.skill }
      : { type: "prompt", value: r.prompt ?? "" };
    if (r.id === undefined) {
      return invokeBackend("add_routine", {
        new: {
          name: r.name,
          cron_expr: r.cron,
          target,
          backend: null,
          enabled: true,
        },
      });
    }
    // `update_routine` replaces every field: the routine's own on/off state is read back so a change keeps it.
    const current = (await invokeBackend<Routine[]>("list_routines")).find(
      (routine) => routine.id === r.id,
    );
    if (!current) throw new Error(`no routine #${r.id}`);
    return invokeBackend("update_routine", {
      id: r.id,
      new: {
        name: r.name,
        cron_expr: r.cron,
        target,
        backend: current.backend,
        enabled: current.enabled,
      },
    });
  },
};

/** A card of any board by its id (archived ones left out). */
async function findCard(id: number): Promise<BoardCard> {
  for (const board of await invokeBackend<{ id: number }[]>("list_boards")) {
    const cards = await invokeBackend<BoardCard[]>("list_board_cards", {
      boardId: board.id,
      includeArchived: false,
    });
    const card = cards.find((entry) => entry.id === id);
    if (card) return card;
  }
  throw new Error(`no card #${id}`);
}

/** The card to start and the project it runs in: a plain open card in no plan, and a project whose folder exists. */
async function cardToStart(
  params: unknown,
): Promise<{ card: BoardCard; project: IdeProject }> {
  const p = (params ?? {}) as Record<string, unknown>;
  const id = int(p.card);
  if (id === undefined) throw new Error("start_card needs a card id");
  const card = await findCard(id);
  if (card.plan_id !== null)
    throw new Error(
      `card #${id} is a step of plan #${card.plan_id}; its plan runs it`,
    );
  const projects = startableProjects(await listProjects());
  const wanted = int(p.project);
  const project =
    wanted === undefined
      ? projects[0]
      : projects.find((entry) => entry.id === wanted);
  if (!project) {
    throw new Error(
      wanted === undefined
        ? "there is no project a card could run in"
        : `no project #${wanted}`,
    );
  }
  return { card, project };
}

const startCard: ModuleAction = {
  name: "start_card",
  description:
    "Start a card as a plan (what Start on the card does): it becomes a plan's goal in a project (the one opened " +
    "last unless project is given), and the Studio opens on the plan in the Flow, where the owner starts its planner.",
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
      question: tChat("askStartCard", {
        id: card.id,
        title: card.title,
        project: project.name,
      }),
      detail: card.body,
      params: {
        card: card.id,
        project: project.id,
        title: card.title,
        body: card.body,
      },
    };
  },
  run: async (params) => {
    const shown = (params ?? {}) as { title?: unknown; body?: unknown };
    const { card, project } = await cardToStart(params);
    if (
      shown.title !== undefined &&
      (shown.title !== card.title || shown.body !== card.body)
    ) {
      throw new Error(
        `card #${card.id} changed since you were asked; nothing was started`,
      );
    }
    const plan = await invokeBackend<BoardPlan>("start_card_plan", {
      cardId: card.id,
      projectId: project.id,
    });
    showPlanInFlow(project.id, { boardId: plan.board_id, planId: plan.id });
    emit("shell:studio");
    return { plan: plan.id };
  },
};

/** The kinds of view `close_view` closes. The Studio is not one: its panes and agents are never the chat's to close. */
export const CLOSABLE_VIEWS = [
  "kanban",
  "file",
  "terminal",
  "second-brain",
  "handbook",
  "settings",
] as const;
type ClosableKind = (typeof CLOSABLE_VIEWS)[number];

/** One open view the chat may close, with what closing it would lose and its own way to close. */
interface Closable {
  kind: ClosableKind;
  /** What `close_view` names it by: the panel's or tile's id, or the kind for a window there is one of. */
  id: string;
  title: string;
  /** A staged panel's path (a file's, `board:<id>`, `card:<id>`), for naming it by path. */
  path?: string;
  /** What closing now would lose, for the owner; `null` for nothing. */
  loss: () => Promise<string | null>;
  /** What happens once the owner said yes, told with the question. */
  onYes: string;
  close: () => void | Promise<void>;
}

/** What closing a Terminal tile would end: the program in its foreground; a shell at its prompt ends nothing. */
async function terminalLoss(instanceId: string): Promise<string | null> {
  const session = terminalSession(instanceId);
  if (!session) return null; // its shell has ended, or never started: nothing runs
  try {
    const program = await invokeBackend<string | null>("terminal_foreground", {
      id: session,
    });
    return program ? tChat("askLossProgram", { program }) : null;
  } catch {
    // Not knowing is no "nothing": the owner decides.
    return tChat("askLossUnknown");
  }
}

/**
 * Closes staged panel `id` the way its × does: the focused field inside it gets its blur first (a Kanban card field
 * saves on blur, which a panel taken away under it would never fire), then `requestClose` — so the panel's own guard
 * still decides (a file with unsaved text asks to save or discard).
 */
async function closeStagedPanel(id: string): Promise<void> {
  const focused = document.activeElement;
  if (
    focused instanceof HTMLElement &&
    focused.closest(`[data-staged="${CSS.escape(id)}"]`)
  )
    focused.blur();
  await requestClose(id);
}

/** Everything open the chat may close, in a stable order: staged panels, Terminal tiles, the shell's windows. */
function closables(): Closable[] {
  const found: Closable[] = [];
  for (const panel of get(staged)) {
    const kind =
      panel.type === FILE_PANEL
        ? "file"
        : panel.type === "kanban"
          ? "kanban"
          : null;
    if (!kind) continue;
    const path =
      typeof panel.config.path === "string" && panel.config.path
        ? panel.config.path
        : undefined;
    found.push({
      kind,
      id: panel.id,
      title:
        kind === "file"
          ? (path ?? tChat("askNewNote"))
          : path
            ? tChat("askKanbanPanel", { path })
            : tChat("askKanbanBoard"),
      path,
      loss: async () => pendingLoss(panel.id),
      onYes: kind === "file" ? tChat("askOnYesFile") : tChat("askOnYesKanban"),
      close: () => closeStagedPanel(panel.id),
    });
  }
  for (const inst of get(instances)) {
    if (inst.type !== "terminal") continue;
    const cwd =
      typeof inst.config.cwd === "string" && inst.config.cwd
        ? inst.config.cwd
        : "";
    const def = getModule("terminal");
    const module = def ? moduleTitle(def) : "Terminal";
    found.push({
      kind: "terminal",
      id: inst.id,
      title: cwd
        ? tChat("askTerminalTileIn", { module, cwd })
        : tChat("askTerminalTile", { module }),
      loss: () => terminalLoss(inst.id),
      onYes: tChat("askOnYesTerminal"),
      // The tile's own × (`canvas/Tile.svelte`): the tile goes, and its shell with it.
      close: () => removeInstance(inst.id),
    });
  }
  for (const [kind, view] of openShellViews()) {
    found.push({
      kind,
      id: kind,
      title: view.title,
      loss: async () => view.loss?.() ?? null,
      onYes: "",
      close: () => view.close(),
    });
  }
  return found;
}

/** The one open view the params name: by `id`, else by `path`, else the only one of its kind. */
function closeTarget(params: unknown): Closable {
  const p = (params ?? {}) as Record<string, unknown>;
  const kind = CLOSABLE_VIEWS.find((k) => k === p.view);
  if (!kind) {
    throw new Error(
      `close_view closes one of ${CLOSABLE_VIEWS.join(", ")}; the Studio and its agents are the owner's to close`,
    );
  }
  const id = text(p.id);
  const path = text(p.path);
  const open = closables().filter((view) => view.kind === kind);
  const named = open.filter((view) =>
    id ? view.id === id : path ? view.path === path : true,
  );
  if (named.length === 1) return named[0];
  if (named.length === 0) {
    throw new Error(
      `no open ${kind}${id ? ` "${id}"` : path ? ` at ${path}` : ""}; open_views lists what is open`,
    );
  }
  throw new Error(
    `${named.length} ${kind} views are open (${named.map((view) => `${view.id}: ${view.title}`).join("; ")}); ` +
      "name one by id",
  );
}

const openViews: ModuleAction = {
  name: "open_views",
  chat: "direct",
  description:
    "List the views open now that close_view can close — Kanban panels, file panels, Terminal tiles, the Second " +
    "Brain, the handbook, Settings — each with its id, title, path and what closing it would lose (null: nothing). " +
    "Changes nothing.",
  params: { type: "object", properties: {} },
  run: async () =>
    Promise.all(
      closables().map(async (view) => ({
        view: view.kind,
        id: view.id,
        title: view.title,
        path: view.path ?? null,
        wouldLose: await view.loss(),
      })),
    ),
};

/**
 * The params of the closes the owner was asked about and said yes to — the very objects `ask` froze, which no caller
 * can forge (the bridge hands `run` that object, never a copy). Any other call is checked again inside `run`.
 */
const shownToOwner = new WeakSet<object>();

const closeView: ModuleAction = {
  name: "close_view",
  chat: "guarded",
  description:
    "Close an open view for the owner: view kanban|file|terminal|second-brain|handbook|settings, with id (from " +
    "open_views) or path when more than one of that kind is open. Never the Studio, its panes or its agents.",
  params: {
    type: "object",
    properties: {
      view: { type: "string", enum: [...CLOSABLE_VIEWS] },
      id: { type: "string" },
      path: { type: "string" },
    },
    required: ["view"],
  },
  // Nothing lost: no question, it closes at once. Something lost: the owner is told exactly what, and the params are
  // frozen to the one view shown — a yes never closes another one opened since.
  ask: async (params) => {
    const target = closeTarget(params);
    const loss = await target.loss();
    if (!loss) return null;
    const frozen = { view: target.kind, id: target.id };
    shownToOwner.add(frozen);
    return {
      question: tChat("askClose", { title: target.title }),
      detail: [tChat("askCloseLoses", { loss }), target.onYes]
        .filter(Boolean)
        .join(" "),
      params: frozen,
    };
  },
  run: async (params) => {
    const target = closeTarget(params);
    // Not a close the owner said yes to (nothing was lost when it was checked, or the owner's own command): checked
    // again here, so a text typed or a program started in between is never closed away unasked.
    if (!(
      typeof params === "object" &&
      params !== null &&
      shownToOwner.has(params)
    )) {
      const loss = await target.loss();
      if (loss)
        throw new Error(
          `closing ${target.title} would lose ${loss}; call close_view again to ask the owner`,
        );
    }
    await target.close();
    // A staged panel's own guard may have kept it (the owner chose Cancel at its unsaved-text question): say so. Only a
    // staged panel is gone as soon as its close returns; a window leaves the list when its component next updates.
    const isPanel = target.kind === "file" || target.kind === "kanban";
    const kept = isPanel && get(staged).some((panel) => panel.id === target.id);
    if (kept)
      return { closed: false, id: target.id, reason: "the owner kept it open" };
    return { closed: target.kind, id: target.id };
  },
};

/** The chat's actions, in the order the manifest lists them. */
export const CHAT_ACTIONS: readonly ModuleAction[] = [
  openView,
  openViews,
  closeView,
  setDisplay,
  startSkill,
  saveRoutine,
  startCard,
];

export function registerChatActions(): void {
  for (const action of CHAT_ACTIONS) registerShellAction(action);
}
