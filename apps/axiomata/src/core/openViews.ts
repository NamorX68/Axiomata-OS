/**
 * What the shell knows is open beyond the staged panels and the canvas tiles, for the chat's `open_views` and
 * `close_view` (`docs/plans/chat-knows-app.md`, CH5): the shell's own windows, each registered by itself while it is
 * open, with its own close; and which terminal session a Terminal tile runs, so what it would end can be asked.
 *
 * Nothing here closes anything by itself — a window's `close` is the one its own × uses.
 */

/** The shell's own windows the chat may close. The Studio is none of them: its panes and agents are never the chat's
 *  to close. */
export type ShellViewKind = "second-brain" | "handbook" | "settings";

export interface ShellView {
  /** As the owner reads it: "Handbook — Kanban". */
  title: string;
  /** What closing now would lose, for the owner; `null` or missing for nothing. */
  loss?: () => string | null;
  /** The window's own close (the same as its × or Escape). */
  close: () => void;
}

const shellViews = new Map<ShellViewKind, ShellView>();

/** A window says it is open; the returned function says it is closed again (an `$effect`'s cleanup). */
export function registerShellView(kind: ShellViewKind, view: ShellView): () => void {
  shellViews.set(kind, view);
  return () => {
    if (shellViews.get(kind) === view) shellViews.delete(kind);
  };
}

/** The shell windows open now, in the order they opened. */
export function openShellViews(): [ShellViewKind, ShellView][] {
  return [...shellViews];
}

const terminalSessions = new Map<string, string>();

/** A Terminal tile (`instanceId`) runs session `sessionId` now, or (`null`) none any more. */
export function setTerminalSession(instanceId: string, sessionId: string | null): void {
  if (sessionId) terminalSessions.set(instanceId, sessionId);
  else terminalSessions.delete(instanceId);
}

/** The session a Terminal tile runs, if its shell is up. */
export function terminalSession(instanceId: string): string | null {
  return terminalSessions.get(instanceId) ?? null;
}
