/**
 * The routine fuse (`docs/plans/ui-ux-round.md` UX3, "Zündschnur"): the next routine burns down like a fuse from its
 * last firing to its next one. How far it has burnt is plain arithmetic, kept here so it can be tested; the drawing is
 * `modules/RoutineFuse.svelte`.
 */

/** How long a fuse is when the routine never fired yet: an hour, so a far-off first firing still shows a cold fuse. */
export const DEFAULT_FUSE_MS = 60 * 60 * 1000;

/** How much of the fuse has burnt at `now`, 0…1: from the last firing (or an hour before the next) to the next. */
export function fuseProgress(lastFired: number | null, nextFire: number, now: number): number {
  const start = lastFired !== null && lastFired < nextFire ? lastFired : nextFire - DEFAULT_FUSE_MS;
  const span = nextFire - start;
  if (span <= 0) return 1;
  return Math.min(1, Math.max(0, (now - start) / span));
}
