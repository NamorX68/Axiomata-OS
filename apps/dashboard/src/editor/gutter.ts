/**
 * Line numbers in the gutter (`docs/plans/editor.md`, D6 and F6): absolute,
 * relative, or hybrid — the current line absolute, all others relative, which is
 * what Vi's counted motions (ED3) read best with.
 */

export type LineNumberMode = "absolute" | "relative" | "hybrid";

/** What the gutter shows: a numbering, or none (Vi's `:set nonu nornu`, ED3.3) — a setting never says "off". */
export type GutterMode = LineNumberMode | "off";

/** The label for logical line `line` (zero-based) with the cursor on `cursorLine`. */
export function lineLabel(line: number, cursorLine: number, mode: GutterMode): string {
  if (mode === "off") return "";
  if (mode === "absolute" || (mode === "hybrid" && line === cursorLine)) return String(line + 1);
  return String(Math.abs(line - cursorLine));
}

/** Characters the gutter must fit, so it does not jump while scrolling. */
export function gutterDigits(lineCount: number, mode: GutterMode): number {
  // Relative labels never exceed the line count either; one width fits all numberings.
  if (mode === "off") return 0;
  return Math.max(2, String(lineCount).length);
}
