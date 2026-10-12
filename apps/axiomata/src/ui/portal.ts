/**
 * Moves an element to `document.body` for as long as it is mounted (a Svelte action). A `position: fixed` popover
 * inside a transformed ancestor (a panel that flew in, the Orbit's centred badge) would be placed relative to that
 * ancestor, and inside a scrolling column it would be clipped; under `body` it is placed relative to the window.
 */
export function portal(node: HTMLElement): { destroy: () => void } {
  document.body.appendChild(node);
  return {
    destroy: () => node.remove(),
  };
}
