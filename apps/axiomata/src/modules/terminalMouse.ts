/**
 * Mouse reports for a program that asked for the mouse (TUI0b, `docs/plans/terminal-tui.md` D3): clicks, drags and
 * motion in the program's encoding — SGR (`?1006`, what Glut, Claude Code, Opencode and `nvim` ask for) or the classic
 * X10 bytes. Pure logic beside `terminalWheel.ts`, which reports the wheel the same way.
 */

import type { MouseReporting, MouseTracking } from "./terminalWheel";

/** What happened: a button went down or up, the pointer moved with a button held, or without one. */
export type MouseKind = "press" | "release" | "drag" | "move";

/** The held modifiers, as the report's button code carries them. */
export interface MouseModifiers {
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
}

/** X10 reports carry each value as one byte offset by 32: a column or row past this cannot be reported. */
const X10_MAX_COORD = 223;
const X10_OFFSET = 32;
/** The button code of "no button" (an X10 release, a move with none held). */
const NO_BUTTON = 3;
/** Added to the button code for a drag or a move. */
const MOTION = 32;

const encoder = new TextEncoder();

/** Whether the program wants a report of `kind` under `tracking`: clicks always, drags from `?1002`, bare motion only
 *  from `?1003`. */
export function wantsReport(kind: MouseKind, tracking: MouseTracking): boolean {
  switch (tracking) {
    case "off":
      return false;
    case "clicks":
      return kind === "press" || kind === "release";
    case "drags":
      return kind !== "move";
    case "motion":
      return true;
  }
}

/**
 * One mouse report: `button` 0 left, 1 middle, 2 right (a DOM `MouseEvent.button`), at the 0-indexed cell `row`/`col`.
 * `null` where X10 cannot express the position, or for a button the protocol has no code for.
 */
export function mouseReport(
  kind: MouseKind,
  button: number,
  mods: MouseModifiers,
  encoding: Exclude<MouseReporting, "off">,
  row: number,
  col: number,
): Uint8Array | null {
  if (kind !== "move" && (button < 0 || button > 2)) return null;
  const modifiers = (mods.shiftKey ? 4 : 0) + (mods.altKey ? 8 : 0) + (mods.ctrlKey ? 16 : 0);
  const x = Math.max(0, col) + 1;
  const y = Math.max(0, row) + 1;
  const moving = kind === "drag" || kind === "move" ? MOTION : 0;
  if (encoding === "sgr") {
    // SGR keeps the button on a release and tells it apart by the final `m`.
    const code = (kind === "move" ? NO_BUTTON : button) + moving + modifiers;
    return encoder.encode(`\x1b[<${code};${x};${y}${kind === "release" ? "m" : "M"}`);
  }
  if (x > X10_MAX_COORD || y > X10_MAX_COORD) return null;
  const code = (kind === "release" || kind === "move" ? NO_BUTTON : button) + moving + modifiers;
  return new Uint8Array([0x1b, 0x5b, 0x4d, X10_OFFSET + code, X10_OFFSET + x, X10_OFFSET + y]);
}

/** What the pane sends when it gains or loses focus, for a program that asked (`?1004`). */
export function focusReport(focused: boolean): Uint8Array {
  return encoder.encode(focused ? "\x1b[I" : "\x1b[O");
}
