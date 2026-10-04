/**
 * The minimap's geometry (`docs/plans/editor.md`, ED5, T8): every visual row
 * as a strip `rowPx` high, the part of the text on screen as a slider over it.
 *
 * * **A text taller than the minimap scrolls it** in proportion to the view:
 *   at the top of the text the minimap shows its top, at the bottom its
 *   bottom, and the slider travels the minimap's whole height in between.
 * * **Dragging the slider scrolls the text** by the same proportion, so the
 *   slider stays under the mouse; a click beside it centres the text there.
 *
 * Pixels throughout; the view multiplies by the device pixel ratio when it draws.
 */

export interface MinimapInput {
  /** Visual rows in the text (folded lines have none). */
  totalRows: number;
  /** The text's row height and the view's height and scroll position, in pixels. */
  rowH: number;
  viewH: number;
  scrollTop: number;
  /** One row's height in the minimap. */
  rowPx: number;
  /** The minimap's height. */
  height: number;
}

export interface MinimapLayout {
  /** How far the minimap's own content is scrolled. */
  miniScroll: number;
  /** The slider: where the view is. */
  sliderTop: number;
  sliderHeight: number;
  /** The first and last row the minimap shows (either may be partly cut off). */
  firstRow: number;
  lastRow: number;
}

/** How far the text can scroll. */
function maxScroll(input: MinimapInput): number {
  return Math.max(0, input.totalRows * input.rowH - input.viewH);
}

/** How far the slider can travel. */
function sliderRange(input: MinimapInput, sliderHeight: number): number {
  return Math.max(0, Math.min(input.totalRows * input.rowPx, input.height) - sliderHeight);
}

export function minimapLayout(input: MinimapInput): MinimapLayout {
  const { totalRows, rowH, viewH, rowPx, height } = input;
  const sliderHeight = Math.min(height, (viewH / rowH) * rowPx);
  const max = maxScroll(input);
  const ratio = max > 0 ? Math.min(1, Math.max(0, input.scrollTop / max)) : 0;
  const miniScroll = ratio * Math.max(0, totalRows * rowPx - height);
  const firstRow = Math.floor(miniScroll / rowPx);
  const lastRow = Math.min(totalRows - 1, Math.ceil((miniScroll + height) / rowPx));
  return { miniScroll, sliderTop: ratio * sliderRange(input, sliderHeight), sliderHeight, firstRow, lastRow };
}

/** The `scrollTop` that puts the slider's top at `sliderTop` — dragging it. */
export function scrollForSlider(input: MinimapInput, sliderTop: number): number {
  const { sliderHeight } = minimapLayout(input);
  const range = sliderRange(input, sliderHeight);
  if (range <= 0) return 0;
  return (Math.min(range, Math.max(0, sliderTop)) / range) * maxScroll(input);
}

/** The `scrollTop` that centres the view on the row under minimap pixel `y` — a click beside the slider. */
export function scrollForClick(input: MinimapInput, y: number): number {
  const { miniScroll } = minimapLayout(input);
  const row = (y + miniScroll) / input.rowPx;
  return Math.min(maxScroll(input), Math.max(0, row * input.rowH - input.viewH / 2));
}

/** Whether minimap pixel `y` is on the slider. */
export function onSlider(input: MinimapInput, y: number): boolean {
  const { sliderTop, sliderHeight } = minimapLayout(input);
  return y >= sliderTop && y <= sliderTop + sliderHeight;
}

/** A piece of a row's text and its colour (a painted segment, `syntax/paint.ts`). */
export interface InkPiece {
  text: string;
  token?: string | null;
}

/** A stretch of non-blank characters in the minimap: `[from, to)` in cells, in one colour. */
export interface InkRun {
  from: number;
  to: number;
  token?: string | null;
}

/**
 * The cells a row's characters take in the minimap, as runs of non-blank
 * text per colour. Tabs reach the next stop, everything else one cell — the
 * minimap is a silhouette; a wide character one cell too narrow does not show.
 */
export function inkRuns(pieces: readonly InkPiece[], tabSize: number, indent = 0): InkRun[] {
  const runs: InkRun[] = [];
  let cell = indent;
  for (const piece of pieces) {
    let from = -1;
    for (const ch of piece.text) {
      const blank = ch === " " || ch === "\t";
      if (blank && from >= 0) {
        runs.push({ from, to: cell, token: piece.token });
        from = -1;
      } else if (!blank && from < 0) from = cell;
      cell = ch === "\t" ? (Math.floor(cell / tabSize) + 1) * tabSize : cell + 1;
    }
    if (from >= 0) runs.push({ from, to: cell, token: piece.token });
  }
  return runs;
}
