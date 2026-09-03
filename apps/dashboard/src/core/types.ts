import type { Component } from "svelte";
import type { Writable } from "svelte/store";

/** A minimal JSON-Schema object description for an action's parameters. Kept
 *  loose on purpose — it is passed to the agent verbatim, not validated here. */
export interface JsonSchemaObject {
  type: "object";
  properties: Record<string, unknown>;
  required?: string[];
}

/** One thing a module instance can be told to do — by the `/` command router,
 *  the agent bridge, or the flip-side settings. */
export interface ModuleAction {
  /** Short verb, unique within the module: "run", "sync", "open". */
  name: string;
  /** One line, shown to the agent in the module manifest. */
  description: string;
  params: JsonSchemaObject;
  run(params: unknown, ctx: ModuleContext): Promise<unknown>;
}

/** Everything a mounted module instance gets from the shell. */
export interface ModuleContext {
  instanceId: string;
  /** This instance's persisted config blob. Reactive; changes are debounced
   *  to `~/.axiomata/dashboard.json` (from step 6). */
  config: Writable<Record<string, unknown>>;
  /** Thin passthrough to `@tauri-apps/api/core` `invoke`. */
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  /** Fire a shell-bus event (e.g. `"open-file"` to hand a path to md-file). */
  emit: (event: string, detail?: unknown) => void;
  /** Ask the shell to resize this tile. */
  requestResize: (size: { w: number; h: number }) => void;
}

/** The static description of a module type. Registered once at startup. */
export interface ModuleDefinition {
  /** Stable id, e.g. "skills-deck". */
  type: string;
  title: string;
  /** Inline SVG markup (no `<img>` — honours the `img-src` CSP). */
  icon: string;
  /** Front face; mounted with `{ ctx }`. */
  component: Component<{ ctx: ModuleContext }>;
  /** Flip-card back; mounted lazily with `{ ctx }`. */
  settings?: Component<{ ctx: ModuleContext }>;
  defaultSize: { w: number; h: number };
  minSize?: { w: number; h: number };
  /** Only one instance allowed (e.g. a future particle-graph module). */
  singleton?: boolean;
  /** May be opened as a slide-in staged panel, not just a canvas tile. */
  stageable?: boolean;
  actions?: ModuleAction[];
}

/** One placed module on the canvas. Persisted in `dashboard.json`. */
export interface CanvasInstance {
  id: string;
  type: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  flipped: boolean;
  config: Record<string, unknown>;
}
