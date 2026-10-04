/**
 * What a file tells about itself when it is opened (`docs/plans/editor.md`,
 * ED1, F11): its line ending, whether it ends with a line break, and how it is
 * indented. The editor keeps all three when saving, so opening and saving a file
 * never produces a diff by itself.
 */

import { splitLines } from "./buffer";

export type LineEnding = "lf" | "crlf";

export interface Indent {
  kind: "spaces" | "tabs";
  /** Spaces per level; for tabs, the width a tab is shown with. */
  size: number;
}

export interface FileShape {
  eol: LineEnding;
  /** The file mixed LF and CRLF; it is saved as LF (and the status bar says so). */
  mixedEol: boolean;
  /** The last line ends with a line break. */
  finalNewline: boolean;
}

/** How many leading lines the indentation guess looks at. */
const INDENT_SAMPLE_LINES = 200;

export function detectShape(text: string): FileShape {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const lf = (text.match(/(?<!\r)\n/g) ?? []).length;
  const mixed = crlf > 0 && lf > 0;
  return {
    eol: crlf > 0 && !mixed ? "crlf" : "lf",
    mixedEol: mixed,
    finalNewline: /\r?\n$/.test(text),
  };
}

/**
 * Guesses the indentation from the first lines: tabs if more lines start with a
 * tab than with spaces; otherwise the most common increase in leading spaces from
 * one indented line to the next. `null` when there is too little to tell — the
 * caller then uses the owner's setting.
 */
export function detectIndent(text: string): Indent | null {
  const lines = splitLines(text).slice(0, INDENT_SAMPLE_LINES);
  let tabLines = 0;
  let spaceLines = 0;
  const steps = new Map<number, number>();
  let previous = 0;
  for (const line of lines) {
    if (line.trim() === "") continue;
    if (line.startsWith("\t")) {
      tabLines++;
      continue;
    }
    const spaces = /^ */.exec(line)?.[0].length ?? 0;
    if (spaces > 0) spaceLines++;
    const step = Math.abs(spaces - previous);
    if (step >= 2 && step <= 8) steps.set(step, (steps.get(step) ?? 0) + 1);
    previous = spaces;
  }
  if (tabLines > spaceLines && tabLines > 0) return { kind: "tabs", size: 4 };
  if (spaceLines === 0 || steps.size === 0) return null;
  const [size] = [...steps.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0];
  return { kind: "spaces", size };
}

/**
 * Joins the store's text for saving, in the file's own shape. The store never
 * holds the final line break (`bodyForStore`), so it is always added here when
 * the file had one — also after an empty last line, which is a line of its own.
 */
export function joinForSave(body: string, shape: FileShape): string {
  const sep = shape.eol === "crlf" ? "\r\n" : "\n";
  const joined = splitLines(body).join(sep);
  return shape.finalNewline ? joined + sep : joined;
}

/**
 * The text as the store holds it: a final line break is metadata
 * (`FileShape.finalNewline`), not an empty last line, so ⌘↓ ends on the last
 * real line and saving restores the break exactly once.
 */
export function bodyForStore(text: string, shape: FileShape): string {
  return shape.finalNewline ? text.replace(/\r?\n$/, "") : text;
}
