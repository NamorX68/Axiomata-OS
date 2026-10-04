/**
 * Search and ex through the machine (`docs/plans/editor.md`, ED3.3, V6, V7):
 * the command line's keys go through the machine's queue like any others, so
 * the same table as `vi.test.ts` applies — text before, keys, text after.
 */

import { describe, expect, it } from "vitest";

import { ctx, docFrom, show } from "../testing";
import { SearchMemory } from "./cmdmode";
import { ViMachine, ViShared, type ViEffect } from "./machine";

function setup(marked: string, options: { shared?: ViShared; readOnly?: boolean } = {}) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const shared = options.shared ?? new ViShared(null);
  const m = new ViMachine(doc, shared, {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    readOnly: options.readOnly,
    fileName: "notes.md",
  });
  return { doc, m, effects, shared };
}

function vi(before: string, keys: string): string {
  const { doc, m } = setup(before);
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

table("search as a motion (V7)", [
  ["|foo bar foo", "/foo<CR>", "foo bar |foo"],
  ["|foo bar foo", "/foo<CR>n", "|foo bar foo"],
  ["foo bar |foo", "?bar<CR>", "foo |bar foo"],
  ["foo |bar foo bar", "?foo<CR>n", "foo bar |foo bar"],
  ["|a b a b a", "/a<CR>N", "|a b a b a"],
  ["|a b a b a", "3/a<CR>", "|a b a b a"],
  ["|a b a b a", "2/a<CR>", "a b a b |a"],
  ["|x\nFoo\nfoo", "/foo<CR>", "x\n|Foo\nfoo"],
  ["|x\nFoo\nfoo", "/Foo<CR>n", "x\n|Foo\nfoo"],
  ["|x cat concat cat", "/\\<cat\\><CR>n", "x cat concat |cat"],
  // An empty pattern searches for the last one again.
  ["|a a a", "/a<CR>/<CR>", "a a |a"],
  // A count on n/N repeats the search that many times, wrapping if it must.
  ["|a b a b a b a", "/a<CR>3n", "|a b a b a b a"],
  ["|x a y a z", "?a<CR>N", "x |a y a z"],
]);

table("* and #", [
  ["|foo food foo", "*", "foo food |foo"],
  ["foo food |foo", "#", "|foo food foo"],
  ["|foo bar foo", "*n", "|foo bar foo"],
  // Not on a word: the next word on the line.
  ["|  foo x foo", "*", "  foo x |foo"],
  ["|Foo foo Foo", "*", "Foo foo |Foo"],
  // A count on # repeats the backward search that many times.
  ["|a b a c a", "2#", "a b |a c a"],
]);

table("operators with a search (d/ c? y/)", [
  ["|one two three", "d/thr<CR>", "|three"],
  ["one two |three", "d?two<CR>", "one |three"],
  ["|a x b x c", "d/x<CR>.", "|x c"],
  ["|one two", "c/two<CR>1 <Esc>", "1| two"],
  ["|a b c", "dn", "|a b c"],
  ["|a b a", "/a<CR>ggdn", "|a"],
  // `.` after a change with a search motion replays the whole thing: the search, then the typed text.
  ["|a x b x c x d", "c/x<CR>ZZ<Esc>.", "ZZ|Zx b x c x d"],
]);

table("Visual mode extends to a search", [["|a b c", "v/c<CR>d", "|"]]);

table(":s (V6)", [
  ["|a a\na a", ":s/a/b/<CR>", "|b a\na a"],
  ["|a a\na a", ":s/a/b/g<CR>", "|b b\na a"],
  ["|a a\na a", ":%s/a/b/g<CR>", "b b\n|b b"],
  ["|x\n  a\na", ":%s/a/b/<CR>", "x\n  b\n|b"],
  ["|a\na\na\na", ":2,3s/a/b/<CR>", "a\nb\n|b\na"],
  ["|a\na\na", "Vj:s/a/b/<CR>", "b\n|b\na"],
  ["|foo-bar", ":s/\\(foo\\)-\\(bar\\)/x/<CR>", "|foo-bar"],
  ["|foo-bar", ":s/(foo)-(bar)/\\2-\\1/<CR>", "|bar-foo"],
  ["|a,b", ":s/,/\\r/<CR>", "a\n|b"],
  ["|a b", ":s/a/[&]/<CR>", "|[a] b"],
  // `u` undoes a whole :s at once.
  ["|a\na", ":%s/a/b/<CR>u", "|a\na"],
  // `&` repeats the last :s on the cursor's line, without its flags.
  ["|a a\na a", ":s/a/b/g<CR>j&", "b b\n|b a"],
  ["|a a\na a", ":s/a/b/g<CR>j:&&<CR>", "b b\n|b b"],
  // An empty pattern is the last search.
  ["|x foo", "/foo<CR>:s//bar/<CR>", "|x bar"],
  // `~` is the previous replacement.
  ["|a\nb", ":s/a/X/<CR>j:s/b/~~/<CR>", "X\n|XX"],
  // A replacement that inserts line breaks on every line of the range: the cursor lands on the
  // very last line the substitution created, not on the original last line of the range.
  ["|a,b\nc,d", ":%s/,/\\r/<CR>", "a\nb\nc\n|d"],
]);

table(":'<,'> after a Visual-block selection (V6)", [
  // The block's columns do not matter to an ex range: `'<`/`'>` are its first/last *line*.
  ["|a a\na a\nx x", "<C-v>j:s/a/b/<CR>", "b a\n|b a\nx x"],
]);

table(":{n}, @: and history", [
  ["|a\nb\nc", ":3<CR>", "a\nb\n|c"],
  ["|a\nb\nc", ":3<CR>``", "|a\nb\nc"],
  ["|a\na\na", ":s/a/b/<CR>j@:", "b\n|b\na"],
  ["|a\na\na", ":s/a/b/<CR>j@:j@@", "b\nb\n|b"],
  ["|a\na", ":s/a/b/<CR>j:<Up><CR>", "b\n|b"],
  ["|ab", ":s/<C-r>=/x/<CR>", "|ab"],
]);

table("macros and . with the command line", [
  ["|a\na\na", "qq:s/a/b/<CR>jq@q", "b\nb\n|a"],
  ["|a x\na x", "qqd/x<CR>jq0@q", "x\n|x"],
]);

describe("the command line in the pill", () => {
  it("shows what is being typed, and the machine's mode underneath", () => {
    const { m } = setup("|a");
    m.feedKeys(":se");
    expect(m.status().cmdline).toEqual({ kind: ":", text: "se", cursor: 2, register: false });
    expect(m.inCommandLine).toBe(true);
    m.feedKeys("<Esc>");
    expect(m.status().cmdline).toBeNull();
    expect(m.inCommandLine).toBe(false);
  });

  it("completes with <Tab>", () => {
    const { m } = setup("|a");
    m.feedKeys(":noh<Tab>");
    expect(m.status().cmdline?.text).toBe("nohlsearch");
  });

  it("inserts a register with <C-r>", () => {
    const { m } = setup("|foo");
    m.feedKeys("yiw:<C-r>0");
    expect(m.status().cmdline?.text).toBe("foo");
    m.feedKeys("<C-r>");
    expect(m.status().cmdline?.register).toBe(true);
  });

  it("keeps `\":` and `\"/` current", () => {
    const { m, shared } = setup("|a");
    m.feedKeys("/a<CR>:noh<CR>");
    expect(shared.registers.get("/")?.text).toBe("a");
    expect(shared.registers.get(":")?.text).toBe("noh");
  });
});

describe("messages", () => {
  it("says a pattern was not found, rings, and forgets it at the next key", () => {
    const { m, effects } = setup("|a");
    m.feedKeys("/zzz<CR>");
    expect(m.status().message).toEqual({ text: "E486: Pattern not found: zzz", error: true });
    expect(effects.some((e) => e.type === "bell")).toBe(true);
    m.feedKeys("l");
    expect(m.status().message).toBeNull();
  });

  it("says when a search went round the end", () => {
    const { m } = setup("a |b a");
    m.feedKeys("/a<CR>/a<CR>");
    expect(m.status().message).toEqual({ text: "search hit BOTTOM, continuing at TOP", error: false });
  });

  it("counts a :s over more than two lines", () => {
    const { m } = setup("|a\na\na");
    m.feedKeys(":%s/a/b/<CR>");
    expect(m.status().message?.text).toBe("3 substitutions on 3 lines");
  });

  it("explains an unknown command and a missing previous pattern", () => {
    const { m } = setup("|a");
    m.feedKeys(":bogus<CR>");
    expect(m.status().message?.text).toBe("E492: Not an editor command: bogus");
    m.feedKeys("n");
    expect(m.status().message?.text).toBe("E35: No previous regular expression");
  });
});

describe("effects for the view", () => {
  it("saves, quits and reloads", () => {
    const { m, effects } = setup("|a");
    m.feedKeys(":w<CR>:q<CR>:q!<CR>:e!<CR>:x<CR>");
    expect(effects).toEqual([
      { type: "save" },
      { type: "quit", force: false },
      { type: "quit", force: true },
      { type: "reload" },
      { type: "quit", force: false },
    ]);
  });

  it("refuses :q with unsaved changes but lets :wq and :x save first", () => {
    const { m, effects } = setup("|a");
    m.feedKeys("x:q<CR>");
    expect(m.status().message?.text).toMatch(/^E37/);
    m.feedKeys(":x<CR>:wq<CR>");
    expect(effects.filter((e) => e.type !== "bell")).toEqual([{ type: "saveQuit" }, { type: "saveQuit" }]);
  });

  it("lets :q! through with unsaved changes, without a write", () => {
    const { m, effects } = setup("|a");
    m.feedKeys("x:q!<CR>");
    expect(m.status().message).toBeNull();
    expect(effects.filter((e) => e.type !== "bell")).toEqual([{ type: "quit", force: true }]);
  });

  it("passes :set on", () => {
    const { m, effects } = setup("|a");
    m.feedKeys(":set nowrap<CR>:set rnu!<CR>");
    expect(effects).toEqual([
      { type: "set", option: "wrap", value: false },
      { type: "set", option: "relativenumber", value: "toggle" },
    ]);
  });
});

describe("hlsearch and incsearch", () => {
  it("shows the typed pattern's matches and the one it would go to, without moving", () => {
    const { m } = setup("|a b a");
    m.feedKeys("/a");
    const { matches, current } = m.searchHighlights(0, 0);
    expect(matches.get(0)).toEqual([
      [0, 1],
      [4, 5],
    ]);
    expect(current).toEqual({ start: { line: 0, col: 4 }, end: { line: 0, col: 5 } });
    expect(m.revealTarget()).toEqual({ line: 0, col: 4 });
    expect(m.cursor).toEqual({ line: 0, col: 0 });
    m.feedKeys("<Esc>");
    expect(m.revealTarget()).toEqual({ line: 0, col: 0 });
  });

  it("keeps the last search highlighted until :noh, and brings it back with n", () => {
    const { m } = setup("|a b a");
    m.feedKeys("/b<CR>");
    expect(m.searchHighlights(0, 0).matches.size).toBe(1);
    m.feedKeys(":noh<CR>");
    expect(m.searchHighlights(0, 0).matches.size).toBe(0);
    m.feedKeys("n");
    expect(m.searchHighlights(0, 0).matches.size).toBe(1);
  });

  it("stays off after :noh until the next search, including *", () => {
    const { m } = setup("|foo bar foo");
    m.feedKeys("/foo<CR>");
    expect(m.searchHighlights(0, 0).matches.size).toBe(1);
    m.feedKeys(":noh<CR>");
    expect(m.searchHighlights(0, 0).matches.size).toBe(0);
    // Moving the cursor around must not turn it back on by itself.
    m.feedKeys("l");
    expect(m.searchHighlights(0, 0).matches.size).toBe(0);
    m.feedKeys("*");
    expect(m.searchHighlights(0, 0).matches.size).toBe(1);
  });
});

describe("SearchMemory.remember (history)", () => {
  it("caps a history at 100 entries, dropping the oldest first", () => {
    const memory = new SearchMemory();
    for (let i = 0; i < 105; i++) memory.remember("search", `p${i}`);
    expect(memory.history.search).toHaveLength(100);
    expect(memory.history.search[0]).toBe("p5");
    expect(memory.history.search[memory.history.search.length - 1]).toBe("p104");
  });

  it("moves a repeated entry to the end instead of duplicating it", () => {
    const memory = new SearchMemory();
    memory.remember("cmd", "w");
    memory.remember("cmd", "s/a/b/");
    memory.remember("cmd", "w");
    expect(memory.history.cmd).toEqual(["s/a/b/", "w"]);
  });

  it("ignores an empty entry", () => {
    const memory = new SearchMemory();
    memory.remember("search", "");
    expect(memory.history.search).toEqual([]);
  });
});

describe("<C-r> on the command line with a clipboard that answers late", () => {
  it("replays the register key and inserts the text once the clipboard resolves", async () => {
    let resolve: (text: string) => void = () => {};
    const shared = new ViShared({ read: () => new Promise<string>((r) => (resolve = r)), write: () => {} });
    const { m } = setup("|a", { shared });
    m.feedKeys(':<C-r>+x<CR>');
    // The clipboard has not answered yet: "x" is queued behind the pending register read.
    expect(m.status().cmdline?.text).toBe("");
    resolve("Z");
    await new Promise((r) => setTimeout(r, 0));
    expect(m.status().cmdline).toBeNull();
    expect(m.status().message?.text).toMatch(/^E492/); // "Zx" is not a known ex command
  });
});

describe("Ctrl-o in Insert mode", () => {
  it("returns to Insert after a command line, not while it is open", () => {
    const { m, doc } = setup("|a\nb");
    m.feedKeys("i<C-o>:");
    expect(m.mode).toBe("normal");
    m.feedKeys("2<CR>");
    expect(m.mode).toBe("insert");
    m.feedKeys("x<Esc>");
    expect(show(doc)).toBe("a\n|xb");
  });

  it("returns to Insert after a search", () => {
    const { m } = setup("|a b");
    m.feedKeys("i<C-o>/b<CR>");
    expect(m.mode).toBe("insert");
    expect(m.cursor).toEqual({ line: 0, col: 2 });
  });
});

describe("a read-only surface", () => {
  it("searches but refuses :s and d/", () => {
    const { m, doc } = setup("|a b a", { readOnly: true });
    m.feedKeys("/b<CR>");
    expect(m.cursor).toEqual({ line: 0, col: 2 });
    m.feedKeys(":s/a/x/<CR>");
    expect(m.status().message?.text).toMatch(/^E21/);
    m.feedKeys("d/a");
    expect(m.inCommandLine).toBe(false);
    expect(doc.store.text()).toBe("a b a");
  });

  it("refuses :x (a write) the same way as :w, with no quit effect", () => {
    const { m, effects } = setup("|a", { readOnly: true });
    m.feedKeys(":x<CR>");
    expect(m.status().message?.text).toMatch(/^E45/);
    expect(effects.filter((e) => e.type !== "bell")).toEqual([]);
  });
});

table(":g, :v, :d and :normal (ED5, T12)", [
  // The cursor stays where the last line was deleted.
  ["|a1\nb\na2\nc", ":g/a/d<CR>", "b\n|c"],
  ["|a1\nb\na2\nc", ":v/a/d<CR>", "a1\n|a2"],
  ["|a1\nb\na2\nc", ":g!/a/d<CR>", "a1\n|a2"],
  // Marked first, then visited: deleting one line does not skip the next.
  ["|x\nx\nx\ny", ":g/x/d<CR>", "|y"],
  ["|a1\nb\na2", ":g/a/s/\\d/N/<CR>", "aN\nb\n|aN"],
  ["|a\nb\na", ":g/a/normal Ax<CR>", "ax\nb\na|x"],
  // `:normal` joining lines shrinks the line count as `:g` visits them: the anchors must follow.
  ["|a1\nx\na2\ny\na3\nz", ":g/a/normal J<CR>", "a1 x\na2 y\na3| z"],
  // A replacement that inserts a real line break (`\r`) grows the line count as `:g` runs.
  ["|a,b\na,c", ":g/a/s/,/\\r/<CR>", "a\nb\na\n|c"],
  ["|one\ntwo\nthree", ":2,3d<CR>", "|one"],
  ["|one\ntwo\nthree", ":d<CR>", "|two\nthree"],
  ["|one\ntwo", ":%normal I- <CR>", "- one\n-| two"],
  // A pattern on no line says so and changes nothing.
  ["|a\nb", ":g/z/d<CR>", "|a\nb"],
  // `:v` on the one empty line of an empty file: it is marked (it does not contain the pattern),
  // but deleting the file's only line is a no-op — there is always at least one line left.
  ["|", ":v/x/d<CR>", "|"],
]);

table(":s across lines and with c (ED5, T5, T12)", [
  ["|a,\nb,\nc", ":%s/,\\n/ /<CR>", "|a b c"],
  ["|x x\nx", ":%s/x/y/gc<CR>yny", "y x\n|y"],
  ["|x x\nx", ":%s/x/y/gc<CR>a", "y y\n|y"],
  ["|x x\nx", ":%s/x/y/gc<CR>yq", "|y x\nx"],
  ["|x x\nx", ":%s/x/y/gc<CR>nl", "|x y\nx"],
  // Without g, one match a line.
  ["|x x\nx", ":%s/x/y/c<CR>yy", "y x\n|y"],
]);

describe(":g and :s///c as one undo step", () => {
  it("undoes a whole :g at once", () => {
    const { doc, m } = setup("|a\nb\na\nb");
    m.feedKeys(":g/a/d<CR>u");
    expect(show(doc)).toBe("|a\nb\na\nb");
  });

  it("undoes a whole :s///c at once", () => {
    const { doc, m } = setup("|x x");
    m.feedKeys(":s/x/y/gc<CR>yyu");
    expect(show(doc)).toBe("|x x");
  });

  it("asks with the replacement and shows the match it asks about", () => {
    const { m } = setup("|ab ab");
    m.feedKeys(":s/b/Z/gc<CR>");
    expect(m.status().message?.text).toBe("replace with Z (y/n/a/q/l)?");
    expect(m.searchHighlights(0, 0).current).toEqual({ start: { line: 0, col: 1 }, end: { line: 0, col: 2 } });
  });

  it("refuses a :g inside a :g", () => {
    const { doc, m } = setup("|a");
    m.feedKeys(":g/a/g/a/d<CR>");
    expect(show(doc)).toBe("|a");
    expect(m.status().message?.text).toMatch(/E147/);
  });
});

describe(":s///c edge cases (ED5, T12)", () => {
  it("'a' after some 'n's replaces only the matches from there on", () => {
    const { doc, m } = setup("|x x x x");
    m.feedKeys(":s/x/y/gc<CR>nnay");
    // The first two are skipped (n n), the rest replaced at once (a); the cursor lands at
    // the substitution's line start, and the trailing 'y' is just a no-op yank waiting for a motion.
    expect(show(doc)).toBe("|x x y y");
  });

  it("asks again at each line for an empty-match pattern, instead of looping on one spot", () => {
    const { doc, m } = setup("|ab\ncd");
    m.feedKeys(":%s/^/> /gc<CR>");
    expect(m.status().message?.text).toBe("replace with >  (y/n/a/q/l)?");
    m.feedKeys("y");
    // Still asking — now about the second line's empty match, not stuck on the first.
    expect(m.status().message?.text).toBe("replace with >  (y/n/a/q/l)?");
    m.feedKeys("y");
    expect(show(doc)).toBe("> ab\n|> cd");
    expect(m.status().message?.text).toBe("2 substitutions");
  });

  it("re-finds a spanning pattern's next match on the text as edited so far", () => {
    const { doc, m } = setup("|a,\nb,\nc");
    m.feedKeys(":%s/,\\n/ /gc<CR>y");
    // One join done; the next match is asked about on the now-joined text, not the original.
    expect(m.status().message?.text).toBe("replace with   (y/n/a/q/l)?");
    m.feedKeys("y");
    expect(show(doc)).toBe("|a b c");
  });

  it("undoes only what was confirmed so far when the round is cut short with Esc", () => {
    const { doc, m } = setup("|x x x");
    m.feedKeys(":s/x/y/gc<CR>y");
    // Still mid-round: the second and third x are still to be asked about.
    expect(m.status().message?.text).toBe("replace with y (y/n/a/q/l)?");
    m.feedKeys("<Esc>");
    expect(show(doc)).toBe("|y x x");
    m.feedKeys("u");
    // The one replacement made before Esc is undone as a single step.
    expect(show(doc)).toBe("|x x x");
  });
});
