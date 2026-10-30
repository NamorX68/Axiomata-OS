/**
 * The Studio's models, tiers and computed agents, frontend side (`docs/plans/agents-model.md`, AM1): which models are
 * switched on, which tier each one has and where that comes from, the price limits, the default engine per tier, and
 * the agents (role × tier × engine) a project can start.
 *
 * Only this file knows the wire shape (`axiomata_core::model_tiers`, `axiomata_core::agent_choice`). The Models tab
 * (AM3) and the Agents tab (AM4) build on it; Rust validates every change.
 */

import { invokeBackend as invoke, type ProviderId } from "../core/backend";
import type { Tier, TierSource } from "../core/roster";

export type { TierSource };

/** A price in USD per million tokens. */
export interface ModelPrice {
  input_per_m: number;
  cached_input_per_m?: number | null;
  output_per_m: number;
}

/** The app's research proposal for a model's tier (never the owner's value). */
export interface TierProposal {
  tier: Tier;
  reason: string;
  source: string;
  /** RFC 3339. */
  at: string;
}

/** One model as the Models tab shows it. Mirrors `model_tiers::ModelEntry`. */
export interface ModelEntry {
  /** `anthropic/…`, `openrouter/…` or `ollama/…`. */
  key: string;
  provider: ProviderId;
  id: string;
  name: string;
  short_name: string;
  enabled: boolean;
  enabled_by_default: boolean;
  tier: Tier | null;
  tier_source: TierSource;
  owner_tier: Tier | null;
  proposal: TierProposal | null;
  price_tier: Tier | null;
  price: ModelPrice | null;
}

/** The price limits of the price proposal, USD per million output tokens. */
export interface TierLimits {
  light_below: number;
  heavy_above: number;
}

/** The owner's default engine per tier; absent = the cheapest of the tier. */
export interface TierEngines {
  light?: string | null;
  medium?: string | null;
  heavy?: string | null;
}

export interface ModelsOverview {
  models: ModelEntry[];
  tier_limits: TierLimits;
  default_engines: TierEngines;
}

/** The owner's word on one model; both absent = the defaults. */
export interface ModelChoice {
  enabled?: boolean | null;
  tier?: Tier | null;
}

/** One agent a project can start. Mirrors `agent_choice::AgentChoice`. */
export interface AgentChoice {
  role: string;
  tier: Tier;
  engine_id: string;
  /** The tier's first engine: the owner's default, else the cheapest. */
  standard: boolean;
  /** "Glut Medium Allrounder (Sonnet 5.5)". */
  name: string;
  /** "Allrounder · Medium". */
  title: string;
  /** "Glut · Sonnet 5.5". */
  subtitle: string;
}

export const listModels = (): Promise<ModelsOverview> => invoke<ModelsOverview>("list_models");

export const setModelChoice = (key: string, choice: ModelChoice): Promise<void> =>
  invoke<void>("set_model_choice", { key, choice });

export const setTierLimits = (limits: TierLimits): Promise<void> => invoke<void>("set_tier_limits", { limits });

/** `engine: null` clears the default (the cheapest of the tier). */
export const setDefaultEngine = (tier: Tier, engine: string | null): Promise<void> =>
  invoke<void>("set_default_engine", { tier, engine });

export const listAgentChoices = (projectId: number): Promise<AgentChoice[]> =>
  invoke<AgentChoice[]>("list_agent_choices", { projectId });

/**
 * "Propose again" (AM2): forgets the research proposals of `keys` (every switched-on model when `null`) and starts the
 * `model-tiering` skill for them. Resolves with the keys asked for; the run ends with {@link MODEL_TIERS_UPDATED}.
 */
export const researchModelTiers = (keys: string[] | null): Promise<string[]> =>
  invoke<string[]>("research_model_tiers", { keys });

/** The event a finished tier research run sends. Mirrors `model_research::MODEL_TIERS_UPDATED` (src-tauri). */
export const MODEL_TIERS_UPDATED = "models:tiers-updated";

/** The providers in the order the Models tab lists them. */
export const PROVIDER_ORDER: readonly ProviderId[] = ["anthropic", "open_router", "ollama"];

/** The models grouped by provider in {@link PROVIDER_ORDER}, each in the catalog's order; empty groups dropped. */
export function modelsByProvider(models: readonly ModelEntry[]): { provider: ProviderId; models: ModelEntry[] }[] {
  return PROVIDER_ORDER.map((provider) => ({ provider, models: models.filter((m) => m.provider === provider) })).filter(
    (group) => group.models.length > 0,
  );
}

/** The choice that toggles a model's switch, keeping the owner's tier. Back at the default it says nothing. */
export function toggledChoice(model: Pick<ModelEntry, "enabled" | "enabled_by_default" | "owner_tier">): ModelChoice {
  const enabled = !model.enabled;
  return {
    enabled: enabled === model.enabled_by_default ? null : enabled,
    tier: model.owner_tier,
  };
}
