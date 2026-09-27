import { describe, expect, it } from "vitest";

import { compilePattern } from "../vi/search";
import {
  compileFind,
  DEFAULT_FIND_OPTIONS,
  escapePattern,
  expandReplacement,
  findPattern,
  fromViPattern,
  preserveCase,
  viPatternFor,
  type FindOptions,
} from "./find";

const opts = (patch: Partial<FindOptions> = {}): FindOptions => ({ ...DEFAULT_FIND_OPTIONS, ...patch });

function matchesOf(query: string, options: FindOptions, text: string): string[] {
  const c = compileFind(query, options);
  if (!c || "error" in c) throw new Error("did not compile");
  return [...text.matchAll(c.re)].map((m) => m[0]);
}

function exec(re: RegExp, text: string): RegExpExecArray {
  const m = new RegExp(re.source, re.flags.replace("g", "")).exec(text);
  if (!m) throw new Error("no match");
  return m;
}

describe("find patterns", () => {
  it("takes a literal query literally, special characters and all", () => {
    expect(matchesOf("a.b(", opts(), "a.b( axb(")).toEqual(["a.b("]);
    expect(matchesOf("c:\\x/y", opts(), "c:\\x/y")).toEqual(["c:\\x/y"]);
  });

  it("ignores case unless asked, with no smartcase", () => {
    expect(matchesOf("Foo", opts(), "foo FOO Foo")).toHaveLength(3);
    expect(matchesOf("foo", opts({ caseSensitive: true }), "foo FOO Foo")).toEqual(["foo"]);
  });

  it("matches whole words only, for literal text and regular expressions alike", () => {
    expect(matchesOf("in", opts({ wholeWord: true }), "in inside pin in")).toHaveLength(2);
    expect(matchesOf("a|in", opts({ wholeWord: true, regex: true }), "a ab in pin")).toEqual(["a", "in"]);
  });

  it("speaks Vi's language in a regular expression", () => {
    expect(matchesOf("\\<x", opts({ regex: true }), "x ax")).toEqual(["x"]);
    expect(compileFind("(", opts({ regex: true }))).toHaveProperty("error");
    expect(compileFind("", opts())).toBeNull();
  });

  it("hands Vi the same search with its case spelled out, and takes Vi's back as a regex", () => {
    const pattern = viPatternFor("a.b", opts());
    const c = compilePattern(pattern);
    if ("error" in c) throw new Error();
    expect("A.B axb".match(c.re)).toEqual(["A.B"]);
    expect(viPatternFor("x", opts({ caseSensitive: true }))).toBe("x\\C");
    expect(fromViPattern("foo\\d")).toEqual({ query: "foo\\d", options: { regex: true, wholeWord: false, caseSensitive: false } });
    expect(fromViPattern("Foo").options.caseSensitive).toBe(true);
    // No capital anywhere: smartcase leaves it case-insensitive.
    expect(fromViPattern("123").options.caseSensitive).toBe(false);
  });

  it("escapes what it must and nothing else", () => {
    expect(escapePattern("a+b")).toBe("a\\+b");
    expect(findPattern("a+b", opts({ regex: true }))).toBe("a+b");
  });
});

describe("expandReplacement", () => {
  const m = exec(/(\w+)@(?<host>\w+)/u, "joe@home");

  it("fills in groups, the match and escapes in a regular expression", () => {
    expect(expandReplacement("$2:$1 [$&] $0 $<host>", m, true)).toBe("home:joe [joe@home] joe@home home");
    expect(expandReplacement("$$1 a\\tb\\nc \\\\", m, true)).toBe("$1 a\tb\nc \\");
  });

  it("leaves what names no group as it is", () => {
    expect(expandReplacement("$9 $<nope> $", m, true)).toBe("$9 $<nope> $");
    // Two digits only while that many groups exist: `$10` is group 1 and a 0.
    expect(expandReplacement("$10", m, true)).toBe("joe0");
    // `$<name` with no closing `>` names nothing either: left as it is.
    expect(expandReplacement("$<abc", m, true)).toBe("$<abc");
  });

  it("takes the template literally for a literal query", () => {
    expect(expandReplacement("$1\\n", m, false)).toBe("$1\\n");
  });

  it("uses a two-digit group index once that many groups exist, else falls back to one digit", () => {
    const many = exec(/(A)(B)(C)(D)(E)(F)(G)(H)(I)(J)(K)/, "ABCDEFGHIJK");
    // 11 groups exist, so `$11` is the eleventh one, not group 1 followed by a literal "1".
    expect(expandReplacement("$11", many, true)).toBe("K");
    const few = exec(/(A)(B)(C)/, "ABC");
    // Only 3 groups: `$11` falls back to group 1 plus a literal "1".
    expect(expandReplacement("$11", few, true)).toBe("A1");
  });
});

describe("preserveCase", () => {
  it("follows capitals, small letters and a capitalised word", () => {
    expect(preserveCase("FOO", "bar")).toBe("BAR");
    expect(preserveCase("foo", "Bar")).toBe("bar");
    expect(preserveCase("Foo", "bAR")).toBe("Bar");
  });

  it("leaves mixed case and letterless matches alone", () => {
    expect(preserveCase("fOo", "bar")).toBe("bar");
    expect(preserveCase("123", "Bar")).toBe("Bar");
  });
});
