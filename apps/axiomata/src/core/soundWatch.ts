/**
 * Listens for what the sounds tell (`core/sounds.ts`): every few seconds one look at all agents' states and all
 * routines, and the most important cue of what changed is played — but only while Axiomata lies in the background;
 * in front, the glow already says it. Unlike the app's other polls this one goes on while the window is hidden: that
 * is the very case it is for. It runs only while the sounds are switched on (`shell/Sounds.svelte`).
 */

import { cuesBetween, type Cue, type Watched } from "./sounds";

/** How often it looks; a cue a few seconds late is fine, a poll every second across all projects is not. */
export const SOUND_POLL_MS = 4000;

/** How long one look may take before it counts as failed: a call that never comes back must not stop the watcher. */
export const LOOK_TIMEOUT_MS = 15_000;

export interface SoundWatch {
  /** One look at the agents and routines; a failure skips the round. */
  look: () => Promise<Watched>;
  play: (cue: Cue) => void;
  /** Whether the window is not the one in front right now. */
  inBackground: () => boolean;
  intervalMs?: number;
  timeoutMs?: number;
}

/** `promise`, or a rejection once `ms` have passed without an answer. */
function within<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error("look timed out")), ms);
  });
  return Promise.race([promise, late]).finally(() => clearTimeout(timer));
}

/**
 * Starts looking (once right away, to set the scene) and returns the function that stops it. A look still pending
 * skips the next round; a failed or timed-out one keeps the last scene, so a short backend hiccup sounds nothing
 * afterwards.
 */
export function startSoundWatch(watch: SoundWatch): () => void {
  let last: Watched | null = null;
  let pending = false;
  let stopped = false;
  const round = async (): Promise<void> => {
    if (pending) return;
    pending = true;
    try {
      const next = await within(watch.look(), watch.timeoutMs ?? LOOK_TIMEOUT_MS);
      if (stopped) return;
      const cues = cuesBetween(last, next);
      last = next;
      const first = cues[0];
      if (first !== undefined && watch.inBackground()) watch.play(first);
    } catch {
      // Keep the last scene; a sound that fails to play is no reason to stop listening either.
    } finally {
      pending = false;
    }
  };
  void round();
  const timer = setInterval(() => void round(), watch.intervalMs ?? SOUND_POLL_MS);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}
