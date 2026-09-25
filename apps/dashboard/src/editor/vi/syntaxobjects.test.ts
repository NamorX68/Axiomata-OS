/**
 * The machine's dispatch to `env.syntaxObjects` (V9, ED3.4) for `if af ic ac
 * ia aa` — through a stub rather than a real tree-sitter tree, since
 * `syntax/objects.test.ts` already covers the real node-type table. Mirrors
 * `vi.test.ts`'s own `setup`/`vi` helpers, extended with a `syntaxObjects` env.
 */

import { describe, expect, it } from "vitest";

import type { TextStore } from "../buffer";
import { pos } from "../position";
import { show, docFrom, ctx } from "../testing";
import { ViMachine, ViShared, type SyntaxObjects, type ViEffect, type ViEnv } from "./machine";

/**
 * A stand-in grammar: `f`'s inner/around is the cursor's line without/with its
 * leading indent (like a one-line function body); `a`'s inner/around is the
 * first run of digits on the cursor's line, with `aa` taking one trailing
 * character (standing in for a trailing comma). Reads `store` fresh on every
 * call, like the real `EditorSurface.svelte` closure does off the highlighter.
 */
function stubSyntaxObjects(store: TextStore): SyntaxObjects {
  return (at, name, inner) => {
    const text = store.line(at.line);
    if (name === "f") {
      const indent = text.length - text.trimStart().length;
      if (inner) return { start: pos(at.line, indent), end: pos(at.line, text.length), linewise: false };
      return { start: pos(at.line, 0), end: pos(at.line, text.length), linewise: true };
    }
    if (name === "a") {
      const m = /\d+/.exec(text);
      if (!m) return null;
      const start = pos(at.line, m.index);
      const end = pos(at.line, m.index + m[0].length);
      return { start, end: inner ? end : pos(at.line, end.col + 1), linewise: false };
    }
    return null;
  };
}

function setup(marked: string, shared = new ViShared(null), envOverrides: Partial<ViEnv> = {}) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const env: ViEnv = {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    fileName: "notes.md",
    fileKey: "workspace:notes.md",
    syntaxObjects: stubSyntaxObjects(doc.store),
    ...envOverrides,
  };
  const m = new ViMachine(doc, shared, env);
  return { doc, m, effects, shared };
}

/** Runs `keys` on `before` through the stub and returns the text with the cursor marked. */
function run(before: string, keys: string, shared?: ViShared): string {
  const { doc, m } = setup(before, shared);
  m.feedKeys(keys);
  return show(doc);
}

describe("the machine dispatching if/af/ic/ac/ia/aa to env.syntaxObjects", () => {
  it("daf deletes the whole line the stub reports as the function object", () => {
    expect(run("  |foo bar\nkeep", "daf")).toBe("|keep");
  });

  it("cif changes only the stub's inner part, keeping the leading indent", () => {
    expect(run("  |foo bar", "cifbaz<Esc>")).toBe("  ba|z");
  });

  it("yia yanks the stub's inner object (a run of digits) into the unnamed register", () => {
    const { m, shared } = setup("|abc 42 def");
    m.feedKeys("yia");
    expect(shared.registers.get('"')).toEqual({ text: "42", kind: "char" });
  });

  it("daa deletes the digit run together with the stub's one trailing character", () => {
    expect(run("abc |42 def", "daa")).toBe("abc |def");
  });

  it("finds nothing (and rings the bell) when the stub returns null for the object", () => {
    const { m, doc, effects } = setup("|no digits here");
    m.feedKeys("dia");
    expect(show(doc)).toBe("|no digits here");
    expect(effects).toContainEqual({ type: "bell" });
  });

  it("repeats cia with . across lines, the stub re-resolving the object each time", () => {
    // The stub only looks at the cursor's line, so `.` on line 2 finds line 2's own digit run.
    expect(run("|12 x\n34 y", "cia3<Esc>j.")).toBe("3 x\n|3 y");
  });

  it("a read-only surface refuses daf: it rings the bell and leaves the text alone", () => {
    const doc = docFrom("  |foo bar");
    const effects: ViEffect[] = [];
    const shared = new ViShared(null);
    const env: ViEnv = {
      ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
      effect: (e) => effects.push(e),
      readOnly: true,
      syntaxObjects: stubSyntaxObjects(doc.store),
    };
    const m = new ViMachine(doc, shared, env);
    m.feedKeys("daf");
    expect(show(doc)).toBe("  |foo bar");
    expect(effects).toContainEqual({ type: "bell" });
  });
});
