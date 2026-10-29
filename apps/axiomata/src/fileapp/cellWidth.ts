/**
 * The width of one character cell of the editor's font, read from a measuring span that holds `cells` characters
 * (`EditorSurface.svelte`). Every position the editor draws — the caret, selections, the gutter — and every click it
 * maps back to text is a multiple of this width, so it must be in the same pixels as the text's own layout.
 *
 * That is why it is the span's layout width, never its `getBoundingClientRect()`: the bounding box is drawn size,
 * transforms included, and a surface measures itself while the floating panel around it still grows out of the Dock
 * (`ui/fromOrigin.ts` starts at `scale(0.4)`). Measured then, a cell came out at a fraction of its width, the caret was
 * drawn at that fraction of the way along the line while the text went in at the real cursor, and it stayed so until
 * the next font change.
 *
 * The span must be laid out as a box (the surface positions it absolutely), so its computed `width` is the used one.
 * Returns `null` when there is no width yet (not rendered); the caller keeps the width it had.
 */
export function cellWidth(measurer: Element, cells: number): number | null {
  const width = parseFloat(getComputedStyle(measurer).width) / cells;
  return Number.isFinite(width) && width > 0 ? width : null;
}
