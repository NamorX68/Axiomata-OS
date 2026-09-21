import { beforeEach, describe, expect, it } from "vitest";

import { PANE_ATTR, SLOT_ATTR, parkPanes, placePanes } from "./paneStore";

/**
 * Builds a dock: a store holding panes, and slots wherever the tree wants one.
 * `panes` and `slots` are lists of tab ids.
 */
function dock(panes: string[], slots: string[]): HTMLElement {
  const root = document.createElement("div");
  const store = document.createElement("div");
  store.className = "store";
  for (const id of panes) {
    const pane = document.createElement("div");
    pane.setAttribute(PANE_ATTR, id);
    pane.dataset.testid = `pane-${id}`;
    store.appendChild(pane);
  }
  root.appendChild(store);

  const tree = document.createElement("div");
  for (const id of slots) {
    const slot = document.createElement("div");
    slot.setAttribute(SLOT_ATTR, id);
    tree.appendChild(slot);
  }
  root.appendChild(tree);

  document.body.appendChild(root);
  return root;
}

function paneIn(root: HTMLElement, id: string): Element | null {
  return root.querySelector(`[${SLOT_ATTR}="${id}"] > [${PANE_ATTR}="${id}"]`);
}

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("parkPanes", () => {
  it("brings every pane home before the tree is rebuilt", () => {
    const root = dock(["a", "b"], ["a", "b"]);
    const store = root.querySelector(".store")!;
    placePanes(root);
    expect(store.children).toHaveLength(0);

    expect(parkPanes(root, store)).toBe(2);
    expect(store.children).toHaveLength(2);
  });

  it("leaves a pane that is already parked where it is", () => {
    const root = dock(["a"], []);
    const store = root.querySelector(".store")!;
    expect(parkPanes(root, store)).toBe(0);
    expect(store.children).toHaveLength(1);
  });

  it("keeps the same elements — parking must not rebuild anything either", () => {
    const root = dock(["a"], ["a"]);
    const store = root.querySelector(".store")!;
    const pane = root.querySelector(`[${PANE_ATTR}="a"]`);
    placePanes(root);
    parkPanes(root, store);
    expect(store.firstElementChild).toBe(pane);
  });
});

describe("placePanes", () => {
  it("moves each pane into the slot that names it", () => {
    const root = dock(["a", "b"], ["a", "b"]);
    const result = placePanes(root);

    expect(paneIn(root, "a")).not.toBeNull();
    expect(paneIn(root, "b")).not.toBeNull();
    expect(result).toEqual([
      { paneId: "a", moved: true, orphaned: false },
      { paneId: "b", moved: true, orphaned: false },
    ]);
  });

  it("keeps the very same element — the whole point", () => {
    const root = dock(["a"], ["a"]);
    const before = root.querySelector(`[${PANE_ATTR}="a"]`);
    placePanes(root);
    // Identity, not equality: a recreated node would mean a recreated
    // component, a closed PTY and a restarted agent.
    expect(paneIn(root, "a")).toBe(before);
  });

  it("does nothing to a pane already in place", () => {
    const root = dock(["a"], ["a"]);
    placePanes(root);
    expect(placePanes(root)).toEqual([{ paneId: "a", moved: false, orphaned: false }]);
  });

  it("follows a pane to a different slot, still the same element", () => {
    // What dragging a tab into another group does: the old slot is destroyed
    // and a new one appears elsewhere. Parking first is what keeps the pane
    // out of the wreckage — see `parkPanes`.
    const root = dock(["a"], ["a"]);
    placePanes(root);
    const pane = paneIn(root, "a");
    const store = root.querySelector(".store")!;

    parkPanes(root, store);
    root.querySelector(`[${SLOT_ATTR}="a"]`)!.remove();
    const other = document.createElement("div");
    other.setAttribute(SLOT_ATTR, "a");
    root.appendChild(other);

    expect(placePanes(root)).toEqual([{ paneId: "a", moved: true, orphaned: false }]);
    expect(other.firstElementChild).toBe(pane);
  });

  it("would lose the pane without parking — which is why parking exists", () => {
    const root = dock(["a"], ["a"]);
    placePanes(root);
    const pane = paneIn(root, "a")!;

    // Destroying the slot with the pane still inside takes the pane with it.
    root.querySelector(`[${SLOT_ATTR}="a"]`)!.remove();
    expect(pane.isConnected).toBe(false);
    expect(root.querySelector(`[${PANE_ATTR}="a"]`)).toBeNull();
  });

  it("leaves a pane alone when no slot wants it, rather than detaching it", () => {
    const root = dock(["a"], []);
    const pane = root.querySelector(`[${PANE_ATTR}="a"]`);

    expect(placePanes(root)).toEqual([{ paneId: "a", moved: false, orphaned: true }]);
    // Still in the store, still in the document: "no slot" is a transient
    // state mid-layout-change, and detaching is the destruction we avoid.
    expect(pane?.isConnected).toBe(true);
  });

  it("ignores a slot with no pane to put in it", () => {
    const root = dock([], ["a"]);
    expect(placePanes(root)).toEqual([]);
  });

  it("places into the first of two slots claiming the same pane", () => {
    const root = dock(["a"], ["a", "a"]);
    placePanes(root);
    const slots = root.querySelectorAll(`[${SLOT_ATTR}="a"]`);
    expect(slots[0].children).toHaveLength(1);
    expect(slots[1].children).toHaveLength(0);
  });

  it("survives a pane whose attribute is empty", () => {
    const root = dock([], []);
    const broken = document.createElement("div");
    broken.setAttribute(PANE_ATTR, "");
    root.appendChild(broken);
    expect(placePanes(root)).toEqual([]);
  });
});
