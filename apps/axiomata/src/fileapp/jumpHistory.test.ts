import { describe, expect, it } from "vitest";

import { keyAction } from "../editor/keymap";
import { editorKeyInput } from "./editorKeys";
import { JumpHistory, jumpKey, MAX_JUMPS, type JumpPlace } from "./jumpHistory";

const at = (rel: string, line = 0): JumpPlace => ({ root: "workspace", rel, line, col: 0 });
const key = (k: string, mods: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) => ({
  key: k,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe("JumpHistory", () => {
  it("goes back to where each jump started and forward again to where it was", () => {
    const h = new JumpHistory();
    h.record(at("a.md", 3)); // a → b
    h.record(at("b.md", 7)); // b → c
    expect(h.back(at("c.md", 1))).toEqual(at("b.md", 7));
    expect(h.back(at("b.md", 7))).toEqual(at("a.md", 3));
    expect(h.back(at("a.md", 3))).toBeNull();
    expect(h.forward()).toEqual(at("b.md", 7));
    expect(h.forward()).toEqual(at("c.md", 1));
    expect(h.forward()).toBeNull();
  });

  it("drops what was ahead on a new jump", () => {
    const h = new JumpHistory();
    h.record(at("a.md"));
    h.record(at("b.md"));
    h.back(at("c.md"));
    h.record(at("b.md", 4)); // from b somewhere new
    expect(h.forward()).toBeNull();
    expect(h.back(at("d.md"))).toEqual(at("b.md", 4));
    expect(h.back(null)).toEqual(at("a.md"));
  });

  it("has nothing to go back to before the first jump", () => {
    expect(new JumpHistory().back(at("a.md"))).toBeNull();
  });

  it("keeps the newest places only", () => {
    const h = new JumpHistory();
    for (let i = 0; i < MAX_JUMPS + 10; i++) h.record(at(`n${i}.md`));
    let count = 0;
    while (h.back(count === 0 ? at("here.md") : null)) count++;
    expect(count).toBe(MAX_JUMPS);
  });
});

describe("jumpKey", () => {
  it("is ⌃- / ⌃⇧- on the Mac", () => {
    expect(jumpKey(key("-", { ctrlKey: true }), "mac")).toBe("back");
    expect(jumpKey(key("_", { ctrlKey: true, shiftKey: true }), "mac")).toBe("forward");
    expect(jumpKey(key("-", { metaKey: true }), "mac")).toBeNull();
    expect(jumpKey(key("-"), "mac")).toBeNull();
  });

  it("is Ctrl+Alt+← / → elsewhere, where Ctrl+- zooms and Alt+← moves by a word", () => {
    expect(jumpKey(key("ArrowLeft", { altKey: true, ctrlKey: true }), "other")).toBe("back");
    expect(jumpKey(key("ArrowRight", { altKey: true, ctrlKey: true }), "other")).toBe("forward");
    expect(jumpKey(key("-", { ctrlKey: true }), "other")).toBeNull();
    expect(jumpKey(key("ArrowLeft", { altKey: true }), "other")).toBeNull();
  });

  it("takes no key the editor itself answers to off the Mac", () => {
    for (const [k, mods] of [
      ["ArrowLeft", { altKey: true, ctrlKey: true }],
      ["ArrowRight", { altKey: true, ctrlKey: true }],
    ] as const) {
      const input = editorKeyInput(key(k, mods), "other", false);
      expect(keyAction(input)).toBeNull();
    }
  });
});
