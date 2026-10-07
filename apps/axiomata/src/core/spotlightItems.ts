/**
 * Turns what the app already knows — skills, routines, modules, cards, plans, projects and the shell's own actions — into
 * [`SpotlightItem`]s (`docs/plans/spotlight-search.md`, CP2). Pure and type-only on its imports, so it is tested without
 * a backend. Every item carries a [`SpotlightAction`] as its payload; `spotlightRun.ts` is what carries one out.
 */

import type { BoardCard, BoardPlan, GraphFile, IdeProject, Routine, SearchHit, Skill } from "./backend";
import type { SpotlightItem } from "./spotlight";

/** What choosing an item does. A data description, not a closure: the model stays free of the bus and the stores. */
export type SpotlightAction =
  | { type: "event"; event: string; detail?: unknown }
  | { type: "module"; moduleType: string }
  | { type: "skill"; name: string }
  | { type: "routines" }
  | { type: "kanban" }
  | { type: "file"; path: string }
  | { type: "studio"; mode?: "editor" | "agents" | "flow"; projectId?: number };

/** The slice of a module definition spotlight needs; a `ModuleDefinition` fits it. */
export interface ModuleLike {
  type: string;
  title: string;
  stageOnly?: boolean;
  singleton?: boolean;
}

const item = (
  kind: SpotlightItem["kind"],
  id: string,
  title: string,
  action: SpotlightAction,
  rest: Partial<SpotlightItem> = {},
): SpotlightItem => ({ kind, id, title, payload: action, ...rest });

/** The shell's own actions: what the icon bar and the ring offer, reachable from the keyboard. */
export function commandItems(): SpotlightItem[] {
  return [
    item("command", "new-note", "Neue Notiz", { type: "event", event: "shell:new-note" }, { keywords: "new note" }),
    item("command", "settings", "Einstellungen", { type: "event", event: "shell:settings" }, { keywords: "settings" }),
    item("command", "kanban", "Kanban öffnen", { type: "kanban" }, { keywords: "board karten" }),
    item("command", "studio", "Studio öffnen", { type: "studio" }, { keywords: "ide editor flow canvas" }),
    item("command", "add-module", "Modul hinzufügen", { type: "event", event: "shell:add-module" }, { keywords: "add" }),
    item(
      "command",
      "brain",
      "Second Brain öffnen",
      { type: "event", event: "open-second-brain" },
      { keywords: "graph orbit notizen" },
    ),
  ];
}

/** What an empty query shows: the actions most often wanted, in this order. */
export function suggestionItems(): SpotlightItem[] {
  const wanted = ["new-note", "kanban", "studio", "brain", "settings"];
  const all = commandItems();
  return wanted.flatMap((id) => all.filter((command) => command.id === id));
}

/** Modules that can be placed: not a panel-only type, and not a singleton that is already on the canvas. */
export function moduleItems(modules: ModuleLike[], isPlaced: (type: string) => boolean): SpotlightItem[] {
  return modules
    .filter((module) => !module.stageOnly && !(module.singleton && isPlaced(module.type)))
    .map((module) =>
      item("module", module.type, module.title, { type: "module", moduleType: module.type }, { subtitle: "Modul platzieren" }),
    );
}

export function skillItems(skills: Skill[]): SpotlightItem[] {
  return skills.map((skill) =>
    item("skill", skill.name, skill.name, { type: "skill", name: skill.name }, { subtitle: skill.description }),
  );
}

export function routineItems(routines: Routine[]): SpotlightItem[] {
  return routines.map((routine) =>
    item("routine", String(routine.id), routine.name, { type: "routines" }, {
      subtitle: `${routine.enabled ? "an" : "aus"} · ${routine.cron_expr}`,
    }),
  );
}

/** Cards of the boards, `boardNames` naming the board each one is on. Title and `#id` are matched; labels as keywords. */
export function cardItems(cards: BoardCard[], boardNames: Map<number, string>): SpotlightItem[] {
  return cards.map((card) =>
    item("card", String(card.id), card.title, { type: "kanban" }, {
      subtitle: `#${card.id} · ${boardNames.get(card.board_id) ?? "Brett"}`,
      keywords: [`#${card.id}`, ...card.labels].join(" "),
    }),
  );
}

export function planItems(plans: BoardPlan[]): SpotlightItem[] {
  return plans.map((plan) =>
    item("plan", String(plan.id), plan.name, { type: "studio", mode: "flow" }, {
      subtitle: `Plan · ${plan.status}`,
      keywords: plan.goal,
    }),
  );
}

export function projectItems(projects: IdeProject[]): SpotlightItem[] {
  return projects.map((project) =>
    item("project", String(project.id), project.name, { type: "studio", projectId: project.id }, {
      subtitle: project.repo_root,
    }),
  );
}

const baseName = (path: string): string => path.slice(path.lastIndexOf("/") + 1);
const dirName = (path: string): string => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");

/** Every file of the workspace graph by name (its own title as a keyword): the search over names needs no round-trip. */
export function fileNameItems(files: GraphFile[]): SpotlightItem[] {
  return files.map((file) =>
    item("file", file.path, baseName(file.path), { type: "file", path: file.path }, {
      subtitle: dirName(file.path),
      keywords: file.title,
    }),
  );
}

/**
 * The names with what the full-text search found laid over them: a file known by name gets the number of its content
 * hits; a file found only by its content is added, its first matching line as the detail (shown, never matched: a line that holds the word is a content hit, not a name hit).
 */
export function withContentHits(names: SpotlightItem[], hits: SearchHit[]): SpotlightItem[] {
  const byPath = new Map(hits.map((hit) => [hit.path, hit]));
  const merged = names.map((entry) => {
    const hit = byPath.get(entry.id);
    return hit ? { ...entry, contentMatches: hit.matches } : entry;
  });
  const known = new Set(names.map((entry) => entry.id));
  for (const hit of hits) {
    if (known.has(hit.path)) continue;
    merged.push(
      item("file", hit.path, baseName(hit.path), { type: "file", path: hit.path }, {
        subtitle: dirName(hit.path),
        detail: hit.snippet.trim(),
        contentMatches: hit.matches,
      }),
    );
  }
  return merged;
}
