/**
 * Where a mouse wheel turn in a terminal goes, and what scrolling through its history by keyboard does (card #80).
 * Pure logic, split out of `terminal.svelte` so it is unit-testable, like `terminalInput.ts`.
 *
 * A program that switched on mouse tracking (Claude Code, Opencode, `nvim`) scrolls itself and gets the wheel as mouse
 * reports. On the alternate screen without tracking there is no history to scroll, so the wheel becomes arrow keys —
 * what Ghostty, iTerm and xterm's "alternate scroll" do, and what lets `less` or `man` scroll. Everywhere else the
 * terminal scrolls its own history.
 */

import { chord, type KeyLike, type Platform } from "../core/keys";

/** Mirrors `axiomata_terminal::MouseReporting`. */
export type MouseReporting = "off" | "x10" | "sgr";

/** What the running program asked for, from the last screen event. */
export interface WheelState {
  mouse: MouseReporting;
  altScreen: boolean;
}

/** What one wheel step does: bytes for the program, or a number of history lines (positive = further back). */
export type WheelAction = { kind: "send"; bytes: Uint8Array } | { kind: "history"; lines: number };

/** X10 mouse reports carry each value as one byte offset by 32, so a column or row past this cannot be reported. */
const X10_MAX_COORD = 223;
/** The X10/SGR button codes of the wheel: 64 is up (away from the user), 65 down. */
const WHEEL_UP_BUTTON = 64;
const WHEEL_DOWN_BUTTON = 65;
/** Offset of every value in an X10 report byte. */
const X10_OFFSET = 32;

const encoder = new TextEncoder();

/**
 * What `lines` wheel lines (positive = up, towards older output) at the 0-indexed cell `row`/`col` do. One report or
 * arrow key per line, as other terminals send them; `lines` of 0 does nothing.
 */
export function wheelAction(lines: number, state: WheelState, row: number, col: number): WheelAction | null {
  if (lines === 0) return null;
  const steps = Math.abs(lines);
  const up = lines > 0;
  if (state.mouse !== "off") {
    const one = mouseWheelReport(up, state.mouse, row, col);
    return one ? { kind: "send", bytes: repeat(one, steps) } : null;
  }
  if (state.altScreen) {
    return { kind: "send", bytes: repeat(encoder.encode(up ? "\x1b[A" : "\x1b[B"), steps) };
  }
  return { kind: "history", lines };
}

/** One wheel report in the program's encoding, or `null` where X10 cannot express the position. */
export function mouseWheelReport(up: boolean, mode: "x10" | "sgr", row: number, col: number): Uint8Array | null {
  const button = up ? WHEEL_UP_BUTTON : WHEEL_DOWN_BUTTON;
  const x = Math.max(0, col) + 1;
  const y = Math.max(0, row) + 1;
  if (mode === "sgr") return encoder.encode(`\x1b[<${button};${x};${y}M`);
  if (x > X10_MAX_COORD || y > X10_MAX_COORD) return null;
  return new Uint8Array([0x1b, 0x5b, 0x4d, X10_OFFSET + button, X10_OFFSET + x, X10_OFFSET + y]);
}

function repeat(bytes: Uint8Array, times: number): Uint8Array {
  const out = new Uint8Array(bytes.length * times);
  for (let i = 0; i < times; i++) out.set(bytes, i * bytes.length);
  return out;
}

/**
 * Turns wheel deltas into whole lines. A mouse wheel sends a few large steps, a trackpad many small ones; counting
 * every event as a fixed number of lines (as before) made a trackpad race through the history. Pixel deltas are
 * collected until a line's height is reached, and the remainder is kept for the next event.
 */
export class WheelAccumulator {
  private rest = 0;

  /**
   * Lines to scroll for one `WheelEvent` (positive = up).
   *
   * @param deltaY the event's `deltaY` (positive = down, as the DOM reports it)
   * @param deltaMode the event's `deltaMode`: 0 pixels, 1 lines, 2 pages
   * @param lineHeight one terminal row in CSS pixels
   * @param pageLines lines in one page, for `deltaMode` 2
   */
  lines(deltaY: number, deltaMode: number, lineHeight: number, pageLines: number): number {
    const inLines = deltaMode === 1 ? deltaY : deltaMode === 2 ? deltaY * pageLines : deltaY / Math.max(1, lineHeight);
    this.rest += inLines;
    const whole = Math.trunc(this.rest);
    this.rest -= whole;
    return -whole;
  }

  /** Forgets a partial line, e.g. when the direction or the target changes. */
  reset(): void {
    this.rest = 0;
  }
}

/** What a history key does: lines to move (positive = back), or a jump to the top or the live bottom. */
export type HistoryKeyAction = { kind: "lines"; lines: number } | { kind: "top" } | { kind: "bottom" };

/**
 * Shift+PageUp/PageDown/Home/End scroll the history (on a Mac keyboard: Shift+fn+↑/↓/←/→), as in xterm and most
 * terminals; only on the primary screen — on the alternate screen the keys are the program's. `null` for any other key.
 */
export function historyKeyAction(
  key: string,
  mods: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean },
  altScreen: boolean,
  pageLines: number,
): HistoryKeyAction | null {
  if (altScreen || !mods.shiftKey || mods.ctrlKey || mods.altKey || mods.metaKey) return null;
  const page = Math.max(1, pageLines - 1);
  switch (key) {
    case "PageUp":
      return { kind: "lines", lines: page };
    case "PageDown":
      return { kind: "lines", lines: -page };
    case "Home":
      return { kind: "top" };
    case "End":
      return { kind: "bottom" };
    default:
      return null;
  }
}

/** The scrollbar's thumb as fractions of the track: where it starts from the top, and how tall it is. */
export interface Thumb {
  top: number;
  size: number;
}

/** A minimum thumb size, as a fraction of the track, so a long history still leaves something to grab. */
const MIN_THUMB = 0.06;

/**
 * The thumb for `rows` visible lines over `scrollback` lines of history, scrolled `offset` lines back; `null` while
 * there is no history to scroll.
 */
export function scrollThumb(rows: number, scrollback: number, offset: number): Thumb | null {
  if (scrollback <= 0 || rows <= 0) return null;
  const total = rows + scrollback;
  const size = Math.max(MIN_THUMB, rows / total);
  const back = Math.min(Math.max(0, offset), scrollback);
  // At offset 0 the thumb sits at the bottom, at `scrollback` at the top.
  const top = (1 - size) * (1 - back / scrollback);
  return { top, size };
}

/** The history offset for a thumb dragged so that its top is at `fraction` of the track (inverse of `scrollThumb`). */
export function offsetForThumb(fraction: number, rows: number, scrollback: number): number {
  if (scrollback <= 0) return 0;
  const size = Math.max(MIN_THUMB, rows / (rows + scrollback));
  const free = 1 - size;
  if (free <= 0) return 0;
  const clamped = Math.min(Math.max(fraction, 0), free);
  return Math.round(scrollback * (1 - clamped / free));
}

/** Lines one press of the scroll keys moves — the same as one notch of a mouse wheel in most terminals. */
export const SCROLL_KEY_LINES = 3;

/**
 * ⌘⇧↑/↓ on the Mac, Ctrl+Shift+↑/↓ elsewhere (owner, card #80): scroll like the mouse wheel — the history on the
 * primary screen, the program's own view where it tracks the mouse. Lines to scroll (positive = up), or `null` for any
 * other key.
 */
export function scrollKeyLines(e: KeyLike, on: Platform): number | null {
  const c = chord(e, on, true);
  // Off the Mac `chord` already reads a terminal's Ctrl+Shift as the primary key and leaves `shift` false.
  if (!c.primary || c.alt || c.typing || (on === "mac" && !c.shift)) return null;
  if (c.key === "arrowup") return SCROLL_KEY_LINES;
  if (c.key === "arrowdown") return -SCROLL_KEY_LINES;
  return null;
}
