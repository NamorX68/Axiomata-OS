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

/** What a card-sized thing is doing, for its ring: working, waiting for the owner, or neither. */
export type RingState = "working" | "waiting" | null | undefined;

const RING_ATTR = "data-ax-ring";

function applyRing(node: HTMLElement, state: RingState): void {
  if (state) node.setAttribute(RING_ATTR, state);
  else node.removeAttribute(RING_ATTR);
}

/**
 * `use:ring={state}` — the glow's form from card size up (`docs/plans/glass.md`, G11): working, a slow ring of Glut
 * light runs round the edge; waiting for the owner, a steady amber ring. One global rule (`[data-ax-ring]` in
 * `styles.css`), stilled by Calm and Off. The element must be positioned; small things (dots, tabs, chips) keep
 * `use:glow`.
 */
export const ring: Action<HTMLElement, RingState> = (node, state) => {
  applyRing(node, state);
  return {
    update: (next) => applyRing(node, next),
    destroy: () => node.removeAttribute(RING_ATTR),
  };
};

/** Glows while `on` is true. */
export const glow: Action<HTMLElement, boolean | undefined> = (node, on) => {
  apply(node, on);
  return {
    update: (next) => apply(node, next),
    destroy: () => node.removeAttribute(ATTR),
  };
};
