/**
 * The debugger's keys. F1–F4 (owner's wish, 2026-10-03) are the ones to use: F11 is “show desktop” on macOS and
 * never reaches the app. F5 and F10 keep working for anyone used to other editors.
 */

import type { DebugAction } from "./debugBackend";

const BY_KEY: Record<string, DebugAction> = {
  F1: "continue",
  F2: "next",
  F3: "step_in",
  F4: "step_out",
  F5: "continue",
  F10: "next",
};

/** What a key press asks of a stopped program, or `null` when it is no debugger key. */
export function debugKeyAction(e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): DebugAction | null {
  if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return null;
  return BY_KEY[e.key] ?? null;
}

/** The key names shown in tooltips, one place so they cannot drift from `BY_KEY`. */
export const KEY_LABEL = { continue: "F1", next: "F2", step_in: "F3", step_out: "F4" } as const;
