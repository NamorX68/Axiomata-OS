/**
 * Direct tests of the `Registers` class (`docs/plans/editor.md`, ED3, V4):
 * numbered-register shifting, uppercase append, the read-only registers, and
 * the Mac clipboard's kind round trip — as a complement to the machine-level
 * `"put and registers"` table in `vi.test.ts`.
 */

import { describe, expect, it } from "vitest";

import { isRegisterName, Registers, type ClipboardPort, type RegisterContent } from "./registers";

function content(text: string, kind: RegisterContent["kind"] = "char"): RegisterContent {
  return { text, kind };
}

describe("numbered registers", () => {
  it("shifts multi-line deletes down through \"1-\"9", () => {
    const registers = new Registers(null);
    registers.put(null, content("a\n", "line"), "delete");
    registers.put(null, content("b\n", "line"), "delete");
    registers.put(null, content("c\n", "line"), "delete");
    expect(registers.get("1")?.text).toBe("c\n");
    expect(registers.get("2")?.text).toBe("b\n");
    expect(registers.get("3")?.text).toBe("a\n");
  });

  it("stops shifting past \"9 — the oldest delete falls off", () => {
    const registers = new Registers(null);
    for (let i = 0; i < 10; i++) registers.put(null, content(`d${i}\n`, "line"), "delete");
    expect(registers.get("1")?.text).toBe("d9\n");
    expect(registers.get("9")?.text).toBe("d1\n");
  });

  it("keeps a single-line delete in \"-, not the numbered stack", () => {
    const registers = new Registers(null);
    registers.put(null, content("word"), "delete");
    expect(registers.get("-")?.text).toBe("word");
    expect(registers.get("1")).toBeNull();
  });

  it("sets \"0 only for a yank, never for a delete", () => {
    const registers = new Registers(null);
    registers.put(null, content("deleted"), "delete");
    expect(registers.get("0")).toBeNull();
    registers.put(null, content("yanked"), "yank");
    expect(registers.get("0")?.text).toBe("yanked");
    // A later delete does not clobber "0 — only a yank does.
    registers.put(null, content("word"), "delete");
    expect(registers.get("0")?.text).toBe("yanked");
  });

  it("always sets the unnamed register too, whichever numbered register it went to", () => {
    const registers = new Registers(null);
    registers.put(null, content("a\n", "line"), "delete");
    expect(registers.get('"')?.text).toBe("a\n");
    registers.put(null, content("yanked"), "yank");
    expect(registers.get('"')?.text).toBe("yanked");
  });
});

describe("named registers and uppercase append", () => {
  it("appends to the lowercase register and keeps it as the unnamed register", () => {
    const registers = new Registers(null);
    registers.put("a", content("foo"), "yank");
    registers.put("A", content("bar"), "yank");
    expect(registers.get("a")?.text).toBe("foobar");
    expect(registers.get('"')?.text).toBe("foobar");
  });

  it("appends as lines when either side is a line, adding the missing line breaks", () => {
    const registers = new Registers(null);
    registers.put("a", content("foo\n", "line"), "yank");
    registers.put("A", content("bar"), "yank");
    expect(registers.get("a")).toEqual({ text: "foo\nbar\n", kind: "line" });
  });

  it("appends as characters when neither side is a line", () => {
    const registers = new Registers(null);
    registers.put("a", content("foo"), "yank");
    registers.put("A", content("bar"), "yank");
    expect(registers.get("a")).toEqual({ text: "foobar", kind: "char" });
  });

  it("round-trips only the named registers through named()/restoreNamed()", () => {
    const registers = new Registers(null);
    registers.put("a", content("foo"), "yank");
    registers.put(null, content("unnamed"), "yank"); // lands in "0 and "", not a named register
    const saved = registers.named();
    expect(Object.keys(saved)).toEqual(["a"]);

    const restored = new Registers(null);
    restored.restoreNamed(saved);
    expect(restored.get("a")?.text).toBe("foo");
    expect(restored.get("0")).toBeNull();
  });

  it("sends nothing to the black hole register", () => {
    const registers = new Registers(null);
    registers.put("_", content("gone"), "delete");
    expect(registers.get('"')).toBeNull();
    expect(registers.get("1")).toBeNull();
  });
});

describe("read-only registers", () => {
  it("reads through their owner function, ignoring any stored content", () => {
    const registers = new Registers(null);
    registers.readOnly["%"] = () => "notes.md";
    expect(registers.get("%")).toEqual({ text: "notes.md", kind: "char" });
  });

  it("is null when a read-only register has no owner registered", () => {
    const registers = new Registers(null);
    expect(registers.get(".")).toBeNull();
    expect(registers.get(":")).toBeNull();
    expect(registers.get("/")).toBeNull();
  });

  it("refuses a write to a read-only register name (`\".yiw`) instead of redirecting it", () => {
    const registers = new Registers(null);
    registers.put(".", content("hi"), "yank");
    expect(registers.get(".")).toBeNull();
    expect(registers.get("0")).toBeNull();
    expect(registers.get('"')).toBeNull();
  });
});

describe("the Mac clipboard's kind round trip", () => {
  function port(initial = ""): ClipboardPort & { text: string } {
    return {
      text: initial,
      read() {
        return this.text;
      },
      write(t: string) {
        this.text = t;
      },
    };
  }

  it("treats clipboard text as lines when it ends with a line break, characters otherwise", () => {
    const clip = port();
    const registers = new Registers(clip);
    clip.text = "one\ntwo\n";
    expect(registers.get('"')).toEqual({ text: "one\ntwo\n", kind: "line" });
    clip.text = "one two";
    expect(registers.get('"')).toEqual({ text: "one two", kind: "char" });
  });

  it("remembers the kind of what it last wrote, for reading the same text straight back", () => {
    const clip = port();
    const registers = new Registers(clip);
    registers.put(null, content("a\nb\n", "line"), "yank");
    expect(clip.text).toBe("a\nb\n");
    // The Mac clipboard is text-only; reading the very text just written keeps its remembered kind.
    expect(registers.get('"')).toEqual({ text: "a\nb\n", kind: "line" });
  });

  it("\"+ and \"* always reach the clipboard, even when \" is not shared with it", () => {
    const clip = port("from the Mac");
    const registers = new Registers(clip, false);
    expect(registers.get("+")?.text).toBe("from the Mac");
    expect(registers.get("*")?.text).toBe("from the Mac");
    expect(registers.get('"')).toBeNull(); // unshared: "" is the plain store, empty so far
  });

  it("waits for a clipboard read that resolves late, via ClipboardPending", async () => {
    let resolve: (text: string) => void = () => {};
    const clip: ClipboardPort = {
      read: () => new Promise<string>((r) => (resolve = r)),
      write: () => {},
    };
    const registers = new Registers(clip);
    expect(() => registers.get('"')).toThrowError();
    try {
      registers.get('"');
    } catch (err) {
      // Confirm it is specifically a pending-clipboard wait, not some other error.
      expect(err).toHaveProperty("text");
      const text = (err as { text: Promise<string> }).text;
      resolve("late text");
      await expect(text).resolves.toBe("late text");
    }
    registers.provide("late text");
    expect(registers.get('"')?.text).toBe("late text");
    registers.settle();
  });
});

describe("isRegisterName", () => {
  it("accepts letters, digits, the quote, and the special register names", () => {
    for (const ch of ["a", "Z", "5", '"', "-", "_", "+", "*", ".", "%", ":", "/"]) {
      expect(isRegisterName(ch)).toBe(true);
    }
  });

  it("rejects anything longer than one character or outside the allowed set", () => {
    expect(isRegisterName("ab")).toBe(false);
    expect(isRegisterName("!")).toBe(false);
    expect(isRegisterName("")).toBe(false);
  });
});
