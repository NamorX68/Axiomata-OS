/**
 * The module registry: every module type registers here once at startup
 * (`src/modules/index.ts`). The shell looks types up to render the picker and
 * mount tiles; the agent bridge (step 12) uses `manifest()` and `invokeAction()`.
 */

import { get, writable } from "svelte/store";

import { invokeBackend } from "./backend";
import type { ModuleAction, ModuleContext, ModuleDefinition, CanvasInstance } from "./types";
import { instances, updateInstance } from "./stores";
import { emit } from "./bus";
import { askOwner } from "./chatAsks";

const registry = new Map<string, ModuleDefinition>();

export function registerModule(def: ModuleDefinition): void {
  if (registry.has(def.type)) {
    throw new Error(`module type "${def.type}" is already registered`);
  }
  registry.set(def.type, def);
}

export function getModule(type: string): ModuleDefinition | undefined {
  return registry.get(type);
}

export function listModules(): ModuleDefinition[] {
  return [...registry.values()];
}

/** A module's name as the owner reads it, in the current language — call it where it is drawn, never keep it. */
export function moduleTitle(def: Pick<ModuleDefinition, "title" | "label">): string {
  return def.label?.() ?? def.title;
}

/**
 * The shell's own actions, reached like a module instance's under the id
 * `shell` — for what belongs to no tile: opening a file for the owner in the
 * file panel (`docs/plans/editor.md`, ED4, W2), which no longer has a tile of
 * its own to carry the action.
 */
export const SHELL_INSTANCE = "shell";
const shellActions: ModuleAction[] = [];

export function registerShellAction(action: ModuleAction): void {
  if (shellActions.some((a) => a.name === action.name)) {
    throw new Error(`shell action "${action.name}" is already registered`);
  }
  shellActions.push(action);
}

/** One entry per mounted instance that declares at least one action. This is
 *  the exact payload rendered into `~/.axiomata/module-context.md` for the
 *  agent (step 12). */
export interface ManifestEntry {
  instance_id: string;
  type: string;
  title: string;
  actions: { name: string; description: string; params: unknown }[];
}

export function manifest(): ManifestEntry[] {
  const offered = shellActions.filter(isOffered);
  const shell: ManifestEntry[] = offered.length
    ? [{ instance_id: SHELL_INSTANCE, type: "shell", title: "Shell", actions: offered.map(describe) }]
    : [];
  return shell.concat(moduleEntries());
}

/** Whether an agent is shown the action at all: a `never` one is the owner's alone (CH5). */
const isOffered = (a: ModuleAction): boolean => a.chat === "direct" || a.chat === "ask" || a.chat === "guarded";

/** Said after an `ask` action's description, so the agent knows its call only asks the owner. */
export const ASKS_SUFFIX = " Asks the owner in the app first; it runs only if they say yes.";

/** Said after a `guarded` action's description: it acts at once only when nothing would be lost. */
export const GUARDED_SUFFIX =
  " Acts at once when nothing would be lost; otherwise it asks the owner in the app, saying what, and acts only if " +
  "they say yes.";

function describe(a: ModuleAction): ManifestEntry["actions"][number] {
  const suffix = a.chat === "ask" ? ASKS_SUFFIX : a.chat === "guarded" ? GUARDED_SUFFIX : "";
  const description = `${a.description}${suffix}`;
  return { name: a.name, description, params: a.params };
}

function moduleEntries(): ManifestEntry[] {
  return get(instances).flatMap((inst): ManifestEntry[] => {
    const actions = (registry.get(inst.type)?.actions ?? []).filter(isOffered);
    if (!actions.length) return [];
    return [
      {
        instance_id: inst.id,
        type: inst.type,
        title: registry.get(inst.type)!.title,
        actions: actions.map(describe),
      },
    ];
  });
}

/** Builds a module context. `onConfig` receives every config change after
 *  the initial value; `onResize` a requested tile size. */
export function createContext(
  instanceId: string,
  initialConfig: Record<string, unknown>,
  onConfig: (config: Record<string, unknown>) => void,
  onResize: (size: { w: number; h: number }) => void = () => {},
): ModuleContext {
  const config = writable<Record<string, unknown>>({ ...initialConfig });
  let first = true;
  config.subscribe((value) => {
    if (first) {
      first = false;
      return;
    }
    onConfig(value);
  });
  return { instanceId, config, invoke: invokeBackend, emit, requestResize: onResize };
}

/** The context a canvas instance is mounted with: config changes and resize
 *  requests flow back into the store (and from there to dashboard.json). */
export function makeContext(inst: CanvasInstance): ModuleContext {
  return createContext(
    inst.id,
    inst.config,
    (config) => updateInstance(inst.id, { config }),
    (size) => updateInstance(inst.id, { w: size.w, h: size.h }),
  );
}

/** One action found by instance and name, with the context it runs in. */
function resolveAction(instanceId: string, action: string): { act: ModuleAction; ctx: ModuleContext } {
  if (instanceId === SHELL_INSTANCE) {
    const act = shellActions.find((a) => a.name === action);
    if (!act) throw new Error(`the shell has no action "${action}"`);
    return { act, ctx: createContext(SHELL_INSTANCE, {}, () => {}) };
  }
  const inst = get(instances).find((i) => i.id === instanceId);
  if (!inst) {
    throw new Error(`no module instance "${instanceId}"`);
  }
  const def = registry.get(inst.type);
  const act = def?.actions?.find((a) => a.name === action);
  if (!act) {
    throw new Error(
      `instance "${instanceId}" (${inst.type}) has no action "${action}"`,
    );
  }
  return { act, ctx: makeContext(inst) };
}

/** Dispatch one declared action on one mounted instance (frontend → frontend), as the owner: the `/` command
 *  router. An agent's call goes through {@link invokeAgentAction}. */
export async function invokeAction(
  instanceId: string,
  action: string,
  params: unknown,
): Promise<unknown> {
  const { act, ctx } = resolveAction(instanceId, action);
  return act.run(params, ctx);
}

/**
 * Dispatch an action an agent asked for through the bridge (`core/agent-bridge.ts`) by its `chat` mark (CH5, Q8):
 * `direct`, it runs; `ask`, the owner is asked in the app and the agent hears at once that it was asked — it runs
 * only on the owner's yes, with the params shown then; `guarded`, it runs at once when its `ask` finds nothing to ask,
 * else as `ask`; anything else is refused. `origin` is who the request says it
 * comes from (`chat`, `agent:<name>`, …), shown with the question.
 */
export async function invokeAgentAction(
  instanceId: string,
  action: string,
  params: unknown,
  origin: string | null = null,
): Promise<unknown> {
  const { act, ctx } = resolveAction(instanceId, action);
  switch (act.chat) {
    case "direct":
      return act.run(params, ctx);
    case "ask":
    case "guarded": {
      if (!act.ask) throw new Error(`"${action}" cannot be asked for`);
      const prepared = await act.ask(params, ctx);
      if (!prepared) {
        // Only a guarded action may find nothing to ask; an ask action that does is refused rather than run.
        if (act.chat === "guarded") return act.run(params, ctx);
        throw new Error(`"${action}" found nothing to ask; it does not run without the owner's yes`);
      }
      const frozen = prepared.params ?? params;
      return askOwner({ ...prepared, source: action, origin }, () => act.run(frozen, ctx));
    }
    default:
      // `never`, or a mark this build does not know: refused.
      throw new Error(`"${action}" is the owner's to do in the app; an agent does not do it`);
  }
}
