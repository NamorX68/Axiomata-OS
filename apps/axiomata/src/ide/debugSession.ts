/**
 * What the debug panel shows, and the actions that change it (`docs/plans/editor-projekt-werkzeuge.md`, #51).
 *
 * A session's events (`debugBackend.ts`) move the state: a stop fetches the stack, the first frame's
 * scopes and the locals; continuing clears them; output is appended (bounded). The controller takes the
 * backend as an argument so the whole sequence — including a stop that is overtaken by the next `continue` —
 * is tested without a Tauri window.
 */

import { isFullScreen, stripAnsi } from "./ansi";
import type { BreakpointSpec } from "./breakpoints";
import { writable, get, type Readable } from "svelte/store";

import {
  controlDebug,
  debugEvaluate,
  debugScopes,
  debugSetBreakpoints,
  debugStack,
  debugVariables,
  startDebug,
  stopDebug,
  type DebugAction,
  type DebugEvent,
  type DebugScope,
  type DebugTarget,
  type DebugVariable,
  type StackFrame,
} from "./debugBackend";

export type Phase = "idle" | "starting" | "running" | "stopped" | "ended";

export interface ScopeView {
  scope: DebugScope;
  /** `null` until fetched (an expensive scope waits for the user to open it). */
  variables: DebugVariable[] | null;
}

export interface WatchView {
  expr: string;
  /** `null` while there is nothing to show (not stopped, or not evaluated yet). */
  value: string | null;
  error: string | null;
}

export interface OutputLine {
  /** `stdout`, `stderr`, `console`, or `repl` for the console's own lines. */
  category: string;
  text: string;
}

export interface DebugState {
  phase: Phase;
  configName: string | null;
  stop: { threadId: number; reason: string; text: string | null } | null;
  frames: StackFrame[];
  frameId: number | null;
  scopes: ScopeView[];
  /** Expanded variables: `variablesReference` → children. */
  children: Record<number, DebugVariable[]>;
  output: OutputLine[];
  exitCode: number | null;
  error: string | null;
  /** Watch expressions, evaluated in the selected frame at every stop. */
  watches: WatchView[];
  /** The program draws a full-screen terminal interface: its output is no console text (see `ansi.ts`). */
  tui: boolean;
}

export const IDLE: DebugState = {
  phase: "idle",
  configName: null,
  stop: null,
  frames: [],
  frameId: null,
  scopes: [],
  children: {},
  output: [],
  exitCode: null,
  error: null,
  tui: false,
  watches: [],
};

/** The most output lines kept; older ones fall off the top. */
export const MAX_OUTPUT_LINES = 3000;

function appendOutput(lines: OutputLine[], category: string, text: string): OutputLine[] {
  const next = lines.slice();
  // Program output arrives in chunks that need not end at a line break: join them to the open line.
  for (const part of text.split(/(?<=\n)/)) {
    const last = next[next.length - 1];
    if (last && last.category === category && !last.text.endsWith("\n")) {
      next[next.length - 1] = { category, text: last.text + part };
    } else {
      next.push({ category, text: part });
    }
  }
  return next.length > MAX_OUTPUT_LINES ? next.slice(next.length - MAX_OUTPUT_LINES) : next;
}

/** What one event does to the state (the follow-up fetches are the controller's). */
export function reduce(state: DebugState, event: DebugEvent): DebugState {
  switch (event.event) {
    case "stopped":
      return {
        ...state,
        phase: "stopped",
        stop: { threadId: event.thread_id, reason: event.reason, text: event.text },
        frames: [],
        frameId: null,
        scopes: [],
        children: {},
      };
    case "continued":
      return { ...state, phase: "running", stop: null, frames: [], frameId: null, scopes: [], children: {} };
    case "output": {
      if (state.tui && (event.category === "stdout" || event.category === "stderr")) return state;
      if (isFullScreen(event.text)) return { ...state, tui: true };
      return { ...state, output: appendOutput(state.output, event.category, stripAnsi(event.text)) };
    }
    case "exited":
      return { ...state, exitCode: event.code };
    case "terminated":
      return { ...state, phase: "ended", stop: null, frames: [], frameId: null, scopes: [], children: {} };
    case "run_in_terminal":
      return state; // the controller hands it to the host's terminal
    case "closed":
      return {
        ...state,
        phase: "ended",
        stop: null,
        frames: [],
        frameId: null,
        scopes: [],
        children: {},
        // An adapter that ends while the session was live says why (stderr's tail) — unless it ended normally.
        error: state.phase === "ended" || !event.stderr.trim() ? state.error : event.stderr.trim(),
      };
  }
}

export interface Backend {
  start: typeof startDebug;
  stop: typeof stopDebug;
  control: typeof controlDebug;
  stack: typeof debugStack;
  scopes: typeof debugScopes;
  variables: typeof debugVariables;
  evaluate: typeof debugEvaluate;
  setBreakpoints: typeof debugSetBreakpoints;
}

export const realBackend: Backend = {
  start: startDebug,
  stop: stopDebug,
  control: controlDebug,
  stack: debugStack,
  scopes: debugScopes,
  variables: debugVariables,
  evaluate: debugEvaluate,
  setBreakpoints: debugSetBreakpoints,
};

export interface Hooks {
  /** The program stopped at `frame` (the top of the stack) — show the file at that line. */
  onStop?: (frame: StackFrame) => void;
  /** The program is to run in a terminal pane: type `line` into a shell there (`title` names the pane). */
  onTerminal?: (title: string, line: string) => void;
  /** Something went wrong that the panel should say. */
  onError?: (message: string) => void;
  /**
   * Is `frame` code of the project (rather than the standard library, a generated file)? At a stop the first
   * such frame is the one shown — a stop inside `rust_panic` lands on the line that panicked.
   */
  isUserFrame?: (frame: StackFrame) => boolean;
}

export interface DebugController {
  state: Readable<DebugState>;
  start: (
    root: string,
    target: DebugTarget,
    breakpoints: { rel: string; breakpoints: BreakpointSpec[] }[],
    name: string,
    terminal?: boolean,
    args?: string[] | null,
  ) => Promise<void>;
  stop: () => Promise<void>;
  control: (action: DebugAction) => Promise<void>;
  selectFrame: (frameId: number) => Promise<void>;
  /** Fetches a scope or a variable's children (once). */
  expand: (variablesReference: number) => Promise<void>;
  /** The console's line: evaluated in the selected frame, answered in the output. */
  evaluate: (expression: string) => Promise<void>;
  /** The editor changed a file's breakpoints while a session runs. */
  syncBreakpoints: (rel: string, specs: BreakpointSpec[]) => Promise<void>;
  /** Watch expressions: set the whole list (loading), add one, drop one. Evaluated at every stop. */
  setWatches: (expressions: string[]) => void;
  addWatch: (expression: string) => void;
  removeWatch: (index: number) => void;
}

const message = (err: unknown): string => (err as { message?: string })?.message ?? String(err);

export function createDebugController(backend: Backend, hooks: Hooks = {}): DebugController {
  const state = writable<DebugState>(IDLE);
  /** Each stop and continue bumps it: a fetch that finds it changed belongs to a stop that is over. */
  let epoch = 0;

  const set = (fn: (s: DebugState) => DebugState) => state.update(fn);

  async function loadFrame(frameId: number, mine: number): Promise<void> {
    const scopes = await backend.scopes(frameId);
    if (mine !== epoch) return;
    set((s) => ({ ...s, frameId, scopes: scopes.map((scope) => ({ scope, variables: null })) }));
    const first = scopes.find((sc) => !sc.expensive) ?? scopes[0];
    if (first) {
      const variables = await backend.variables(first.variables_reference);
      if (mine !== epoch) return;
      set((s) => ({
        ...s,
        scopes: s.scopes.map((v) => (v.scope.variables_reference === first.variables_reference ? { ...v, variables } : v)),
      }));
    }
  }

  /** Evaluates every watch expression in `frameId`; a stop that has been overtaken throws its answers away. */
  async function refreshWatches(frameId: number, mine: number): Promise<void> {
    const expressions = get(state).watches.map((w) => w.expr);
    const results = await Promise.all(
      expressions.map(async (expr) => {
        try {
          return { expr, value: (await backend.evaluate(expr, frameId, "watch")).value, error: null };
        } catch (err) {
          return { expr, value: null, error: message(err) };
        }
      }),
    );
    if (mine !== epoch) return;
    // Keep what was added or removed while this was running.
    set((s) => ({ ...s, watches: s.watches.map((w) => results.find((r) => r.expr === w.expr) ?? w) }));
  }

  async function onStopped(mine: number): Promise<void> {
    try {
      const frames = await backend.stack();
      if (mine !== epoch) return;
      set((s) => ({ ...s, frames }));
      // The first frame that is the project's own (a stop inside the standard library lands on the caller).
      const shown = frames.find((f) => hooks.isUserFrame?.(f)) ?? frames[0];
      if (!shown) return;
      hooks.onStop?.(shown);
      await loadFrame(shown.id, mine);
      await refreshWatches(shown.id, mine);
    } catch (err) {
      if (mine === epoch) hooks.onError?.(message(err));
    }
  }

  function onEvent(event: DebugEvent): void {
    if (event.event === "run_in_terminal") {
      hooks.onTerminal?.(event.title, event.line);
      return;
    }
    epoch++;
    set((s) => reduce(s, event));
    if (event.event === "stopped") void onStopped(epoch);
  }

  return {
    state: { subscribe: state.subscribe },

    async start(root, target, breakpoints, name, terminal = false, args = null) {
      epoch++;
      state.set({
        ...IDLE,
        phase: "starting",
        configName: name,
        watches: get(state).watches.map((w) => ({ expr: w.expr, value: null, error: null })),
      });
      try {
        await backend.start(root, target, breakpoints, terminal, args, onEvent);
        // The program may already have stopped on a breakpoint by now; only a quiet start is "running".
        set((s) => (s.phase === "starting" ? { ...s, phase: "running" } : s));
      } catch (err) {
        set((s) => ({ ...s, phase: "ended", error: message(err) }));
        hooks.onError?.(message(err));
      }
    },

    async stop() {
      try {
        await backend.stop();
      } catch (err) {
        hooks.onError?.(message(err));
      }
      set((s) => (s.phase === "idle" ? s : { ...s, phase: "ended", stop: null, frames: [], scopes: [], frameId: null }));
    },

    async control(action) {
      if (get(state).phase !== "stopped" && action !== "pause") return;
      try {
        await backend.control(action);
      } catch (err) {
        hooks.onError?.(message(err));
      }
    },

    async selectFrame(frameId) {
      const mine = epoch;
      const frame = get(state).frames.find((f) => f.id === frameId);
      if (!frame) return;
      set((s) => ({ ...s, frameId, children: {} }));
      hooks.onStop?.(frame);
      try {
        await loadFrame(frameId, mine);
        await refreshWatches(frameId, mine);
      } catch (err) {
        if (mine === epoch) hooks.onError?.(message(err));
      }
    },

    async expand(reference) {
      const known = get(state);
      if (known.children[reference]) return;
      const mine = epoch;
      try {
        const variables = await backend.variables(reference);
        if (mine !== epoch) return;
        set((s) => ({
          ...s,
          children: { ...s.children, [reference]: variables },
          // A scope's own list fills in the same way.
          scopes: s.scopes.map((v) => (v.scope.variables_reference === reference ? { ...v, variables } : v)),
        }));
      } catch (err) {
        if (mine === epoch) hooks.onError?.(message(err));
      }
    },

    async evaluate(expression) {
      const text = expression.trim();
      if (!text) return;
      const frameId = get(state).frameId;
      set((s) => ({ ...s, output: appendOutput(s.output, "repl", `> ${text}\n`) }));
      try {
        const result = await backend.evaluate(text, frameId);
        set((s) => ({ ...s, output: appendOutput(s.output, "repl", `${result.value}\n`) }));
      } catch (err) {
        set((s) => ({ ...s, output: appendOutput(s.output, "stderr", `${message(err)}\n`) }));
      }
    },

    setWatches(expressions) {
      set((s) => ({ ...s, watches: expressions.map((expr) => ({ expr, value: null, error: null })) }));
      const now = get(state);
      if (now.phase === "stopped" && now.frameId !== null) void refreshWatches(now.frameId, epoch);
    },

    addWatch(expression) {
      const expr = expression.trim();
      if (!expr || get(state).watches.some((w) => w.expr === expr)) return;
      set((s) => ({ ...s, watches: [...s.watches, { expr, value: null, error: null }] }));
      const now = get(state);
      if (now.phase === "stopped" && now.frameId !== null) void refreshWatches(now.frameId, epoch);
    },

    removeWatch(index) {
      set((s) => ({ ...s, watches: s.watches.filter((_, i) => i !== index) }));
    },

    async syncBreakpoints(rel, specs) {
      const phase = get(state).phase;
      if (phase === "idle" || phase === "ended") return;
      try {
        await backend.setBreakpoints(rel, specs);
      } catch (err) {
        hooks.onError?.(message(err));
      }
    },
  };
}
