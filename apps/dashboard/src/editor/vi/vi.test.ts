/**
 * The Vi machine as a table: text before (`|` marks the cursor, which in
 * Normal mode sits *on* the character after it), the keys typed in Vim
 * notation, and the text after — as `docs/plans/editor.md` §6 asks for.
 */

import { describe, expect, it } from "vitest";

import { show, docFrom, ctx } from "../testing";
import { parseKeys, keysToText } from "./keys";
import { ViMachine, ViShared, type ViEffect } from "./machine";
import { parse } from "./parse";
import { pos, range } from "../position";

function setup(marked: string, shared = new ViShared(null)) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const m = new ViMachine(doc, shared, {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    fileName: "notes.md",
    fileKey: "workspace:notes.md",
  });
  return { doc, m, effects, shared };
}

/** Runs `keys` on `before` and returns the text with the cursor marked. */
function vi(before: string, keys: string, shared?: ViShared): string {
  const { doc, m } = setup(before, shared);
  m.feedKeys(keys);
  return show(doc);
}

type Case = [string, string, string];

function table(name: string, cases: Case[]) {
  describe(name, () => {
    for (const [before, keys, after] of cases) {
      it(`${JSON.stringify(before)} ${keys}`, () => {
        expect(vi(before, keys)).toBe(after);
      });
    }
  });
}

describe("keys", () => {
  it("parses Vim notation and writes it back", () => {
    expect(parseKeys("ci(<Esc>")).toEqual(["c", "i", "(", "<Esc>"]);
    expect(parseKeys("a<lt>b<C-r>a")).toEqual(["a", "<", "b", "<C-r>", "a"]);
    expect(parseKeys("<nope>")).toEqual(["<", "n", "o", "p", "e", ">"]);
    expect(keysToText(["d", "<", "<Esc>", { text: "x<y" }])).toBe("d<lt><Esc>x<lt>y");
  });
});

describe("parse", () => {
  const done = (keys: string) => {
    const r = parse(parseKeys(keys), "normal");
    return r.status === "done" ? r.command : r.status;
  };
  it("multiplies counts and keeps the keys without them", () => {
    expect(done("2d3w")).toMatchObject({ count: 6, body: { kind: "operator", op: "d", target: { name: "w" } } });
    expect(done('"a2yy')).toMatchObject({ register: "a", count: 2, body: { target: { kind: "line" } } });
    expect((done("2d3w") as { plain: unknown }).plain).toEqual(["d", "w"]);
  });
  it("waits for what is missing and refuses what cannot be", () => {
    expect(done("d")).toBe("incomplete");
    expect(done("g")).toBe("incomplete");
    expect(done("gU")).toBe("incomplete");
    expect(done("di")).toBe("incomplete");
    expect(done("dq")).toBe("invalid");
    expect(done("0")).toMatchObject({ body: { kind: "motion", name: "0" } });
    expect(done("gUiw")).toMatchObject({ body: { op: "gU", target: { kind: "object", name: "w", inner: true } } });
    expect(done("gcc")).toMatchObject({ body: { op: "gc", target: { kind: "line" } } });
    expect(done("ysiw)")).toMatchObject({ body: { op: "ys", char: ")" } });
    expect(done("cs'\"")).toMatchObject({ body: { kind: "command", name: "cs", char: "'", char2: '"' } });
    expect(done("fx")).toMatchObject({ body: { kind: "motion", name: "f", char: "x" } });
  });
});

table("motions", [
  ["|foo bar baz", "w", "foo |bar baz"],
  ["|foo.bar baz", "w", "foo|.bar baz"],
  ["|foo.bar baz", "W", "foo.bar |baz"],
  ["foo |bar", "w", "foo ba|r"],
  ["foo\n\n|bar", "b", "foo\n|\nbar"],
  ["foo |bar baz", "2w", "foo bar ba|z"],
  ["|foo bar", "e", "fo|o bar"],
  ["fo|o bar", "e", "foo ba|r"],
  ["foo ba|r", "b", "foo |bar"],
  ["foo ba|r", "ge", "fo|o bar"],
  ["  |  foo", "0", "|    foo"],
  ["|    foo", "^", "    |foo"],
  ["|foo bar", "$", "foo ba|r"],
  ["|a\nb\nc", "2j", "a\nb\n|c"],
  ["a\nb\n|c", "gg", "|a\nb\nc"],
  ["|a\nb\nc", "G", "a\nb\n|c"],
  ["|a\nb\nc", "2G", "a\n|b\nc"],
  ["|foo(bar)", "fb", "foo(|bar)"],
  ["|foo(bar)", "t)", "foo(ba|r)"],
  ["foo(ba|r)", "F(", "foo|(bar)"],
  ["|a,b,c", "f,;", "a,b|,c"],
  ["|a,b,c", "f,;,", "a|,b,c"],
  ["|fn a(b(c)) {}", "%", "fn a(b(c)|) {}"],
  ["fn a(b(c)|) {}", "%", "fn a|(b(c)) {}"],
  ["|a\nb\n\nc\nd", "}", "a\nb\n|\nc\nd"],
  ["a\nb\n\nc\n|d", "{", "a\nb\n|\nc\nd"],
  ["|ab\ncdef\ngh", "lljj", "ab\ncdef\ng|h"],
  ["|abcdef\nab\nabcdef", "$jj", "abcdef\nab\nabcde|f"],
  ["one. |two. three.", ")", "one. two. |three."],
  ["|a\n  b", "+", "a\n  |b"],
  ["x|yz", "h", "|xyz"],
  ["|xyz", "3l", "xy|z"],
]);

table("operators with motions and objects", [
  ["|foo bar baz", "dw", "|bar baz"],
  ["foo |bar", "dw", "foo| "],
  ["|foo bar baz", "d2w", "|baz"],
  ["|foo bar baz", "2dw", "|baz"],
  ["foo |bar baz", "cwqux<Esc>", "foo qu|x baz"],
  ["|foo bar", "d$", "|"],
  ["foo |bar", "D", "foo| "],
  ["|a\nb\nc", "dd", "|b\nc"],
  ["a\nb\n|c", "dd", "a\n|b"],
  ["|a\nb\nc", "2dd", "|c"],
  ["|a\nb\nc", "dj", "|c"],
  ["a\n|b\nc", "dk", "|c"],
  ["|a\nb\nc", "dG", "|"],
  ["foo(|bar, baz)", "di(", "foo(|)"],
  ["foo(|bar, baz)", "da(", "fo|o"],
  ['say "|hi there" now', 'ci"yo<Esc>', 'say "y|o" now'],
  ['say "|hi" now', 'da"', "say |now"],
  ["fn f() {\n    |a();\n    b();\n}", "di{", "fn f() {\n|}"],
  ["x = [1, |2]", "ci[3<Esc>", "x = [|3]"],
  ["foo |bar baz", "diw", "foo | baz"],
  ["foo |bar baz", "daw", "foo |baz"],
  ["<a><b>|x</b></a>", "dit", "<a><b>|</b></a>"],
  ["<a><b>|x</b></a>", "d2at", "|"],
  ["|a\nb\n\nc", "dap", "|c"],
  ["|a\nb\n\nc", "yipP", "|a\nb\na\nb\n\nc"],
  ["|foo bar", "gUiw", "|FOO bar"],
  ["|Foo Bar", "g~~", "|fOO bAR"],
  ["|FOO bar", "guw", "|foo bar"],
  ["|a\nb", ">j", "    |a\n    b"],
  ["    |a", "<<", "|a"],
  // The file indents by two, so `>>` does too.
  ["|a\n  b\nc", ">>", "  |a\n  b\nc"],
  ["|a {\nb\n}", "=G", "|a {\n    b\n}"],
  ["|a\nb", "gcj", "|// a\n// b"],
  ["// |a", "gcc", "|a"],
  ["|foo bar", "ysiw)", "|(foo) bar"],
  ["|foo bar", "ysiw(", "|( foo ) bar"],
  ["(|foo) bar", "cs)]", "|[foo] bar"],
  ["'|foo' bar", "cs'\"", '|"foo" bar'],
  ["[ |foo ] bar", "ds[", "|foo bar"],
  ["  |foo", "yss\"", '  |"foo"'],
]);

table("commands", [
  ["|abc", "x", "|bc"],
  ["ab|c", "x", "a|b"],
  ["ab|c", "X", "a|c"],
  ["|abc", "3x", "|"],
  ["|abc", "sX<Esc>", "|Xbc"],
  ["|a\nb", "Sx<Esc>", "|x\nb"],
  ["  |abc", "Cz<Esc>", "  |z"],
  ["|abc", "rx", "|xbc"],
  ["|abc", "3rx", "xx|x"],
  ["|abc", "4rx", "|abc"],
  ["|abc", "~~", "AB|c"],
  ["|a\nb\nc", "J", "a| b\nc"],
  ["|a\n   b", "gJ", "a|   b"],
  ["|a\nb\nc", "3J", "a b| c"],
  ["|x 7 y", "<C-a>", "x |8 y"],
  ["|x 7 y", "10<C-x>", "x -|3 y"],
  ["|x 0x0f", "<C-a>", "x 0x1|0"],
  ["|abc", "ix<Esc>", "|xabc"],
  ["|abc", "ax<Esc>", "a|xbc"],
  ["  a|bc", "Ix<Esc>", "  |xabc"],
  ["a|bc", "Ax<Esc>", "abc|x"],
  ["|abc", "3ix<Esc>", "xx|xabc"],
  ["  |a", "ob<Esc>", "  a\n  |b"],
  ["  |a", "Ob<Esc>", "  |b\n  a"],
  ["|a", "2ob<Esc>", "a\nb\n|b"],
  ["|abcd", "Rxy<Esc>", "x|ycd"],
  ["|abcd", "Rxy<BS><Esc>", "|xbcd"],
  ["|ab", "ifoo<CR>bar<Esc>", "foo\nba|rab"],
  ["foo ba|r", "a baz<C-w><Esc>", "foo bar| "],
  ["|a", "ib<C-t><Esc>", "    |ba"],
]);

table("put and registers", [
  ["|foo bar", "yiwP", "fo|ofoo bar"],
  ["|foo bar", "yiwwp", "foo bfo|oar"],
  ["|a\nb", "yyp", "a\n|a\nb"],
  ["|a\nb", "yyP", "|a\na\nb"],
  ["|a\nb", "yy2p", "a\n|a\na\nb"],
  ["|a\nb", "ddp", "b\n|a"],
  ["|foo bar", '"ayiww"ap', "foo bfo|oar"],
  ["|foo bar", '"ayiw"Ayiww"aP', "foo foofo|obar"],
  ["|foo bar", "dwP", "foo| bar"],
  ["|one two", '"_dwP', "|two"],
  ["|a\nb", "yyjgp", "a\nb\n|a"],
  ["|foo bar", "yiwwviwp", "foo fo|o"],
  ["|abc", "xp", "b|ac"],
]);

table("visual", [
  ["|foo bar", "vlld", "| bar"],
  ["foo |bar", "vhd", "foo|ar"],
  ["|a\nb\nc", "Vjd", "|c"],
  ["|foo bar", "veU", "|FOO bar"],
  ["|foo bar", "vey$p", "foo barfo|o"],
  ["|a\nb\nc", "Vj>", "    |a\n    b\nc"],
  ["|a\nb\nc", "VjJ", "a| b\nc"],
  ["|abc\nabc\nabc", "<C-v>jjld", "|c\nc\nc"],
  ["|abc\nabc\nabc", "<C-v>jjIx<Esc>", "|xabc\nxabc\nxabc"],
  ["|abc\nabc\nabc", "<C-v>jj$Ax<Esc>", "abc|x\nabcx\nabcx"],
  ["|abc\nabc", "<C-v>jlcX<Esc>", "|Xc\nXc"],
  ["|abcd", "vllrx", "|xxxd"],
  ["|foo (bar)", "f(vi(d", "foo (|)"],
  ["|foo bar", "vllSx", "|xfoox bar"],
  ["|foo bar", "vll<Esc>gvd", "| bar"],
  ["|foo bar", "vlloh d", "f| bar"],
]);

table("repeat", [
  ["|a b c d", "dw.", "|c d"],
  ["|a b c d", "dw2.", "|d"],
  ["|abc abc", "cwx<Esc>w.", "x |x"],
  ["|a\na\na", "Ax<Esc>j.j.", "ax\nax\na|x"],
  ["|abcdef", "vld.", "|ef"],
  ["|a\nb\nc\nd", "Vjd.", "|"],
  ["|x x x", "rZw.w.", "Z Z |Z"],
]);

table("undo", [
  ["|foo bar", "cwbaz<Esc>u", "|foo bar"],
  ["|foo", "ia<Esc>ib<Esc>u", "|afoo"],
  ["|foo", "xxu<C-r>", "|o"],
  ["|a b", "dwdwuu", "|a b"],
]);

describe("macros", () => {
  it("records into a register and plays it back with a count", () => {
    expect(vi("|a1\na1\na1\na1", "qqA!<Esc>jq2@q")).toBe("a1!\na1!\na1!\na|1");
  });
  it("repeats the last macro with @@", () => {
    expect(vi("|x\nx\nx", "qwix<Esc>jq@w@@")).toBe("xx\nxx\n|xx");
  });
  it("stores the macro as Vim notation", () => {
    const { m, shared } = setup("|a");
    m.feedKeys("qqciw<lt>b<Esc>q");
    expect(shared.registers.get("q")?.text).toBe("ciw<lt>b<Esc>");
  });
});

describe("marks and jumps", () => {
  it("jumps to a mark by line or exactly", () => {
    expect(vi("a\nb|cd\ne", "majj'a")).toBe("a\n|bcd\ne");
    expect(vi("a\nb|cd\ne", "majj`a")).toBe("a\nb|cd\ne");
  });
  it("goes back and forth along the jump list", () => {
    expect(vi("|a\nb\nc\nd", "G<C-o>")).toBe("|a\nb\nc\nd");
    expect(vi("|a\nb\nc\nd", "G<C-o><C-i>")).toBe("a\nb\nc\n|d");
    expect(vi("|a\nb\nc\nd", "G''")).toBe("|a\nb\nc\nd");
  });
  it("keeps a mark on its line when lines are added above", () => {
    expect(vi("a\n|b", "mbggOnew<Esc>'b")).toBe("new\na\n|b");
  });
  it("hands a file mark in another file to the view", () => {
    const shared = new ViShared(null);
    shared.fileMarks.set("A", { file: "workspace:other.md", at: { line: 3, col: 0 } });
    const { m, effects } = setup("|a", shared);
    m.feedKeys("'A");
    expect(effects).toContainEqual({ type: "fileMark", file: "workspace:other.md", at: { line: 3, col: 0 } });
  });
});

describe("modes and status", () => {
  it("reports the mode, the pending keys and the cursor shape", () => {
    const { m } = setup("|abc");
    m.feedKeys('"a2d');
    expect(m.status()).toMatchObject({ mode: "normal", pending: '"a2d' });
    m.feedKeys("<Esc>i");
    expect(m.status().mode).toBe("insert");
    expect(m.cursorShape()).toBe("bar");
    m.feedKeys("<Esc>v");
    expect(m.status().mode).toBe("visual");
    expect(m.visualRegion()).toEqual({ kind: "char", start: { line: 0, col: 0 }, end: { line: 0, col: 1 } });
    m.feedKeys("<Esc>qa");
    expect(m.status().recording).toBe("a");
  });

  it("refuses changes on a read-only surface but moves, selects and yanks", () => {
    const doc = docFrom("|foo bar");
    const effects: ViEffect[] = [];
    const shared = new ViShared(null);
    const m = new ViMachine(doc, shared, {
      ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
      effect: (e) => effects.push(e),
      readOnly: true,
    });
    m.feedKeys("dwixwyiw");
    expect(show(doc)).toBe("foo |bar");
    expect(shared.registers.get('"')?.text).toBe("bar");
    expect(effects.filter((e) => e.type === "bell").length).toBeGreaterThan(0);
  });

  it("hands ]c, [c and scrolling to the view; : opens the machine's own command line", () => {
    const { m, effects } = setup("|a\nb");
    m.feedKeys("]c[czz:");
    expect(effects).toEqual([
      { type: "hunk", dir: 1 },
      { type: "hunk", dir: -1 },
      { type: "scroll", line: 0, to: "center" },
    ]);
    expect(m.status().cmdline).toEqual({ kind: ":", text: "", cursor: 0, register: false });
  });

  it("hands ]d and [d to the view, which knows the language server's problems", () => {
    const { m, effects } = setup("|a\nb");
    m.feedKeys("]d[dgdK");
    expect(effects).toEqual([
      { type: "problem", dir: 1 },
      { type: "problem", dir: -1 },
      { type: "definition" },
      { type: "hover" },
    ]);
  });

  it("hands gri, grt and grr to the view, one key at a time as typed (ED6.3)", () => {
    const { m, effects } = setup("|a\nb");
    for (const key of "grigrtgrr") m.feedKeys(key);
    expect(effects).toEqual([
      { type: "locations", kind: "implementation" },
      { type: "locations", kind: "typeDefinition" },
      { type: "locations", kind: "references" },
    ]);
  });

  it("hands gra to the view: at the cursor in Normal, the selection as the range in Visual (ED6.7)", () => {
    const { m, effects } = setup("a|bc def\nghi");
    m.feedKeys("gra");
    m.feedKeys("vlgra");
    m.feedKeys("jVgra");
    expect(effects).toEqual([
      { type: "codeAction", range: null },
      { type: "codeAction", range: { start: pos(0, 1), end: pos(0, 3) } },
      { type: "codeAction", range: { start: pos(1, 0), end: pos(1, 3) } },
    ]);
    expect(m.status().mode).toBe("normal");
  });

  it("takes a completion inside Insert as one change, and . repeats its text without the import (ED6.4)", () => {
    const { doc, m } = setup("|x\ny");
    m.feedKeys("jAfo");
    const import_ = { range: range(pos(0, 0), pos(0, 0)), text: "use foo;\n" };
    m.feed({ command: { type: "complete", edit: { before: 2, after: 0, text: "foobar", cursor: 6, extra: [import_] } } });
    m.feedKeys("<Esc>");
    expect(show(doc)).toBe("use foo;\nx\nyfooba|r");
    m.feedKeys("k.");
    expect(show(doc)).toBe("use foo;\nxfooba|r\nyfoobar");
    m.feedKeys("uu");
    expect(doc.store.text()).toBe("x\ny");
  });

  it("still takes a two-key g command and rings for an unknown gr", () => {
    expect(vi("|ab", "gvx")).toBe("|b");
    const { m, effects } = setup("|ab");
    m.feedKeys("grx");
    expect(effects.some((e) => e.type === "locations")).toBe(false);
  });
});

describe("the Mac clipboard", () => {
  it("is the unnamed register while shared, and remembers what kind of text it wrote", () => {
    let clip = "";
    const shared = new ViShared({ read: () => clip, write: (t) => (clip = t) });
    expect(vi("|a\nb", "yyjp", shared)).toBe("a\nb\n|a");
    expect(clip).toBe("a\n");
    clip = "pasted";
    expect(vi("|x", "p", shared)).toBe("xpaste|d");
  });

  it("waits for a clipboard that answers late, and keeps the keys typed meanwhile in order", async () => {
    let resolve: (text: string) => void = () => {};
    const shared = new ViShared({ read: () => new Promise<string>((r) => (resolve = r)), write: () => {} });
    const { doc, m } = setup("|ab", shared);
    m.feedKeys("px");
    expect(show(doc)).toBe("|ab");
    resolve("Z");
    await new Promise((r) => setTimeout(r, 0));
    // `p` ran first (after the "a"), then `x` took the pasted "Z" again.
    expect(show(doc)).toBe("a|b");
  });
});

table("more insert keys and commands", [
  ["|abc", "ix<C-o>$y<Esc>", "xab|yc"],
  ["|foo bar", 'yiwA <C-r>"<Esc>', "foo bar fo|o"],
  ["    ab|c", "a d<C-u><Esc>", "   | "],
  ["|abc\nxyz", "ifoo<Esc>jgibar<Esc>", "fooba|rabc\nxyz"],
  ["  |x = 1", "ccy<Esc>", "  |y"],
  ["|ab", "ix<Esc>3.", "xx|xxab"],
  ["|aaa", "3~", "AA|A"],
  ["|foo", "gUU", "|FOO"],
  ["|abc\nabc", "<C-v>jly$p", "abc|ab\nabcab"],
  ["|abc\nabcd", "<C-v>j$d", "|\n"],
  ['a|b "cd" e', 'ci"x<Esc>', 'ab "|x" e'],
]);

describe("screen-relative motions and scrolling", () => {
  const lines = Array.from({ length: 30 }, (_, i) => `l${i}`).join("\n");
  it("moves to the top, middle and bottom of the screen", () => {
    expect(vi(`|${lines}`, "L").split("\n")[9]).toBe("|l9");
    expect(vi(`|${lines}`, "M").split("\n")[4]).toBe("|l4");
    expect(vi(`|${lines}`, "3H").split("\n")[2]).toBe("|l2");
  });
  it("moves half a screen with Ctrl-d and asks the view to scroll with it", () => {
    const { doc, m, effects } = setup(`|${lines}`);
    m.feedKeys("<C-d>");
    expect(doc.selection.head.line).toBe(5);
    expect(effects).toContainEqual({ type: "scrollLines", delta: 5 });
  });
});

describe("a late clipboard while a macro records", () => {
  it("records the paste once and replays it", async () => {
    let resolve: (text: string) => void = () => {};
    let pending = true;
    const shared = new ViShared({
      read: () => (pending ? new Promise<string>((r) => (resolve = r)) : "Z"),
      write: () => {},
    });
    const { doc, m } = setup("|a\nb", shared);
    m.feedKeys("qqpjq");
    resolve("Z");
    await new Promise((r) => setTimeout(r, 0));
    pending = false;
    expect(shared.registers.get("q")?.text).toBe("pj");
    expect(show(doc)).toBe("aZ\n|b");
    m.feedKeys("@q");
    expect(show(doc)).toBe("aZ\nb|Z");
  });
});

describe("dispose (ED3.2)", () => {
  it("closes an undo group an Insert session left open, so a later machine's edit is its own step", () => {
    const doc = docFrom("|foo");
    const shared = new ViShared(null);
    const env = { ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }), effect: () => {} };
    const m1 = new ViMachine(doc, shared, env);
    // "iX" without an Escape: the Insert session's undo group is never closed by `finish()`.
    m1.feedKeys("iX");
    expect(show(doc)).toBe("X|foo");
    expect(doc.inUndoGroup).toBe(true);
    m1.dispose();
    expect(doc.inUndoGroup).toBe(false);

    // A fresh machine on the same document: its own edit must not join the abandoned group.
    const m2 = new ViMachine(doc, shared, env);
    m2.feedKeys("x");
    expect(show(doc)).toBe("X|oo");

    // Undoing the second machine's edit alone leaves the first machine's typing in place.
    expect(doc.undo()).toBe(true);
    expect(show(doc)).toBe("X|foo");
    expect(doc.undo()).toBe(true);
    expect(show(doc)).toBe("|foo");
  });

  it("is a no-op when no undo group is open", () => {
    const doc = docFrom("|foo");
    const shared = new ViShared(null);
    const env = { ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }), effect: () => {} };
    const m = new ViMachine(doc, shared, env);
    expect(doc.inUndoGroup).toBe(false);
    expect(() => m.dispose()).not.toThrow();
    expect(doc.inUndoGroup).toBe(false);
  });
});
