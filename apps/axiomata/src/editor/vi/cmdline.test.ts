import { describe, expect, it } from "vitest";

import { CommandLine } from "./cmdline";
import { parseKeys } from "./keys";

function type(line: CommandLine, notation: string) {
  let last = null;
  for (const key of parseKeys(notation)) last = line.key(key);
  return last;
}

describe("CommandLine", () => {
  it("types, moves and deletes like a one-line field", () => {
    const line = new CommandLine(":", "", []);
    type(line, "abc<Left><Left>X<End>Y<Home>Z");
    expect(line.text).toBe("ZaXbcY");
    expect(line.cursor).toBe(1);
    type(line, "<Del>");
    expect(line.text).toBe("ZXbcY");
  });

  it("deletes a word with <C-w> and everything before the cursor with <C-u>", () => {
    const line = new CommandLine(":", "s/foo bar", []);
    type(line, "<C-w>");
    expect(line.text).toBe("s/foo ");
    type(line, "<C-u>");
    expect(line.text).toBe("");
  });

  it("submits on <CR>, cancels on <Esc> and on <BS> in an empty line", () => {
    expect(type(new CommandLine("/", "x", []), "<CR>")).toEqual({ kind: "submit", text: "x" });
    expect(type(new CommandLine("/", "x", []), "<Esc>")).toEqual({ kind: "cancel" });
    expect(type(new CommandLine("/", "x", []), "<BS>")).toEqual({ kind: "edited" });
    expect(type(new CommandLine("/", "x", []), "<BS><BS>")).toEqual({ kind: "cancel" });
  });

  it("takes typed text and Mac commands (⌥⌫, ⌘←)", () => {
    const line = new CommandLine(":", "", []);
    line.key({ text: "grüße welt" });
    line.key({ command: { type: "deleteBackward", unit: "word" } });
    expect(line.text).toBe("grüße ");
    line.key({ command: { type: "move", motion: "lineStart", extend: false } });
    expect(line.cursor).toBe(0);
  });

  it("walks the history among entries that begin with what was typed", () => {
    const line = new CommandLine(":", "", ["s/a/b/", "w", "set wrap"]);
    type(line, "s<Up>");
    expect(line.text).toBe("set wrap");
    type(line, "<Up>");
    expect(line.text).toBe("s/a/b/");
    type(line, "<Up>");
    expect(line.text).toBe("s/a/b/");
    type(line, "<Down><Down>");
    expect(line.text).toBe("s");
  });

  it("does nothing on <Up>/<Down> with an empty history", () => {
    const line = new CommandLine(":", "abc", []);
    type(line, "<Up>");
    expect(line.text).toBe("abc");
    type(line, "<Down>");
    expect(line.text).toBe("abc");
  });

  it("cycles completions with <Tab> and back to what was typed", () => {
    const line = new CommandLine(":", "w", [], () => ["write", "wq"]);
    type(line, "<Tab>");
    expect(line.text).toBe("write");
    type(line, "<Tab>");
    expect(line.text).toBe("wq");
    type(line, "<Tab>");
    expect(line.text).toBe("w");
    type(line, "<S-Tab>");
    expect(line.text).toBe("wq");
  });

  it("asks for a register after <C-r>, and flattens line breaks when inserting", () => {
    const line = new CommandLine(":", "", []);
    expect(type(line, "<C-r>a")).toEqual({ kind: "register", name: "a" });
    line.insert("x\ny");
    expect(line.text).toBe("x y");
  });

  it("ignores special keys it has no use for", () => {
    const line = new CommandLine(":", "a", []);
    type(line, "<C-x><PageUp>");
    expect(line.text).toBe("a");
  });
});
