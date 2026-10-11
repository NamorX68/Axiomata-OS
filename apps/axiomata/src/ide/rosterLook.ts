/**
 * What the Studio's engines, roles and agents are called and how they look, as plain data
 * (`docs/plans/studio-roster-ui.md`, R-2…R-5): derived ids, the name of an engine on an agent's card, the role chip's
 * tone, and which agent a role's "Start" chip opens. No Svelte, no DOM — the components only draw this.
 */

import type { Harness, IdeAgent } from "../core/backend";
import { MAX_IDENT_LEN, type EngineEntry, type Role } from "../core/roster";

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
  if (harness === "mini") return "Mini";
  if (harness === "opencode") return "Opencode";
  return harness;
}

/** Two letters for the avatar of an agent: the nicknames the owner uses ("CC", "OC"). */
export function harnessBadge(harness: Harness | string): string {
  if (harness === "claude_code") return "CC";
  if (harness === "opencode") return "OC";
  if (harness === "mini") return "M";
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

/** "reviewer-light" → "Reviewer light": an id read as a name. */
export function roleTitle(name: string): string {
  const text = name.replace(/[-_]+/g, " ").trim();
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : name;
}

/** The name of a new agent for a role: the role's title, with a number when the project already has that name. */
export function agentNameFor(role: string, existing: readonly string[]): string {
  const base = roleTitle(role);
  const used = new Set(existing.map((name) => name.trim().toLowerCase()));
  if (!used.has(base.toLowerCase())) return base;
  for (let n = 2; ; n++) {
    if (!used.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
  }
}

/**
 * The agent a role's "Start" chip opens: the newest agent of that role on that engine with no pane open (starting it
 * again continues its session in its own worktree); `null` when there is none and a new one is to be made. A second
 * chip click while it runs makes a second agent — two sessions of one role are a choice, not an accident.
 */
export function startableAgent(
  agents: readonly IdeAgent[],
  role: Pick<Role, "name" | "engine">,
  openIds: ReadonlySet<number>,
): IdeAgent | null {
  const fitting = agents.filter(
    (a) => a.agent_role === role.name && (role.engine === null || a.engine_id === role.engine) && !openIds.has(a.id),
  );
  if (fitting.length === 0) return null;
  // An unreadable date counts as the oldest, so the pick does not depend on the order of the list.
  const stamp = (a: IdeAgent) => Date.parse(a.updated_at) || 0;
  return fitting.reduce((newest, a) => (stamp(a) > stamp(newest) ? a : newest));
}

/** The kinds of work the Start chips are grouped under, in the order a person reads a project: plan first. */
const KIND_GROUPS: readonly { kind: string; title: string }[] = [
  { kind: "plan", title: "Planning" },
  { kind: "implement", title: "Implementation" },
  { kind: "review", title: "Review" },
  { kind: "test", title: "Testing" },
  { kind: "doc", title: "Documentation" },
];

/** Light before medium before heavy: the cheap role first, the expensive one last. */
const TIER_ORDER = ["light", "medium", "heavy"];

/** One heading of the Start chips and the roles under it. */
export interface RoleGroup {
  kind: string;
  title: string;
  roles: Role[];
}

/**
 * The roles of a project grouped by their kind of work, each group in the order of {@link KIND_GROUPS} (kinds it does
 * not know follow, alphabetically) and each role in a group by tier, then name. Empty groups are left out.
 */
export function groupRoles(roles: readonly Role[]): RoleGroup[] {
  const known = new Map(KIND_GROUPS.map((group, i) => [group.kind, i]));
  const byKind = new Map<string, Role[]>();
  for (const role of roles) byKind.set(role.kind, [...(byKind.get(role.kind) ?? []), role]);
  const rank = (role: Role) => {
    const at = TIER_ORDER.indexOf(role.tier);
    return at === -1 ? TIER_ORDER.length : at;
  };
  return [...byKind.entries()]
    .map(([kind, list]) => ({
      kind,
      title: KIND_GROUPS.find((group) => group.kind === kind)?.title ?? roleTitle(kind),
      roles: [...list].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name)),
    }))
    .sort((a, b) => {
      const [x, y] = [known.get(a.kind) ?? known.size, known.get(b.kind) ?? known.size];
      return x - y || a.kind.localeCompare(b.kind);
    });
}

