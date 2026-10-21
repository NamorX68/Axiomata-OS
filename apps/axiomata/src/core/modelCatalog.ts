/**
 * The providers' model catalogs (board card #17), mirrored from `axiomata_core::model_catalog`: what the model
 * pickers offer. Rust refreshes the catalog at start and once a day and announces it on
 * `models:catalog-updated`; this module keeps the last one in a store and turns it into picker options.
 */

import { writable } from "svelte/store";

import { type EffortLevel, type ProviderId, invokeBackend, listenBackend } from "./backend";

/** Mirrors `axiomata_core::config::ModelCost`, USD per million tokens. */
export interface ModelPrice {
  input_per_m: number;
  cached_input_per_m?: number;
  output_per_m: number;
}

/** Mirrors `axiomata_core::model_catalog::ModelReasoning`: the effort levels a reasoning model takes. */
export interface ModelReasoning {
  /** The least first, never empty. */
  efforts: EffortLevel[];
  default_effort?: EffortLevel;
  /** Its reasoning cannot be turned off. */
  mandatory: boolean;
}

export interface CatalogModel {
  id: string;
  name: string;
  context_length?: number;
  price?: ModelPrice;
  /** Absent when the provider does not say which levels the model takes. */
  reasoning?: ModelReasoning;
}

export interface ProviderCatalog {
  models: CatalogModel[];
  fetched_at: string | null;
  error: string | null;
}

export interface ModelCatalog {
  version: number;
  providers: Partial<Record<ProviderId, ProviderCatalog>>;
}

/** Mirrors `axiomata_core::model_catalog::RefreshOutcome`. */
export interface RefreshOutcome {
  catalog: ModelCatalog;
  repriced: string[];
  price_error: string | null;
}

/** One entry of a model picker. */
export interface ModelOption {
  /** What is stored when it is picked. */
  value: string;
  /** The readable name, shown beside the value when it says more. */
  label: string;
  /** A short second line: the price, the context length. */
  hint: string;
}

export const MODEL_CATALOG_UPDATED = "models:catalog-updated";

export const modelCatalog = writable<ModelCatalog | null>(null);

let subscribed = false;

/** Loads the stored catalog once and follows every refresh after it. */
export async function loadModelCatalog(): Promise<void> {
  if (!subscribed) {
    subscribed = true;
    await listenBackend<RefreshOutcome>(MODEL_CATALOG_UPDATED, (outcome) => modelCatalog.set(outcome.catalog));
  }
  modelCatalog.set(await invokeBackend<ModelCatalog>("get_model_catalog"));
}

/** Fetches one provider's list again (every provider without one) and writes the new prices. */
export async function refreshModelCatalog(provider?: ProviderId): Promise<RefreshOutcome> {
  const outcome = await invokeBackend<RefreshOutcome>("refresh_model_catalog", { provider: provider ?? null });
  modelCatalog.set(outcome.catalog);
  return outcome;
}

/** `$0.15 / $0.50 per M` — the price as the picker's second line shows it; "free" for a zero price. */
export function priceHint(price: ModelPrice | undefined): string {
  if (!price) return "";
  if (price.input_per_m === 0 && price.output_per_m === 0) return "free";
  const usd = (n: number) => `$${Number(n.toPrecision(3))}`;
  return `${usd(price.input_per_m)} in / ${usd(price.output_per_m)} out per M`;
}

function hintFor(model: CatalogModel): string {
  const context = model.context_length ? `${Math.round(model.context_length / 1000)}k ctx` : "";
  return [priceHint(model.price), context].filter(Boolean).join(" · ");
}

function optionsOf(catalog: ModelCatalog | null, provider: ProviderId, prefix = ""): ModelOption[] {
  return (catalog?.providers[provider]?.models ?? []).map((m) => ({
    value: prefix + m.id,
    label: m.name !== m.id ? m.name : "",
    hint: hintFor(m),
  }));
}

/** The models a provider's own settings take (bare ids). */
export function providerOptions(catalog: ModelCatalog | null, provider: ProviderId): ModelOption[] {
  return optionsOf(catalog, provider);
}

/** Opencode's name of each provider, the prefix of an Opencode engine's `provider/model`. */
const OPENCODE_PREFIX: Record<ProviderId, string> = {
  anthropic: "anthropic/",
  open_router: "openrouter/",
  ollama: "ollama/",
};

/**
 * The models an engine of `harness` takes: Claude Code the bare Anthropic ids, Glut the bare ids of the providers it
 * speaks to (OpenRouter, Ollama — the chat provider's), Opencode `provider/model`.
 */
export function engineOptions(catalog: ModelCatalog | null, harness: string): ModelOption[] {
  if (harness === "claude_code") return optionsOf(catalog, "anthropic");
  if (harness === "glut") return [...optionsOf(catalog, "open_router"), ...optionsOf(catalog, "ollama")];
  return (Object.keys(OPENCODE_PREFIX) as ProviderId[]).flatMap((p) => optionsOf(catalog, p, OPENCODE_PREFIX[p]));
}

/** The most entries a picker draws at once; the owner narrows a 470-model list by typing, not by scrolling. */
export const MAX_SHOWN = 80;

/** The options matching `query`: every whitespace-separated word must occur in the value or the label. */
export function filterOptions(options: ModelOption[], query: string): ModelOption[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = words.length
    ? options.filter((o) => {
        const text = `${o.value} ${o.label}`.toLowerCase();
        return words.every((w) => text.includes(w));
      })
    : options;
  return hits.slice(0, MAX_SHOWN);
}
