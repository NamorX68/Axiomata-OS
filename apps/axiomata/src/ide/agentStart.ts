/**
 * Starting an agent from the Agents tab, the owner's favorites, and which sessions the tab lists
 * (`docs/plans/agents-model.md`, AM4, Q7, Q8, Q16, Q17, Q18). Only this file knows the wire shape of
 * `axiomata_core::agent_start`; Rust resolves every engine and names every session.
 */

import { invokeBackend as invoke, type IdeAgent } from "../core/backend";
import type { EngineEntry, Role, Tier } from "../core/roster";

/** An engine as a tier lists it — the wire shape of `engines::EngineInfo` (an `EngineEntry` without the count). */
export type TierEngine = Omit<EngineEntry, "sessions">;

/** The engines of one tier in the order a start tries them; the first is the tier's standard. */
export interface TierEngines {
  tier: Tier;
  engines: TierEngine[];
}

/** A favorite: a role at a tier, on an engine or (`engine` absent) on the tier's default. */
export interface Favorite {
  role: string;
  tier: Tier;
  engine?: string | null;
}

/** A favorite as its tile shows it. Mirrors `agent_start::FavoriteView`. */
export interface FavoriteView extends Favorite {
  /** What a click starts on now; `null` when it cannot start. */
  engine_id: string | null;
  /** Why it cannot start now. */
  unavailable: string | null;
  name: string;
  title: string;
  subtitle: string;
}

export const listTierEngines = (): Promise<TierEngines[]> => invoke<TierEngines[]>("list_tier_engines");

export const listFavorites = (projectId: number): Promise<FavoriteView[]> =>
  invoke<FavoriteView[]>("list_favorites", { projectId });

/** Adds the favorite, or removes it when it is one; resolves with whether it is one now. */
export const toggleFavorite = (favorite: Favorite): Promise<boolean> =>
  invoke<boolean>("toggle_favorite", { favorite: { ...favorite, engine: favorite.engine ?? null } });

/** Starts `role` at `tier` on `engineId`, or on the tier's default engine when `null`. */
export const startAgentChoice = (
  projectId: number,
  role: string,
  tier: Tier,
  engineId: string | null,
): Promise<IdeAgent> => invoke<IdeAgent>("start_agent_choice", { projectId, role, tier, engineId });

/** The sessions of a project that hold work not taken over. */
export const agentsWithWork = (projectId: number): Promise<number[]> =>
  invoke<number[]>("ide_agents_with_work", { projectId });

/** The roles a start offers: every role in force but the master's, the seven fixed ones first in their order. */
export const FIXED_ROLES = ["allrounder", "implementer", "documenter", "planner", "reviewer", "tester", "grill"];

export function startableRoles(roles: readonly Role[]): Role[] {
  const rank = (name: string) => {
    const at = FIXED_ROLES.indexOf(name);
    return at === -1 ? FIXED_ROLES.length : at;
  };
  return roles
    .filter((role) => role.name !== "master")
    .slice()
    .sort((a, b) => rank(a.name) - rank(b.name) || a.name.localeCompare(b.name));
}

/** The engines of `tier`, the first (the standard) first. */
export function enginesOf(byTier: readonly TierEngines[], tier: Tier): TierEngine[] {
  return byTier.find((t) => t.tier === tier)?.engines ?? [];
}

/** Whether `favorites` holds exactly this choice (`engine` `null` = the tier's default). */
export function isFavorite(favorites: readonly Favorite[], choice: Favorite): boolean {
  return favorites.some(
    (f) => f.role === choice.role && f.tier === choice.tier && (f.engine ?? null) === (choice.engine ?? null),
  );
}

/** Which sessions the tab lists, and where (Q17). */
export interface SessionSections {
  /** Running or waiting: a pane is open, or the harness works or waits for an answer. Further masters head it. */
  active: IdeAgent[];
  /** Ended, with work not taken over: "Waiting for you: take over or discard". */
  waiting: IdeAgent[];
}

/**
 * A harness state that says the session runs without a pane open here. "idle" and "starting" do not: a pane closed with
 * the app never reports its end, so the last state stays "idle" for good — the status dot reads it as "closed" too
 * (`withoutPane`).
 */
const RUNNING = new Set(["working", "waiting"]);

/**
 * Splits the non-master sessions (and the further masters, which always head the list) into what the tab shows: an
 * ended session without work of its own is not listed — the weekly clean-up removes it.
 */
export function sessionSections(
  agents: readonly IdeAgent[],
  further: readonly IdeAgent[],
  isOpen: (agent: IdeAgent) => boolean,
  stateOf: (agent: IdeAgent) => string | undefined,
  withWork: ReadonlySet<number>,
): SessionSections {
  const active: IdeAgent[] = [...further];
  const waiting: IdeAgent[] = [];
  for (const agent of agents) {
    if (agent.agent_role === "master") continue;
    const state = stateOf(agent);
    if (isOpen(agent) || (state !== undefined && RUNNING.has(state))) active.push(agent);
    else if (withWork.has(agent.id)) waiting.push(agent);
  }
  return { active, waiting };
}

/** An engine as a pick list names it: "Claude · Haiku 5.5", "Glut · GLM 5.3"; an own engine by its label. */
export function engineChoiceLabel(engine: Pick<TierEngine, "origin" | "harness" | "model_name" | "label">): string {
  if (engine.origin === "auto" && engine.model_name) {
    return `${engine.harness === "claude_code" ? "Claude" : "Glut"} · ${engine.model_name}`;
  }
  return engine.label;
}

/** "Glut Medium Allrounder (Sonnet 5.5) #2": harness, tier, role, model and the number, as `agent_name` writes it. */
const SESSION_NAME = /^(Claude|Glut|Opencode) (Light|Medium|Heavy) (.+) \((.+)\)( #\d+)?$/;

/**
 * A session named by `agent_lines` read in two lines (Q8): "Allrounder · Medium #2" over its harness and model. The
 * second line follows the engine the session runs on **now** — after an escalation the name still says the old one
 * (the card's claim is made from it). `null` for a name of another shape.
 */
export function sessionLines(
  name: string,
  engine: Pick<TierEngine, "origin" | "harness" | "model_name" | "label"> | undefined,
): { title: string; subtitle: string } | null {
  const m = SESSION_NAME.exec(name);
  if (!m) return null;
  return {
    title: `${m[3]} · ${m[2]}${m[5] ?? ""}`,
    subtitle: engine ? engineChoiceLabel(engine) : `${m[1]} · ${m[4]}`,
  };
}
