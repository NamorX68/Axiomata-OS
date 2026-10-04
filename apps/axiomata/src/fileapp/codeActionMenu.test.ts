import { describe, expect, it } from "vitest";

import type { CodeActionItem } from "../editor/lsp/codeActions";
import { pos } from "../editor/position";
import { CodeActionMenu, initialChoice } from "./codeActionMenu";

const action = (title: string, extra: Partial<CodeActionItem> = {}): CodeActionItem => ({
  title,
  kind: "quickfix",
  preferred: false,
  disabled: null,
  fixes: false,
  edit: null,
  command: null,
  raw: null,
  ...extra,
});

function menu() {
  let changes = 0;
  const m = new CodeActionMenu(() => changes++);
  return { m, changes: () => changes };
}

describe("CodeActionMenu (ED6.7, L20)", () => {
  it("stays closed with nothing to offer", () => {
    const { m } = menu();
    expect(m.show(pos(0, 0), [])).toBe(false);
    expect(m.isOpen).toBe(false);
  });

  it("opens on the preferred action, else on the first one that can be taken", () => {
    expect(initialChoice([action("a"), action("b", { preferred: true })])).toBe(1);
    expect(initialChoice([action("a", { disabled: "no" }), action("b")])).toBe(1);
    expect(initialChoice([action("a", { disabled: "no", preferred: true })])).toBe(0);
    const { m } = menu();
    m.show(pos(2, 4), [action("a"), action("b", { preferred: true })]);
    expect(m.view).toMatchObject({ anchor: pos(2, 4), selected: 0, query: "" });
    // The preferred one is ordered first, and chosen.
    expect(m.view?.items[0].title).toBe("b");
  });

  it("moves round, narrows as letters are typed and widens with ⌫", () => {
    const { m, changes } = menu();
    m.show(pos(0, 0), [action("Add import"), action("Remove unused"), action("Inline value")]);
    m.move(-1);
    expect(m.view?.selected).toBe(2);
    m.move(1);
    expect(m.view?.selected).toBe(0);
    m.narrow("r");
    m.narrow("e");
    m.narrow("m");
    expect(m.view?.items.map((i) => i.title)).toEqual(["Remove unused"]);
    expect(m.view?.query).toBe("rem");
    m.narrow(null);
    m.narrow(null);
    m.narrow(null);
    expect(m.view?.items).toHaveLength(3);
    m.narrow(null);
    expect(changes()).toBeGreaterThan(0);
  });

  it("takes the chosen entry or a numbered one, never one that is not available", () => {
    const { m } = menu();
    const items = [action("one"), action("two"), action("off", { disabled: "why" })];
    m.show(pos(0, 0), items);
    expect(m.take(3)).toBeNull();
    expect(m.take(9)).toBeNull();
    expect(m.isOpen).toBe(true);
    expect(m.take(2)?.title).toBe("two");
    expect(m.isOpen).toBe(false);
    m.show(pos(0, 0), items);
    m.move(1);
    expect(m.take()?.title).toBe("two");
  });
});
