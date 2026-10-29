import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import { panelOpen } from "../core/chat";
import ChatPanel from "./ChatPanel.svelte";
import Window from "./Window.svelte";

// jsdom has no Web Animations; Svelte's transitions only need an animation that finishes.
beforeAll(() => {
  Element.prototype.animate ??= function (this: Element) {
    const animation = { onfinish: null as null | (() => void), cancel() {}, currentTime: 0, effect: null };
    queueMicrotask(() => animation.onfinish?.());
    return animation as unknown as Animation;
  };
});

const body = createRawSnippet(() => ({ render: () => "<p>body</p>" }));

const mounted: (() => void)[] = [];
afterEach(() => {
  while (mounted.length) mounted.pop()?.();
  panelOpen.set(false);
});

function mountWindow(props: Record<string, unknown>): HTMLElement {
  const target = document.body.appendChild(document.createElement("div"));
  const app = mount(Window, { target, props: { title: "T", onClose: () => {}, children: body, ...props } });
  mounted.push(() => {
    void unmount(app);
    target.remove();
  });
  flushSync();
  return target.querySelector(".window") as HTMLElement;
}

describe("Window", () => {
  it("keeps the caller's own size and placement when it is neither movable nor resizable", () => {
    for (const modal of [true, false]) {
      const win = mountWindow({ modal, style: "width: 100%; height: 100%; max-height: 50vh;" });
      expect(win.style.width, `modal=${modal}`).toBe("100%");
      expect(win.style.height, `modal=${modal}`).toBe("100%");
      expect(win.style.maxHeight).toBe("50vh");
      expect(win.style.left).toBe("");
      expect(win.style.top).toBe("");
      expect(win.classList.contains("positioned")).toBe(false);
      expect(win.classList.contains("movable")).toBe(false);
      expect(win.classList.contains("resizable")).toBe(false);
    }
  });

  it("sizes a resizable window itself, over the caller's style", () => {
    const win = mountWindow({ resizable: true, defaultSize: { w: 600, h: 400 }, style: "width: 100%;" });
    expect(win.style.width).toBe("600px");
    expect(win.style.height).toBe("400px");
  });
});

describe("ChatPanel", () => {
  it("fills its docked frame above the assistant bar", () => {
    const target = document.body.appendChild(document.createElement("div"));
    panelOpen.set(true);
    const app = mount(ChatPanel, { target });
    mounted.push(() => {
      void unmount(app);
      target.remove();
    });
    flushSync();
    const wrap = target.querySelector(".chat-wrap") as HTMLElement;
    const win = wrap.querySelector(".window") as HTMLElement;
    // `.chat-wrap` is fixed to the bottom with the panel's height; the window must fill it, else it hangs from the
    // frame's top edge in the middle of the screen and grows downward.
    expect(win.style.width).toBe("100%");
    expect(win.style.height).toBe("100%");
    expect(win.classList.contains("positioned")).toBe(false);
  });
});
