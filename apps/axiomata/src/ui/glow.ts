/**
 * `use:glow={on}` — the glow ("Glut", `docs/plans/ui-ux-round.md` U1) on any element: a soft shine around it that
 * breathes, for as long as something there lives, works or needs the owner. The look is one global rule
 * (`[data-ax-glow]` in `styles.css`), so every glowing thing glows alike and the motion setting stills all of them.
 *
 * Reserved for the accent rule's cases (a running agent, skill or routine, a waiting question): a glow on everything
 * would mean nothing.
 */

import type { Action } from "svelte/action";

const ATTR = "data-ax-glow";

function apply(node: HTMLElement, on: boolean | undefined): void {
  if (on) node.setAttribute(ATTR, "");
  else node.removeAttribute(ATTR);
}

/** Glows while `on` is true. */
export const glow: Action<HTMLElement, boolean | undefined> = (node, on) => {
  apply(node, on);
  return {
    update: (next) => apply(node, next),
    destroy: () => node.removeAttribute(ATTR),
  };
};
