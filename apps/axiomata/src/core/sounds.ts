/**
 * The app's sounds (`docs/plans/ui-ux-round.md` UX7d, G10: off by default): three soft cues for when Axiomata lies in
 * the background — an agent finished its turn, an agent waits for the owner, a routine fired. Plain logic here: which
 * changes between two looks at the app call for a cue, and what each cue sounds like as data. The watching is
 * `core/soundWatch.ts`, the playing (WebAudio, no sound files) `shell/soundPlayer.ts`.
 */

import type { AgentState } from "./backend";

/** What a sound tells. */
export type Cue = "waiting" | "done" | "routine";

/** Most important first: when several happen at once, only the first of them sounds. */
export const CUE_ORDER: readonly Cue[] = ["waiting", "done", "routine"];

/** One look at what the sounds listen to. */
export interface Watched {
  /** Every agent's state, across all projects (agent ids are unique app-wide). */
  agents: ReadonlyMap<number, AgentState>;
  /** Every routine's last firing (`last_fired_at` as the backend writes it), `null` while it never fired. */
  routines: ReadonlyMap<number, string | null>;
}

/**
 * The cues the change from `prev` to `next` calls for, most important first and each at most once. The first look
 * (`prev` is `null`) only sets the scene. An agent that starts to wait counts even when it is new (a harness may ask
 * for a permission before it reported anything else); a finished turn is `working` going to `idle` or `ended`; a
 * routine counts when a known one's last firing changed.
 */
export function cuesBetween(prev: Watched | null, next: Watched): Cue[] {
  if (prev === null) return [];
  const found = new Set<Cue>();
  for (const [id, state] of next.agents) {
    const before = prev.agents.get(id);
    if (state === "waiting" && before !== "waiting") found.add("waiting");
    if (before === "working" && (state === "idle" || state === "ended")) found.add("done");
  }
  for (const [id, fired] of next.routines) {
    if (!prev.routines.has(id)) continue;
    if (fired !== null && fired !== prev.routines.get(id)) found.add("routine");
  }
  return CUE_ORDER.filter((cue) => found.has(cue));
}

/** One sound of a cue: a tone, or a spark (a short burst of noise through a band filter). */
export interface Voice {
  kind: "tone" | "spark";
  /** The tone's pitch, or the spark's band centre, in Hz. */
  freq: number;
  /** When it starts after the cue, in seconds. */
  at: number;
  /** How long it rings out, in seconds. */
  dur: number;
  /** Its loudest, 0…1, before the master volume. */
  gain: number;
}

/**
 * What each cue sounds like: short, soft and low enough to sit under music. Waiting asks (two equal notes, the second
 * a touch louder, like a knock), done settles (a rising fifth that rings out), the routine catches fire (a spark,
 * then a low warm note).
 */
export const CUE_VOICES: Readonly<Record<Cue, readonly Voice[]>> = {
  waiting: [
    { kind: "tone", freq: 784, at: 0, dur: 0.35, gain: 0.5 },
    { kind: "tone", freq: 784, at: 0.18, dur: 0.6, gain: 0.6 },
  ],
  done: [
    { kind: "tone", freq: 659.25, at: 0, dur: 0.5, gain: 0.45 },
    { kind: "tone", freq: 987.77, at: 0.12, dur: 0.9, gain: 0.5 },
  ],
  routine: [
    { kind: "spark", freq: 3200, at: 0, dur: 0.12, gain: 0.35 },
    { kind: "tone", freq: 440, at: 0.05, dur: 0.8, gain: 0.45 },
  ],
};

/** The master volume every voice goes through. */
export const MASTER_GAIN = 0.18;

/**
 * One scene from the backend's answers: every project's agent states (agent ids are unique app-wide) and every
 * routine's last firing.
 */
export function sceneOf(
  statesByProject: Iterable<readonly { agent_id: number; state: AgentState }[]>,
  routines: readonly { id: number; last_fired_at: string | null }[],
): Watched {
  const agents = new Map<number, AgentState>();
  for (const states of statesByProject) for (const s of states) agents.set(s.agent_id, s.state);
  return { agents, routines: new Map(routines.map((r) => [r.id, r.last_fired_at])) };
}

/** The names the settings show. */
export const CUE_LABELS: Readonly<Record<Cue, string>> = {
  waiting: "An agent waits for you",
  done: "An agent finished its turn",
  routine: "A routine fired",
};
