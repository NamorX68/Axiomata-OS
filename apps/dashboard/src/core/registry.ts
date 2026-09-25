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
  const shell: ManifestEntry[] = shellActions.length
    ? [{ instance_id: SHELL_INSTANCE, type: "shell", title: "Shell", actions: shellActions.map(describe) }]
    : [];
  return shell.concat(moduleEntries());
}

function describe(a: ModuleAction): ManifestEntry["actions"][number] {
  return { name: a.name, description: a.description, params: a.params };
}

function moduleEntries(): ManifestEntry[] {
  return get(instances).flatMap((inst): ManifestEntry[] => {
    const def = registry.get(inst.type);
    if (!def?.actions?.length) return [];
    return [
      {
        instance_id: inst.id,
        type: inst.type,
        title: def.title,
        actions: def.actions.map(describe),
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

/** Dispatch one declared action on one mounted instance (frontend → frontend).
 *  Used by the `/` command router and the agent bridge. */
export async function invokeAction(
  instanceId: string,
  action: string,
  params: unknown,
): Promise<unknown> {
  if (instanceId === SHELL_INSTANCE) {
    const act = shellActions.find((a) => a.name === action);
    if (!act) throw new Error(`the shell has no action "${action}"`);
    return act.run(params, createContext(SHELL_INSTANCE, {}, () => {}));
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
  return act.run(params, makeContext(inst));
}
