/**
 * The open dialogs, newest last (`docs/plans/ui-ux-round.md` UX2: every panel keeps one close contract — ⌘W and
 * Escape). `shell/Window.svelte` registers each modal window while it is open; `App.svelte`'s ⌘W closes the newest
 * one before it turns to the staged panels underneath, and `StagingLayer.svelte`'s Escape steps aside for it.
 */

const stack: Array<() => void> = [];

/** Puts a window's close on the stack; returns the function that takes it off again. */
export function pushClose(close: () => void): () => void {
  stack.push(close);
  return () => {
    const at = stack.lastIndexOf(close);
    if (at >= 0) stack.splice(at, 1);
  };
}

/** Whether a dialog is open — it sits above the staged panels, so their own Escape leaves the key to it. */
export function dialogOpen(): boolean {
  return stack.length > 0;
}

/** Closes the newest open window; `false` when there is none. */
export function closeTop(): boolean {
  const close = stack[stack.length - 1];
  if (!close) return false;
  close();
  return true;
}
