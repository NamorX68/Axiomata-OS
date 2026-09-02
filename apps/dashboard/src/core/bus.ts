/**
 * A tiny in-process event bus for module → shell and shell → module messages
 * (e.g. a module emits `"open-file"` with a path; md-file listens). Not
 * persisted, not cross-window.
 */

const target = new EventTarget();

export function emit(event: string, detail?: unknown): void {
  target.dispatchEvent(new CustomEvent(event, { detail }));
}

/** Subscribe; returns an unsubscribe function. */
export function on(event: string, handler: (detail: unknown) => void): () => void {
  const listener = (e: Event) => handler((e as CustomEvent).detail);
  target.addEventListener(event, listener);
  return () => target.removeEventListener(event, listener);
}
