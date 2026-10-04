import { describe, expect, it } from "vitest";

import { ctx, docFrom, show } from "../editor/testing";
import { ViShared, type ViEffect, type ViMode } from "../editor/vi/machine";
import type { ClipboardPort } from "../editor/vi/registers";
import { pos, range } from "../editor/position";
import { ViSurface, type ViStatus, type ViSurfaceHost } from "./viSurface";

/** A synchronous fake of the Mac clipboard (V3) — no promise, so `Registers` never has to wait. */
class FakeClipboard implements ClipboardPort {
  text = "";
  read(): string {
    return this.text;
  }
  write(text: string): void {
    this.text = text;
  }
}

const press = (key: string, mods: { meta?: boolean; alt?: boolean; shift?: boolean; ctrl?: boolean } = {}) => ({
  key,
  meta: mods.meta ?? false,
  alt: mods.alt ?? false,
  shift: mods.shift ?? false,
  ctrl: mods.ctrl ?? false,
});

/** A host recording everything it was told, so a test can assert on the call log rather than mocks. */
function makeHost(overrides: Partial<ViSurfaceHost> = {}) {
  const statuses: ViStatus[] = [];
  const effects: ViEffect[] = [];
  let changedCount = 0;
  const host: ViSurfaceHost = {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    changed: () => {
      changedCount++;
    },
    effect: (e) => effects.push(e),
    status: (s) => statuses.push(s),
    readOnly: false,
    fileName: "notes.md",
    fileKey: "workspace:notes.md",
    ...overrides,
  };
  return { host, statuses, effects, changedCount: () => changedCount };
}

function setup(marked: string, hostOverrides: Partial<ViSurfaceHost> = {}, clipboard = new FakeClipboard()) {
  const doc = docFrom(marked);
  const shared = new ViShared(clipboard);
  const { host, statuses, effects, changedCount } = makeHost(hostOverrides);
  const surface = new ViSurface(doc, shared, host);
  return { doc, surface, statuses, effects, changedCount, clipboard, shared };
}

describe("ViSurface.keydown (ED3.2)", () => {
  it("leaves a plain character to the textarea", () => {
    const { surface, changedCount } = setup("|foo");
    expect(surface.keydown(press("d"))).toEqual({ handled: false });
    expect(changedCount()).toBe(0);
  });

  it("carries out a Mac effect (⌘S) without feeding the machine", () => {
    const { surface, changedCount } = setup("|foo");
    expect(surface.keydown(press("s", { meta: true }))).toEqual({ handled: true, effect: "save" });
    expect(changedCount()).toBe(0);
  });

  it("feeds a single Vi key (Escape) and reports the new status", () => {
    const { surface, doc, statuses } = setup("|foo");
    surface.typed("i");
    surface.typed("X");
    expect(surface.keydown(press("Escape"))).toEqual({ handled: true });
    expect(show(doc)).toBe("|Xfoo");
    expect(statuses[statuses.length - 1]?.mode).toBe("normal");
  });

  it("⌘C yanks into the shared clipboard register (\"+yy)", () => {
    const { surface, clipboard, changedCount } = setup("|line one\nline two", {});
    expect(surface.keydown(press("c", { meta: true }))).toEqual({ handled: true });
    expect(clipboard.text).toBe("line one\n");
    expect(changedCount()).toBe(1);
  });

  it("⌘Z replays as 'u' (undo) through the machine", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("i");
    surface.typed("X");
    surface.keydown(press("Escape"));
    expect(show(doc)).toBe("|Xfoo");
    expect(surface.keydown(press("z", { meta: true }))).toEqual({ handled: true });
    expect(show(doc)).toBe("|foo");
  });

  it("bells instead of changing a read-only surface", () => {
    // "x" itself reaches the machine through `typed` (V12); `readOnly` is wired into the
    // machine at construction time, from the host given to `ViSurface`'s constructor.
    const { surface, doc, effects } = setup("|foo", { readOnly: true });
    surface.typed("x");
    expect(show(doc)).toBe("|foo");
    expect(effects).toContainEqual({ type: "bell" });
  });
});

describe("ViSurface.typed (V12)", () => {
  it("does nothing for empty text", () => {
    const { surface, changedCount } = setup("|foo");
    surface.typed("");
    expect(changedCount()).toBe(0);
  });

  it("types characters as Vi keys in Normal mode", () => {
    const { surface, doc } = setup("|foo bar");
    surface.typed("dw");
    expect(show(doc)).toBe("|bar");
  });

  it("types text as one insertion in Insert mode", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("i");
    surface.typed("hi ");
    expect(show(doc)).toBe("hi |foo");
  });

  it("types text as one insertion in Replace mode", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("R");
    surface.typed("XY");
    expect(show(doc)).toBe("XY|o");
  });
});

describe("ViSurface.deadKey (V12)", () => {
  it("takes the composed character as a key at once outside Insert mode", () => {
    const { surface, doc } = setup("|foo");
    expect(surface.deadKey("l")).toBe(true);
    expect(show(doc)).toBe("f|oo");
  });

  it("swallows the compositionend `typed` that follows a taken dead key", () => {
    const { surface, doc } = setup("|foo");
    surface.deadKey("l");
    expect(show(doc)).toBe("f|oo");
    // The browser still fires `typed("l")` once composition ends — it must not move twice.
    surface.typed("l");
    expect(show(doc)).toBe("f|oo");
    // The swallow only covers the one compositionend; the next real key works normally.
    surface.typed("l");
    expect(show(doc)).toBe("fo|o");
  });

  it("does nothing while typing (Insert mode composes for real)", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("i");
    expect(surface.deadKey("^")).toBe(false);
    expect(show(doc)).toBe("|foo");
  });

  it("does nothing for empty composed text", () => {
    const { surface } = setup("|foo");
    expect(surface.deadKey("")).toBe(false);
  });
});

describe("ViSurface.visualRanges (V2, V8)", () => {
  it("is null outside Visual mode", () => {
    const { surface, doc } = setup("|foo");
    expect(surface.visualRanges(doc, 4)).toBeNull();
  });

  it("covers one range for a characterwise selection", () => {
    const { surface, doc } = setup("|foo bar");
    surface.machine.feedKeys("vll");
    expect(surface.visualRanges(doc, 4)).toEqual([range(pos(0, 0), pos(0, 3))]);
  });

  it("covers the full width of every selected line", () => {
    const { surface, doc } = setup("|foo\nbar baz");
    surface.machine.feedKeys("Vj");
    expect(surface.visualRanges(doc, 4)).toEqual([range(pos(0, 0), pos(1, 7))]);
    expect(surface.lineVisual).toBe(true);
  });

  it("covers one range per line for a block selection, short lines clipped to their end", () => {
    const { surface, doc } = setup("ab|cdef\nab\nabcdef");
    // <C-v> starts a block at (0,2); ll extends it to (0,4); jj drops two lines, keeping the column.
    surface.machine.feedKeys("<C-v>lljj");
    const ranges = surface.visualRanges(doc, 4);
    expect(ranges).toHaveLength(3);
    // The middle line ("ab", length 2) has nothing at columns 2-4: an empty range at its end.
    expect(ranges?.[1]).toEqual(range(pos(1, 2), pos(1, 2)));
    expect(ranges?.[0].start).toEqual(pos(0, 2));
    expect(ranges?.[2].start.line).toBe(2);
  });
});

describe("ViSurface.placeCursor / selectVisual (mouse input)", () => {
  it("moves the cursor and reports status, without a full redraw", () => {
    const { surface, doc, statuses, changedCount } = setup("|foo bar");
    surface.placeCursor(pos(0, 4));
    expect(doc.selection.head).toEqual(pos(0, 4));
    expect(statuses[statuses.length - 1]?.mode).toBe("normal");
    expect(changedCount()).toBe(0);
  });

  it("drags out a characterwise Visual selection", () => {
    const { surface, doc, statuses } = setup("|foo bar");
    surface.selectVisual(pos(0, 0), pos(0, 3));
    expect((statuses[statuses.length - 1] as ViStatus).mode).toBe("visual");
    expect(surface.visualRanges(doc, 4)).toEqual([range(pos(0, 0), pos(0, 4))]);
  });

  it("leaves Visual mode and drops a pending command on a plain click", () => {
    const { surface, doc } = setup("|foo bar");
    surface.machine.feedKeys("v");
    surface.placeCursor(pos(0, 5));
    expect(surface.visualRanges(doc, 4)).toBeNull();
    expect(doc.selection.head).toEqual(pos(0, 5));
  });
});

describe("ViSurface.pasted (Mac paste)", () => {
  it("is ignored outside Insert/Replace mode", () => {
    const { surface, doc, changedCount } = setup("|foo");
    surface.pasted("XYZ");
    expect(show(doc)).toBe("|foo");
    expect(changedCount()).toBe(0);
  });

  it("inserts text in Insert mode", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("i");
    surface.pasted("XYZ");
    expect(show(doc)).toBe("XYZ|foo");
  });

  it("inserts text in Replace mode", () => {
    const { surface, doc } = setup("|foo");
    surface.typed("R");
    surface.pasted("XY");
    expect(show(doc)).toBe("XY|o");
  });
});

describe("ViSurface status and cursor shape (V8)", () => {
  it("mirrors the machine's status and shape", () => {
    const { surface } = setup("|foo");
    expect(surface.status()).toEqual({ mode: "normal", pending: "", recording: null, cmdline: null, message: null });
    expect(surface.cursorShape()).toBe("block");
    surface.typed("i");
    expect(surface.cursorShape()).toBe("bar");
    expect(surface.status().mode).toBe("insert" satisfies ViMode);
  });
});

describe("ViSurface on the command line (ED3.3)", () => {
  it("counts the command line as typing, so text and dead keys go into it", () => {
    const { surface } = setup("|foo bar");
    surface.typed("/");
    expect(surface.typing).toBe(true);
    expect(surface.deadKey("^")).toBe(false);
    surface.typed("bä");
    expect(surface.status().cmdline?.text).toBe("bä");
  });

  it("sends Enter as <CR> and a paste into the line", () => {
    const { surface } = setup("|foo bar");
    surface.typed("/");
    surface.pasted("ba");
    expect(surface.keydown(press("Enter"))).toEqual({ handled: true });
    expect(surface.status().cmdline).toBeNull();
    expect(surface.machine.cursor).toEqual({ line: 0, col: 4 });
  });

  it("reveals the incsearch match while typing, the cursor otherwise", () => {
    const { surface } = setup("|foo\nbar");
    surface.typed("/");
    surface.typed("bar");
    expect(surface.revealTarget()).toEqual({ line: 1, col: 0 });
    expect(surface.searchHighlights(0, 1).current).toEqual({ start: { line: 1, col: 0 }, end: { line: 1, col: 3 } });
    surface.keydown(press("Escape"));
    expect(surface.revealTarget()).toEqual({ line: 0, col: 0 });
  });
});
