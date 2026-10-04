/**
 * The Studio's roster, frontend side (`docs/plans/a2a.md`, CP-A1): the owner's **engines** (harness + model +
 * environment) and **roles** (`AGENT.md` files: tier, engine, limits, instructions).
 *
 * Only this file knows the wire shape. Rust is authoritative for validation; the checks here exist so a form can
 * point at a field before the round trip, and they must stay *looser or equal* — never stricter — than
 * `axiomata-roster`.
 */

import { invokeBackend as invoke, type Harness } from "./backend";

export type Billing = "metered" | "subscription";

/** One entry of the engine catalog. Mirrors `axiomata_roster::Engine`. */
export interface Engine {
  id: string;
  label: string;
  harness: Harness;
  /** Empty = the harness's own default command. */
  command: string;
  /** For Opencode the full `provider/model` id. */
  model: string | null;
  /** `KEY=value` per line. */
  env: string;
  billing: Billing;
}

/** An engine with the number of agent sessions that run on it. */
export interface EngineEntry extends Engine {
  sessions: number;
}

export type Tier = "light" | "medium" | "heavy";
export type RoleSource = "user" | "project";

export interface Limits {
  max_cost_usd?: number | null;
  max_tokens?: number | null;
  max_steps?: number | null;
}

/** One role. Mirrors `axiomata_roster::Role`. */
export interface Role {
  name: string;
  description: string;
  kind: string;
  tier: Tier;
  /** `null` = the owner picks the engine at every start (the planner). */
  engine: string | null;
  fallback_engines: string[];
  permissions: string[];
  limits: Limits;
  creates: string[];
  instructions: string;
  source: RoleSource;
}

export interface SkippedRole {
  name: string;
  reason: string;
}

export interface LoadedRoles {
  roles: Role[];
  skipped: SkippedRole[];
}

export interface ProjectRoles {
  overrides: {
    dir: string;
    present: boolean;
    hash: string;
    roles: Role[];
    skipped: SkippedRole[];
    /** Why these files can never be confirmed (a symlinked directory, more files than are read), else `null`. */
    blocked: string | null;
  };
  confirmed: boolean;
  effective: Role[];
  skipped: SkippedRole[];
  /** Names of the owner's own roles the project's files would replace. */
  replaces: string[];
  /** Engine ids the project's roles name that the owner's catalog does not have. */
  unknown_engines: string[];
}

export const listEngines = (): Promise<EngineEntry[]> => invoke<EngineEntry[]>("list_engines");
export const saveEngine = (engine: Engine): Promise<void> => invoke<void>("save_engine", { engine });
export const deleteEngine = (id: string): Promise<boolean> => invoke<boolean>("delete_engine", { id });

export const listRoles = (): Promise<LoadedRoles> => invoke<LoadedRoles>("list_roles");
export const saveRole = (role: Role): Promise<void> => invoke<void>("save_role", { role });
export const deleteRole = (name: string): Promise<boolean> => invoke<boolean>("delete_role", { name });

/** The roles in force for a project, and what the project itself brings. */
export const projectRoles = (projectId: number): Promise<ProjectRoles> =>
  invoke<ProjectRoles>("project_roles", { projectId });

/** The owner confirmed the project's role files as shown (`hash` is what was shown). */
export const confirmProjectRoles = (projectId: number, hash: string): Promise<void> =>
  invoke<void>("confirm_project_roles", { projectId, hash });

// ---- Pure helpers for the forms ----

export const TIERS: { id: Tier; label: string; hint: string }[] = [
  { id: "light", label: "Light", hint: "Small, well-specified changes" },
  { id: "medium", label: "Medium", hint: "The normal case" },
  { id: "heavy", label: "Heavy", hint: "Large or delicate changes; where escalation ends" },
];

export const BILLINGS: { id: Billing; label: string; hint: string }[] = [
  { id: "metered", label: "Per token", hint: "Limits in dollars" },
  { id: "subscription", label: "Subscription", hint: "Limits in tokens (Claude Code on the account)" },
];

/** The longest id the Rust side accepts. */
export const MAX_IDENT_LEN = 48;

const SLUG = /^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$/;

/** Whether `value` is a valid slug (lower-case letters/digits, `-` or `_` inside). */
export function isSlug(value: string): boolean {
  return value.length > 0 && value.length <= MAX_IDENT_LEN && SLUG.test(value);
}

/** `"a, b\nc"` → `["a","b","c"]`; blanks and duplicates removed, order kept. */
export function splitList(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of text.split(/[,\n]/)) {
    const item = part.trim();
    if (item && !seen.has(item)) {
      seen.add(item);
      out.push(item);
    }
  }
  return out;
}

/** Permission rules are free text that may contain commas, so they split on lines only. */
export function splitLines(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

export function blankEngine(): Engine {
  return { id: "", label: "", harness: "opencode", command: "", model: null, env: "", billing: "metered" };
}

export type FormResult<T> = { ok: true; value: T } | { ok: false; field: string; message: string };

const fail = (field: string, message: string): FormResult<never> => ({ ok: false, field, message });

/** Trims an engine form and checks what can be checked without the catalog. */
export function checkEngine(form: Engine): FormResult<Engine> {
  const id = form.id.trim();
  if (!isSlug(id)) return fail("id", "ID: lower-case letters and digits, with - or _ between them (up to 48 characters).");
  const label = form.label.trim();
  if (!label) return fail("label", "Label must not be empty.");
  if ([...label].length > 80) return fail("label", "Label: up to 80 characters.");
  for (const line of form.env.split("\n").filter((l) => l.trim())) {
    const key = line.split("=")[0]?.trim() ?? "";
    if (!line.includes("=") || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      return fail("env", `Environment: “${line.trim()}” is not KEY=value.`);
    }
  }
  const model = form.model?.trim() ? form.model.trim() : null;
  return { ok: true, value: { ...form, id, label, command: form.command.trim(), model } };
}

/** A role as the form edits it: lists and numbers as text. */
export interface RoleForm {
  name: string;
  description: string;
  kind: string;
  tier: Tier;
  /** `""` = chosen at every start. */
  engine: string;
  fallback: string;
  permissions: string;
  creates: string;
  maxCost: string;
  maxTokens: string;
  maxSteps: string;
  instructions: string;
}

export function blankRoleForm(): RoleForm {
  return {
    name: "",
    description: "",
    kind: "implement",
    tier: "medium",
    engine: "",
    fallback: "",
    permissions: "",
    creates: "",
    maxCost: "",
    maxTokens: "",
    maxSteps: "",
    instructions: "",
  };
}

export function roleToForm(role: Role): RoleForm {
  return {
    name: role.name,
    description: role.description,
    kind: role.kind,
    tier: role.tier,
    engine: role.engine ?? "",
    fallback: role.fallback_engines.join(", "),
    permissions: role.permissions.join("\n"),
    creates: role.creates.join(", "),
    maxCost: role.limits.max_cost_usd == null ? "" : String(role.limits.max_cost_usd),
    maxTokens: role.limits.max_tokens == null ? "" : String(role.limits.max_tokens),
    maxSteps: role.limits.max_steps == null ? "" : String(role.limits.max_steps),
    instructions: role.instructions,
  };
}

/** `""` → `null`; a positive number (integer when `integer`) → the number; anything else → `undefined`. */
function parseLimit(text: string, integer: boolean): number | null | undefined {
  const t = text.trim();
  if (!t) return null;
  const n = Number(t);
  if (!Number.isFinite(n) || n <= 0) return undefined;
  if (integer && !Number.isInteger(n)) return undefined;
  return n;
}

/** Builds the role a form describes, or the first problem. `engines` are the known ids. */
export function checkRoleForm(form: RoleForm, engines: string[]): FormResult<Role> {
  const name = form.name.trim();
  if (!isSlug(name)) return fail("name", "Name: lower-case letters and digits, with - or _ between them (up to 48 characters).");
  const kind = form.kind.trim();
  if (!isSlug(kind)) return fail("kind", "Kind: one lower-case word, e.g. implement, review, plan.");
  const description = form.description.trim();
  if (description.includes("\n") || description.length > 200) {
    return fail("description", "Description: one line, up to 200 characters.");
  }
  const engine = form.engine.trim();
  const fallback = splitList(form.fallback);
  const known = new Set(engines);
  if (engine && !known.has(engine)) return fail("engine", `There is no engine “${engine}”.`);
  for (const id of fallback) {
    if (!known.has(id)) return fail("fallback", `There is no engine “${id}”.`);
    if (id === engine) return fail("fallback", `“${id}” is already the default engine.`);
  }
  const creates = splitList(form.creates);
  const badKind = creates.find((k) => !isSlug(k));
  if (badKind) return fail("creates", `Card kind “${badKind}”: one lower-case word.`);
  const cost = parseLimit(form.maxCost, false);
  if (cost === undefined) return fail("maxCost", "Cost limit: a positive number or empty.");
  const tokens = parseLimit(form.maxTokens, true);
  if (tokens === undefined) return fail("maxTokens", "Token limit: a positive whole number or empty.");
  const steps = parseLimit(form.maxSteps, true);
  if (steps === undefined) return fail("maxSteps", "Step limit: a positive whole number or empty.");
  return {
    ok: true,
    value: {
      name,
      description,
      kind,
      tier: form.tier,
      engine: engine || null,
      fallback_engines: fallback,
      permissions: splitLines(form.permissions),
      limits: { max_cost_usd: cost, max_tokens: tokens, max_steps: steps },
      creates,
      instructions: form.instructions.trim(),
      source: "user",
    },
  };
}
