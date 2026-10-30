/**
 * What the Studio's engines, roles and agents are called and how they look, as plain data
 * (`docs/plans/studio-roster-ui.md`, R-2…R-5): derived ids, the name of an engine on an agent's card, the role chip's
 * tone, and which agent a role's "Start" chip opens. No Svelte, no DOM — the components only draw this.
 */

import type { Harness, IdeAgent } from "../core/backend";
import { MAX_IDENT_LEN, type EngineEntry } from "../core/roster";

/**
 * A free text as an id (`isSlug`'s shape): lower case, accents taken off, every run of other characters one `-`,
 * no `-` at either end, cut to the id length. May be empty — a text with no letter or digit has no id.
 */
export function slugify(text: string): string {
  const plain = text
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return plain.slice(0, MAX_IDENT_LEN).replace(/-+$/g, "");
}

/** `base`, or `base-2`, `base-3`… — the first `taken` lacks; the suffix never pushes it over the id length. */
export function uniqueSlug(base: string, taken: ReadonlySet<string> | readonly string[]): string {
  const used = taken instanceof Set ? taken : new Set(taken);
  if (!used.has(base)) return base;
  for (let n = 2; ; n++) {
    const suffix = `-${n}`;
    const candidate = `${base.slice(0, MAX_IDENT_LEN - suffix.length).replace(/-+$/g, "")}${suffix}`;
    if (!used.has(candidate)) return candidate;
  }
}

/** A role's id from the name typed (an empty one becomes `<kind>-<tier>`), clear of the ids already taken. */
export function roleIdFor(name: string, kind: string, tier: string, taken: readonly string[]): string {
  const base = slugify(name) || slugify(`${kind}-${tier}`) || "role";
  return uniqueSlug(base, taken);
}

/** An engine's id from its label (an empty one becomes harness + model), clear of the ids already taken. */
export function engineIdFor(
  label: string,
  harness: Harness,
  model: string | null,
  taken: readonly string[],
): string {
  const base = slugify(label) || slugify(`${harness}-${modelShort(model)}`) || "engine";
  return uniqueSlug(base, taken);
}

/** The harness as a person says it. */
export function harnessLabel(harness: Harness | string): string {
  if (harness === "claude_code") return "Claude Code";
  if (harness === "glut") return "Glut";
  if (harness === "opencode") return "Opencode";
  return harness;
}

/** Two letters for the avatar of an agent: the nicknames the owner uses ("CC", "OC"). */
export function harnessBadge(harness: Harness | string): string {
  if (harness === "claude_code") return "CC";
  if (harness === "opencode") return "OC";
  if (harness === "glut") return "GL";
  return harness.slice(0, 2).toUpperCase() || "?";
}

/** A model id without the provider path: `openrouter/deepseek/deepseek-v4.1-flash` → `deepseek-v4.1-flash`. */
export function modelShort(model: string | null | undefined): string {
  const text = (model ?? "").trim().replace(/\/+$/, "");
  if (!text) return "default model";
  return text.slice(text.lastIndexOf("/") + 1);
}

/** What an agent runs on, for its card: its engine's label, else harness and the short model of its own profile. */
export function engineName(
  agent: Pick<IdeAgent, "engine_id" | "harness" | "model">,
  engines: readonly Pick<EngineEntry, "id" | "label">[],
): string {
  const engine = agent.engine_id ? engines.find((e) => e.id === agent.engine_id) : undefined;
  if (engine) return engine.label;
  return `${harnessLabel(agent.harness)} · ${modelShort(agent.model)}`;
}

/** The tone of a role chip, from the role's kind of work. */
export type RoleTone = "plan" | "review" | "implement" | "other";

export function roleTone(kind: string | null | undefined): RoleTone {
  const k = (kind ?? "").toLowerCase();
  if (k === "plan" || k === "planner") return "plan";
  if (k === "review" || k === "reviewer") return "review";
  if (k === "implement" || k === "implementer" || k === "test" || k === "doc") return "implement";
  return "other";
}

/** A role's two letters on its hexagon in "Start an agent": "allrounder" → "AL", "grill" → "GR". */
export function roleBadge(name: string): string {
  const letters = name.replace(/[^a-z0-9]/gi, "");
  return (letters.slice(0, 2) || "?").toUpperCase();
}

/** "reviewer-light" → "Reviewer light": an id read as a name. */
export function roleTitle(name: string): string {
  const text = name.replace(/[-_]+/g, " ").trim();
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : name;
}
