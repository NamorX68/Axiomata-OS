import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DockDrag } from "./dockDrag.svelte";
import { ROOT_NODE_ID } from "./dock";
import { singleGroupLayout, type Layout } from "./layout";

/** A pointer event jsdom has no constructor for. */
function pointer(type: string, x: number, y: number, extra: Partial<PointerEventInit> = {}): PointerEvent {
  const e = new Event(type, { bubbles: true }) as unknown as Record<string, unknown>;
  Object.assign(e, { clientX: x, clientY: y, pointerId: 1, button: 0, ...extra });
  return e as unknown as PointerEvent;
}

function rect(x: number, y: number, w: number, h: number): DOMRect {
  return { x, y, left: x, top: y, width: w, height: h, right: x + w, bottom: y + h, toJSON: () => ({}) } as DOMRect;
}

describe("DockDrag", () => {
  let dock: HTMLElement;
  let layout: Layout;
  const calls = { press: [] as string[], move: [] as Array<[string, unknown]>, resize: [] as unknown[][] };

  const make = () =>
    new DockDrag({
      dockEl: () => dock,
      layout: () => layout,
      onPress: (id) => calls.press.push(id),
      onMove: (id, target) => calls.move.push([id, target]),
      onResize: (...args) => calls.resize.push(args),
    });

  beforeEach(() => {
    layout = singleGroupLayout([
      { id: "a", kind: "terminal", title: "A", config: {} },
      { id: "b", kind: "terminal", title: "B", config: {} },
    ]);
    calls.press.length = 0;
    calls.move.length = 0;
    calls.resize.length = 0;
    dock = document.createElement("div");
    const group = document.createElement("div");
    group.dataset.ideGroup = layout.root.id;
    group.getBoundingClientRect = () => rect(0, 0, 200, 300);
    const bar = document.createElement("div");
    bar.dataset.ideTabbar = "";
    bar.getBoundingClientRect = () => rect(0, 0, 200, 30);
    for (const [i, id] of ["a", "b"].entries()) {
      const tab = document.createElement("div");
      tab.dataset.ideTab = id;
      tab.getBoundingClientRect = () => rect(i * 100, 0, 100, 30);
      bar.append(tab);
    }
    group.append(bar);
    dock.append(group);
    // A second group: the root's edge is only a target when there is something to split against.
    const other = document.createElement("div");
    other.dataset.ideGroup = "other";
    other.getBoundingClientRect = () => rect(200, 0, 200, 300);
    const otherBar = document.createElement("div");
    otherBar.dataset.ideTabbar = "";
    otherBar.getBoundingClientRect = () => rect(200, 0, 200, 30);
    other.append(otherBar);
    dock.append(other);
    dock.getBoundingClientRect = () => rect(0, 0, 400, 300);
    document.body.append(dock);
  });

  afterEach(() => dock.remove());

  it("presses a tab at once, but only drags after the pointer moved a few pixels", () => {
    const drag = make();
    drag.startTab("a", pointer("pointerdown", 10, 10));
    expect(calls.press).toEqual(["a"]);
    window.dispatchEvent(pointer("pointermove", 11, 10));
    expect(drag.draggingTab).toBeNull();
    window.dispatchEvent(pointer("pointermove", 60, 120));
    expect(drag.draggingTab).toBe("a");
    expect(drag.hint).not.toBeNull();
    drag.abandon();
  });

  it("drops the tab where the pointer is let go, and a plain click moves nothing", () => {
    const drag = make();
    drag.startTab("a", pointer("pointerdown", 10, 10));
    window.dispatchEvent(pointer("pointerup", 10, 10));
    expect(calls.move).toEqual([]);

    drag.startTab("a", pointer("pointerdown", 10, 10));
    window.dispatchEvent(pointer("pointermove", 100, 150));
    window.dispatchEvent(pointer("pointerup", 100, 150));
    expect(calls.move).toHaveLength(1);
    expect(calls.move[0][0]).toBe("a");
    expect(drag.draggingTab).toBeNull();
    expect(drag.hint).toBeNull();
  });

  it("resolves a drop on the whole dock's edge to the root's id", () => {
    const drag = make();
    drag.startTab("a", pointer("pointerdown", 10, 10));
    window.dispatchEvent(pointer("pointermove", 2, 150));
    expect(drag.rootHint).toBe("left");
    window.dispatchEvent(pointer("pointerup", 2, 150));
    expect((calls.move[0][1] as { nodeId: string }).nodeId).toBe(layout.root.id);
    expect((calls.move[0][1] as { nodeId: string }).nodeId).not.toBe(ROOT_NODE_ID);
  });

  it("ends a drag that was cancelled or abandoned, so the panes get the pointer back", () => {
    const drag = make();
    drag.startTab("a", pointer("pointerdown", 10, 10));
    window.dispatchEvent(pointer("pointermove", 200, 150));
    expect(drag.draggingTab).toBe("a");
    window.dispatchEvent(pointer("pointercancel", 200, 150));
    expect(drag.draggingTab).toBeNull();

    drag.startTab("b", pointer("pointerdown", 110, 10));
    window.dispatchEvent(pointer("pointermove", 200, 150));
    drag.abandon();
    expect(drag.draggingTab).toBeNull();
    window.dispatchEvent(pointer("pointerup", 200, 150));
    expect(calls.move).toEqual([]);
  });

  it("ignores a right-click and a second pointer", () => {
    const drag = make();
    drag.startTab("a", pointer("pointerdown", 10, 10, { button: 2 }));
    expect(calls.press).toEqual([]);
    drag.startTab("a", pointer("pointerdown", 10, 10));
    drag.startTab("b", pointer("pointerdown", 110, 10, { pointerId: 2 }));
    expect(calls.press).toEqual(["a"]);
    drag.abandon();
    vi.restoreAllMocks();
  });
});
