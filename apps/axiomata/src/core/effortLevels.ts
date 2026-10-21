/**
 * Which effort levels a model takes (`docs/plans/glut-effort.md`, F1–F4): the catalog's `reasoning` of a model,
 * turned into what the chat's effort switch, `/effort` and the Settings selects offer. A model the catalog says
 * nothing about gets every level — the provider takes a near one where it knows fewer.
 */

import { EFFORT_LEVELS, type EffortLevel, type ProviderId } from "./backend";
import type { ModelCatalog, ModelReasoning } from "./modelCatalog";

/** The catalog's word on `model` of `provider`; `undefined` when the catalog does not know it or its levels. */
export function reasoningOf(
  catalog: ModelCatalog | null,
  provider: ProviderId,
  model: string,
): ModelReasoning | undefined {
  return catalog?.providers[provider]?.models.find((m) => m.id === model)?.reasoning;
}

/** The levels to offer, the least first: the model's own, else all of them. */
export function levelsFor(reasoning: ModelReasoning | undefined): readonly EffortLevel[] {
  return reasoning ? EFFORT_LEVELS.filter((level) => reasoning.efforts.includes(level)) : EFFORT_LEVELS;
}

/** What a picker offers: the model's levels, and `chosen` among them when it is set but unlisted — a select must
 *  be able to show what is stored. The least first. */
export function offeredLevels(reasoning: ModelReasoning | undefined, chosen: EffortLevel | null): EffortLevel[] {
  const own = levelsFor(reasoning);
  return EFFORT_LEVELS.filter((level) => own.includes(level) || level === chosen);
}

/** Whether `level` is one the model names; with no word from the catalog, every level is. */
export function takesLevel(reasoning: ModelReasoning | undefined, level: EffortLevel): boolean {
  return !reasoning || reasoning.efforts.includes(level);
}

/** `default (high)` — what "no level" means for this model, as a picker's first entry reads. */
export function defaultLabel(reasoning: ModelReasoning | undefined): string {
  return reasoning?.default_effort ? `default (${reasoning.default_effort})` : "default";
}

/** Where the level of the next chat turn comes from. */
export type EffortSource = "conversation" | "settings" | "default";

/** The level the chat runs at as far as the app can see it, and where it comes from. */
export interface EffortState {
  /** The level sent; `null` when the app sends none (Glut's own setting or the model's default). */
  level: EffortLevel | null;
  source: EffortSource;
  /** One line for a tooltip or `/effort`'s head. */
  explanation: string;
}

/** The chat's effort: this conversation's choice over the configured chat effort over Glut's own and the model's. */
export function effortState(
  turn: EffortLevel | null,
  configured: EffortLevel | null,
  reasoning: ModelReasoning | undefined,
): EffortState {
  const fallback = reasoning?.default_effort
    ? `Glut's own setting, else the model's default (${reasoning.default_effort})`
    : "Glut's own setting, else the provider's default";
  if (turn) {
    const settings = configured ? `the configured ${configured}` : fallback;
    const explanation = `${turn} for this conversation — default: ${settings}`;
    return { level: turn, source: "conversation", explanation };
  }
  if (configured) {
    return { level: configured, source: "settings", explanation: `${configured}, the configured chat effort` };
  }
  return { level: null, source: "default", explanation: fallback };
}

/** Said beside a level the model does not name: OpenRouter takes a near one, another provider may refuse it. */
export function unlistedNote(reasoning: ModelReasoning | undefined, level: EffortLevel | null): string | null {
  if (!level || takesLevel(reasoning, level)) return null;
  return `the model does not list ${level} — the provider takes a near level or refuses it`;
}
