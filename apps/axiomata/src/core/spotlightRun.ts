/**
 * The spotlight's two impure halves: fetching the lists its instant sources are made of, and carrying out the action of
 * the item the owner chose (`docs/plans/spotlight-search.md`, CP2). The ranking lives in `spotlight.ts`, the item
 * building in `spotlightItems.ts`; both are pure.
 */

import { get } from "svelte/store";

import { requestMode } from "../ide/modeRequest";
import { requestProject } from "../ide/projectRequest";
import { listProjects } from "../ide/projects";
import { openKanban } from "../modules/kanbanApp";
import { openFilePanel } from "./staging";
import {
  invokeBackend,
  type Board,
  type BoardCard,
  type BoardPlan,
  type Routine,
  type RunSummary,
  type SearchHit,
  type Skill,
  type WorkspaceGraph,
} from "./backend";
import { emit } from "./bus";
import { createInstance, isPlacedSingleton } from "./lifecycle";
import { listModules } from "./registry";
import type { SpotlightSources } from "./spotlight";
import {
  cardItems,
  commandItems,
  fileNameItems,
  moduleItems,
  planItems,
  projectItems,
  routineItems,
  skillItems,
  suggestionItems,
  type SpotlightAction,
} from "./spotlightItems";
import { bringToFront, instances } from "./stores";
import { toast } from "./toast";

/** Files the full-text search may return. */
const CONTENT_HIT_LIMIT = 30;

/** The type of the Routines board module, which a routine hit opens. */
const ROUTINES_MODULE = "routines-board";

/** A source that fails to load is an empty one: a broken skills directory must not take the whole search down. */
async function orEmpty<T>(load: Promise<T[]>): Promise<T[]> {
  try {
    return await load;
  } catch {
    return [];
  }
}

/** Loads every instant source. Called each time the spotlight opens, so a new card or skill is there at once. */
export async function loadInstantSources(): Promise<SpotlightSources> {
  const [skills, routines, boards, projects, graph] = await Promise.all([
    orEmpty(invokeBackend<Skill[]>("list_skills")),
    orEmpty(invokeBackend<Routine[]>("list_routines")),
    orEmpty(invokeBackend<Board[]>("list_boards")),
    orEmpty(listProjects()),
    invokeBackend<WorkspaceGraph>("get_workspace_graph").catch(() => null),
  ]);
  const perBoard = await Promise.all(
    boards.map(async (board) => ({
      cards: await orEmpty(invokeBackend<BoardCard[]>("list_board_cards", { boardId: board.id, includeArchived: false })),
      plans: await orEmpty(invokeBackend<BoardPlan[]>("list_board_plans", { boardId: board.id })),
    })),
  );
  const names = new Map(boards.map((board) => [board.id, board.name]));
  return {
    suggestions: suggestionItems(),
    command: commandItems(),
    module: moduleItems(listModules(), isPlacedSingleton),
    skill: skillItems(skills),
    routine: routineItems(routines),
    card: cardItems(
      perBoard.flatMap((entry) => entry.cards),
      names,
    ),
    plan: planItems(perBoard.flatMap((entry) => entry.plans)),
    project: projectItems(projects),
    file: fileNameItems(graph?.files ?? []),
  };
}

/** Brings the Routines board forward when it is on the canvas, else places it. */
function showRoutines(): void {
  const placed = get(instances).find((instance) => instance.type === ROUTINES_MODULE);
  if (placed) {
    bringToFront(placed.id);
    return;
  }
  const result = createInstance(ROUTINES_MODULE);
  if (!result.ok) toast(result.reason, "warning");
}

/** The full-text search over the workspace's files (`search_workspace`): the one source that is a round-trip. */
export function searchFileContents(query: string): Promise<SearchHit[]> {
  return invokeBackend<SearchHit[]>("search_workspace", { query, limit: CONTENT_HIT_LIMIT });
}

/**
 * Carries out what the chosen item stands for. `secondary` is ⌘Return: a file is shown in the Second Brain instead of
 * opened; every other kind has no second action and does what Return does.
 */
export async function runSpotlightAction(action: SpotlightAction, secondary = false): Promise<void> {
  switch (action.type) {
    case "file":
      if (secondary) emit("open-second-brain", { focus: `file:${action.path}` });
      else openFilePanel(action.path, "read");
      return;
    case "event":
      emit(action.event, action.detail);
      return;
    case "module": {
      const result = createInstance(action.moduleType);
      if (!result.ok) toast(result.reason, "warning");
      return;
    }
    case "skill":
      try {
        const run = await invokeBackend<RunSummary>("run_skill", { name: action.name });
        toast(`/${action.name}: ${run.status} (${run.duration_ms} ms)`, run.status === "success" ? "info" : "warning");
      } catch (err) {
        toast(String(err), "danger");
      }
      return;
    case "routines":
      showRoutines();
      return;
    case "kanban":
      await openKanban({ boardId: action.boardId, cardId: action.cardId });
      return;
    case "studio":
      if (action.projectId !== undefined) requestProject(action.projectId);
      if (action.mode) requestMode(action.mode);
      emit("shell:studio");
      return;
  }
}
