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
  /** The context window in tokens, when the catalog says. */
  context_length: number | null;
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

/** Whether a tier research run is under way (the Models tab's quiet note). */
export const modelResearchRunning = (): Promise<boolean> => invoke<boolean>("model_research_running");

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

// ---- Pure helpers for the Models tab (AM3) ----

/** A provider list longer than this starts switched off and gets a filter field (`model_tiers::SMALL_LIST_MAX`). */
export const SMALL_LIST_MAX = 20;

/** The provider and the harness that runs its models, as the Models tab heads a group. */
export function providerHeading(provider: ProviderId): string {
  switch (provider) {
    case "anthropic":
      return "Anthropic · Claude Code";
    case "open_router":
      return "OpenRouter · Glut";
    case "ollama":
      return "Ollama · Glut";
  }
}

/** The models whose key or name holds `query` (any case); all of them for an empty query. */
export function filterModels(models: readonly ModelEntry[], query: string): ModelEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...models];
  return models.filter((m) => m.key.toLowerCase().includes(q) || m.name.toLowerCase().includes(q));
}

/** The owner's word for `model` with tier `tier` (`null` = none of the owner's), keeping its switch as it is. */
export function choiceWithTier(model: Pick<ModelEntry, "enabled" | "enabled_by_default">, tier: Tier | null): ModelChoice {
  return { enabled: model.enabled === model.enabled_by_default ? null : model.enabled, tier };
}

/** Where a model's tier comes from, in the words the Models tab shows. */
export function tierOrigin(model: Pick<ModelEntry, "tier_source" | "owner_tier" | "proposal">): string {
  switch (model.tier_source) {
    case "owner":
      return model.proposal && model.proposal.tier !== model.owner_tier
        ? `Yours · the proposal says ${model.proposal.tier}`
        : "Yours";
    case "research":
      return "Proposal · not reviewed";
    case "price":
      return "From the price";
    case "unrated":
      return "Unrated · no agent until it has a tier";
  }
}

/** Whether a proposal waits for the owner's one click: research spoke and the owner has not. */
export function proposalWaits(model: Pick<ModelEntry, "tier_source" | "proposal">): boolean {
  return model.tier_source === "research" && model.proposal !== null;
}

/** "$0.40 / M out", or "local" / "no price". */
export function priceLabel(model: Pick<ModelEntry, "provider" | "price">): string {
  if (model.provider === "ollama") return "local";
  if (!model.price) return "no price";
  const out = model.price.output_per_m;
  return `$${out < 1 ? out.toFixed(2) : String(Math.round(out * 100) / 100)} / M out`;
}

/** A context window as "128K" or "1M" tokens; empty when unknown. */
export function contextLabel(tokens: number | null | undefined): string {
  if (!tokens) return "";
  if (tokens >= 1_000_000) return `${Math.round((tokens / 1_000_000) * 10) / 10}M context`;
  return `${Math.round(tokens / 1000)}K context`;
}

/** The host a source link points at ("lmarena.ai"), for its text; the text itself when it is no URL. */
export function sourceHost(source: string): string {
  try {
    return new URL(source).host.replace(/^www\./, "");
  } catch {
    return source;
  }
}

/** Whether `source` may be opened in the system browser: an http(s) address and nothing else. */
export function isWebAddress(source: string): boolean {
  try {
    const url = new URL(source);
    return url.protocol === "https:" || url.protocol === "http:";
  } catch {
    return false;
  }
}

/** What is wrong with price limits typed into the form, or `null` (looser or equal to Rust's check). */
export function limitsProblem(lightBelow: number, heavyAbove: number): string | null {
  if (!Number.isFinite(lightBelow) || !Number.isFinite(heavyAbove) || lightBelow < 0 || heavyAbove < 0) {
    return "Both limits are prices in USD per million output tokens.";
  }
  if (lightBelow > heavyAbove) return "“Light below” must not be above “heavy above”.";
  return null;
}

/** An engine as the default-engine select needs it. */
export interface TierEngineOption {
  id: string;
  label: string;
  rated_tier: Tier | null;
  output_per_m: number | null;
  unusable: string | null;
}

/** The engines a tier's default may be: the usable ones of that tier, the cheapest first (like `engines::engine_for`). */
export function engineOptionsFor<T extends TierEngineOption>(engines: readonly T[], tier: Tier): T[] {
  return engines
    .filter((e) => e.rated_tier === tier && e.unusable === null)
    .sort((a, b) => (a.output_per_m ?? Infinity) - (b.output_per_m ?? Infinity) || a.id.localeCompare(b.id));
}

/**
 * The price limits the form shows after a reload: what is stored, unless the owner is typing — then what they typed
 * stays, so a reload caused by another change never throws their edit away.
 */
export function limitsAfterReload(
  typed: { lightBelow: string; heavyAbove: string },
  stored: TierLimits,
  editing: boolean,
): { lightBelow: string; heavyAbove: string } {
  return editing ? typed : { lightBelow: String(stored.light_below), heavyAbove: String(stored.heavy_above) };
}

/**
 * Hands out numbers to requests and says whether an answer is the newest — so an older reload that answers late never
 * overwrites a newer one.
 */
export class Latest {
  private last = 0;

  /** The number of a request about to be sent. */
  next(): number {
    this.last += 1;
    return this.last;
  }

  /** Whether `ticket`'s answer is still the one to show. */
  current(ticket: number): boolean {
    return ticket === this.last;
  }
}

/** `busy` with `key` added — each row waits on its own change, not on another row's. */
export function withBusy(busy: ReadonlySet<string>, key: string): Set<string> {
  return new Set([...busy, key]);
}

/** `busy` without `key`. */
export function withoutBusy(busy: ReadonlySet<string>, key: string): Set<string> {
  const next = new Set(busy);
  next.delete(key);
  return next;
}

/** What "propose again" says when it was answered: nothing was asked for, one model, or several. */
export function researchToast(asked: number): string {
  if (asked === 0) return "No model is switched on to research.";
  return asked === 1 ? "Researching the model's tier…" : `Researching the tiers of ${asked} models…`;
}
