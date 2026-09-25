/**
 * The geometry Vi's scrolling needs from a surface (`docs/plans/editor.md`, ED3.2):
 * which logical lines are on screen (`H M L`, Ctrl-d/u) and where `zt zz zb` put
 * the scroller. Pure arithmetic over the surface's rows, kept out of
 * `EditorSurface.svelte` so it can be tested without a DOM.
 */

/** What of the visual layout the arithmetic reads (`VisualLayout` has it). */
export interface RowMap {
  readonly totalRows: number;
  /** The first visual row of logical line `line`. */
  firstRow(line: number): number;
  /** The logical line a visual row belongs to. */
  lineAt(row: number): { line: number };
}

/** The scroller's state, in pixels. */
export interface ScrollView {
  scrollTop: number;
  viewH: number;
  rowH: number;
}

/** The first and last logical line with a row fully on screen. */
export function visibleLines(layout: RowMap, view: ScrollView): { top: number; bottom: number } {
  const { scrollTop, viewH, rowH } = view;
  const top = layout.lineAt(Math.floor(scrollTop / rowH)).line;
  const lastRow = Math.max(0, Math.min(layout.totalRows - 1, Math.floor((scrollTop + viewH) / rowH) - 1));
  return { top, bottom: Math.max(top, layout.lineAt(lastRow).line) };
}

/** The `scrollTop` that puts line `line` at the top, centre or bottom of the view (`zt zz zb`). */
export function scrollTopFor(layout: RowMap, view: ScrollView, line: number, to: "top" | "center" | "bottom"): number {
  const { viewH, rowH } = view;
  const y = layout.firstRow(line) * rowH;
  const top = to === "top" ? y : to === "center" ? y - (viewH - rowH) / 2 : y - viewH + rowH;
  return Math.max(0, top);
}
