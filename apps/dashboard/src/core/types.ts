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
  /** Overrides `defaultSize` for a freshly created instance — called fresh
   *  by `core/lifecycle.ts`'s `createInstance` each time, instead of a
   *  static value, for a module whose "reasonable starting size" genuinely
   *  depends on something computed at creation time rather than fixed at
   *  registration (Terminal: a size that fits a fixed character-cell count
   *  at whatever font is currently configured — see
   *  `modules/index.ts`'s Terminal registration). `defaultSize` above still
   *  has to be set regardless (used if this throws, or is absent, or a
   *  caller reads `defaultSize` directly without going through
   *  `createInstance` — e.g. `ModulePicker`'s own size-label text). */
  computeDefaultSize?: () => { w: number; h: number };
  /** Overrides `ModulePicker.svelte`'s own `${defaultSize.w}×${defaultSize.h}`
   *  size label for a module that declares `computeDefaultSize` — that
   *  label would otherwise keep showing the static `defaultSize` (a real
   *  pixel figure, now stale by construction the moment a computed size
   *  diverges meaningfully from it, e.g. Terminal at a larger-than-default
   *  font size — architecture review, Checkpoint 5e) as if it were still
   *  accurate. A plain string, not a second size computation: the picker
   *  renders a whole list, so calling `computeDefaultSize` per row just to
   *  throw the number away on the next render would be wasted work for a
   *  label that's illustrative, not load-bearing, either way. */
  sizeLabel?: string;
  minSize?: { w: number; h: number };
  /** Only one instance allowed (e.g. a future particle-graph module). */
  singleton?: boolean;
  /** May be opened as a slide-in staged panel, not just a canvas tile. */
  stageable?: boolean;
  /** Renders full-size behind the tiles (in `#particle-slot`) instead of as
   *  a tile — the particle graph. Implies one instance at a time. */
  background?: boolean;
  /** Dev-only scaffolding (the `dummy*` modules), registered only in
   *  `import.meta.env.DEV`. An explicit opt-out flag rather than a `dummy*`
   *  name-prefix convention, so anything that filters dev modules out (e.g.
   *  the App Ring) doesn't have to know that convention exists. */
  dev?: boolean;
  actions?: ModuleAction[];
}

/** Which canvas edges a tile follows when the window is resized, and the
 *  canvas size its `x`/`y` were committed at. Missing = left/top. */
export interface TileAnchor {
  x: "left" | "right";
  y: "top" | "bottom";
  /** Canvas size at commit time — the reference for the edge offsets. */
  w: number;
  h: number;
}

/** One placed module on the canvas. Persisted in `dashboard.json`. `x`/`y`
 *  are the desired position for the anchor's reference size; the displayed
 *  position follows the anchored edge and is clamped into the current canvas
 *  (see `canvas/snap.ts` `displayRect`). */
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
  anchor?: TileAnchor;
}
