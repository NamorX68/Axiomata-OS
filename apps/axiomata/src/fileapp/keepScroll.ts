/**
 * Marks an element whose scroll position must survive being moved in the DOM
 * (`docs/plans/git-layer.md`, CP9). Moving a node resets the scroll position of
 * everything in it, and no scroll event says so — an editor surface that draws
 * only the rows on screen then draws the wrong ones, over a blank band. The IDE
 * dock moves panes on every layout change (`ide/paneStore.ts`) and puts marked
 * elements' positions back; opt-in, so a move need not visit every element.
 *
 * Here rather than in `ide/` because the elements that carry it live in the
 * file app, which must not import the IDE. In markup: `{...KEEP_SCROLL}`.
 */

export const KEEP_SCROLL_ATTR = "data-keep-scroll";

/** Spread onto an element to mark it. */
export const KEEP_SCROLL = { [KEEP_SCROLL_ATTR]: "" } as const;
