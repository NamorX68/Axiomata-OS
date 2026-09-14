import { describe, expect, it } from "vitest";

import { parseEnvLines } from "./terminalEnv";

describe("parseEnvLines", () => {
  it("parses KEY=value lines into ordered pairs", () => {
    expect(parseEnvLines("FOO=bar\nBAZ=qux")).toEqual([
      ["FOO", "bar"],
      ["BAZ", "qux"],
    ]);
  });

  it("skips blank lines", () => {
    expect(parseEnvLines("FOO=bar\n\n\nBAZ=qux\n")).toEqual([
      ["FOO", "bar"],
      ["BAZ", "qux"],
    ]);
  });

  it("skips lines with no '=' at all", () => {
    expect(parseEnvLines("FOO=bar\nnotanassignment\nBAZ=qux")).toEqual([
      ["FOO", "bar"],
      ["BAZ", "qux"],
    ]);
  });

  it("skips a line with an empty key ('=value')", () => {
    expect(parseEnvLines("=novalue\nFOO=bar")).toEqual([["FOO", "bar"]]);
  });

  it("only splits at the first '=', keeping a value that itself contains '=' intact", () => {
    expect(parseEnvLines("FOO=a=b=c")).toEqual([["FOO", "a=b=c"]]);
  });

  it("trims the whole line first (so leading/trailing line whitespace is gone), then only the key", () => {
    // The line's own leading/trailing whitespace is stripped before
    // splitting at "=" (so a stray indent or trailing space on the line
    // itself doesn't become part of the value) — but once split, only the
    // key side is trimmed again; inner whitespace right after "=" is kept
    // as part of the value verbatim.
    expect(parseEnvLines("  FOO  =  bar")).toEqual([["FOO", "  bar"]]);
  });

  it("returns an empty list for empty or whitespace-only input", () => {
    expect(parseEnvLines("")).toEqual([]);
    expect(parseEnvLines("   \n  \n")).toEqual([]);
  });

  it("keeps an explicit empty value ('KEY=') rather than dropping the pair", () => {
    // A line like `FOO=` is a legitimate way to set a variable to the empty
    // string (distinct from not setting it at all) — it must survive as
    // `["FOO", ""]`, not be treated the same as a line with no "=" at all.
    expect(parseEnvLines("FOO=\nBAR=baz")).toEqual([
      ["FOO", ""],
      ["BAR", "baz"],
    ]);
  });

  it("keeps duplicate keys in the order they appeared, letting the last one win downstream", () => {
    // `parseEnvLines` itself does no de-duplication — `PtySession::spawn`
    // (the Rust side) applies entries in order via `cmd.env(key, value)`,
    // so a repeated key's *last* occurrence is what actually takes effect;
    // this only pins down that the parser hands both pairs through intact,
    // in order, for that Rust-side behaviour to rely on.
    expect(parseEnvLines("FOO=first\nFOO=second")).toEqual([
      ["FOO", "first"],
      ["FOO", "second"],
    ]);
  });
});
