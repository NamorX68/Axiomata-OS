import { describe, expect, it } from "vitest";

import { filterActions, fixesOnly, kindTag, orderActions, parseCodeActions, type CodeActionItem } from "./codeActions";

const action = (title: string, extra: Partial<CodeActionItem> = {}): CodeActionItem => ({
  title,
  kind: "",
  preferred: false,
  disabled: null,
  fixes: false,
  edit: null,
  command: null,
  raw: { title },
  ...extra,
});

describe("parseCodeActions (ED6.7)", () => {
  it("reads code actions and bare commands, skipping malformed entries", () => {
    const edit = { changes: {} };
    const items = parseCodeActions([
      { title: "Add import", kind: "quickfix", isPreferred: true, diagnostics: [{}], edit },
      { title: "Organize", command: "organize", arguments: [1, "x"] },
      { title: "Extract", kind: "refactor.extract", command: { title: "Extract", command: "extract" } },
      { title: "Inline", kind: "refactor.inline", disabled: { reason: "no selection" } },
      { kind: "quickfix" },
      "nonsense",
    ]);
    expect(items.map((i) => i.title)).toEqual(["Add import", "Organize", "Extract", "Inline"]);
    const [fix, bare, extract, inline] = items;
    expect(fix).toMatchObject({ kind: "quickfix", preferred: true, fixes: true, edit, command: null });
    expect(bare).toMatchObject({ kind: "", raw: null, command: { command: "organize", arguments: [1, "x"] } });
    // No `arguments` from the server: none echoed back (Rust compares exactly, L18).
    expect(extract.command).toEqual({ title: "Extract", command: "extract" });
    expect(inline.disabled).toBe("no selection");
    expect(parseCodeActions(null)).toEqual([]);
  });
});

describe("orderActions and fixesOnly (L19, L21)", () => {
  it("puts fixes first, the preferred one first among them, and what cannot be taken last", () => {
    const items = [
      action("source", { kind: "source.organizeImports" }),
      action("off", { kind: "quickfix", disabled: "no" }),
      action("refactor", { kind: "refactor.rewrite" }),
      action("fix", { kind: "quickfix" }),
      action("best", { kind: "quickfix", preferred: true }),
      action("cmd"),
    ];
    expect(orderActions(items).map((i) => i.title)).toEqual(["best", "fix", "refactor", "source", "cmd", "off"]);
  });

  it("keeps the actions tied to the problem, else every fix", () => {
    const tied = action("tied", { kind: "quickfix", fixes: true });
    const loose = action("loose", { kind: "quickfix" });
    const refactor = action("refactor", { kind: "refactor" });
    expect(fixesOnly([tied, loose, refactor])).toEqual([tied]);
    expect(fixesOnly([loose, refactor])).toEqual([loose]);
  });
});

describe("filterActions and kindTag (L19, L20)", () => {
  it("narrows by title unscharf, best first, and keeps the order for no query", () => {
    const items = [action("Extract into function"), action("Add missing import"), action("Inline variable")];
    expect(filterActions(items, "").map((i) => i.title)).toEqual(items.map((i) => i.title));
    expect(filterActions(items, "imp").map((i) => i.title)).toEqual(["Add missing import"]);
    expect(filterActions(items, "if").map((i) => i.title)).toEqual(["Extract into function"]);
    expect(filterActions(items, "zzz")).toEqual([]);
  });

  it("names each kind briefly", () => {
    expect(kindTag("quickfix")).toBe("fix");
    expect(kindTag("refactor.extract.function")).toBe("extract");
    expect(kindTag("source.organizeImports")).toBe("imports");
    expect(kindTag("source.fixAll.ruff")).toBe("fix all");
    expect(kindTag("")).toBe("cmd");
    expect(kindTag("notebook.format")).toBe("notebook");
  });
});
