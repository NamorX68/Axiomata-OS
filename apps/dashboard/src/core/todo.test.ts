import { describe, expect, it } from "vitest";

import {
  addTodo,
  completeTodo,
  deleteDone,
  deleteOpen,
  parseTodoDoc,
  reopenTodo,
  serializeTodoDoc,
  todayIso,
  type TodoDoc,
} from "./todo";

const SAMPLE = `# ToDo

- [ ] Steuerunterlagen sortieren
- [ ] Rückruf Werkstatt

## Done

- [x] Reifen wechseln lassen (done: 2026-09-04)
- [x] Pass verlängern (done: 2026-09-02)
`;

describe("parseTodoDoc", () => {
  it("splits open and done at the first `## Done` heading", () => {
    const doc = parseTodoDoc(SAMPLE);
    expect(doc.open.map((i) => i.text)).toEqual(["Steuerunterlagen sortieren", "Rückruf Werkstatt"]);
    expect(doc.done.map((i) => i.text)).toEqual(["Reifen wechseln lassen", "Pass verlängern"]);
    expect(doc.done.map((i) => i.doneOn)).toEqual(["2026-09-04", "2026-09-02"]);
  });

  it("treats lines after `## Done` as done regardless of checkbox state", () => {
    const doc = parseTodoDoc("# ToDo\n\n- [x] ticked but still open\n\n## Done\n\n- [ ] unticked but archived\n");
    expect(doc.open.map((i) => i.text)).toEqual(["ticked but still open"]);
    expect(doc.done.map((i) => i.text)).toEqual(["unticked but archived"]);
  });

  it("keeps a done item whose date is missing or malformed (doneOn = null)", () => {
    const doc = parseTodoDoc("# ToDo\n\n## Done\n\n- [x] no date here\n- [x] bad date (done: yesterday)\n");
    expect(doc.done.map((i) => i.text)).toEqual(["no date here", "bad date (done: yesterday)"]);
    expect(doc.done.map((i) => i.doneOn)).toEqual([null, null]);
  });

  it("handles a file with no `## Done` section", () => {
    const doc = parseTodoDoc("# ToDo\n\n- [ ] only open\n");
    expect(doc.open.map((i) => i.text)).toEqual(["only open"]);
    expect(doc.done).toEqual([]);
  });

  it("recognises any heading level and trailing text on the Done heading", () => {
    expect(parseTodoDoc("# ToDo\n### done (3)\n- [x] x\n").done).toHaveLength(1);
    expect(parseTodoDoc("# ToDo\n# DONE\n- [x] x\n").done).toHaveLength(1);
  });

  it("tolerates lenient bullet spacing and `*` bullets, ignores non-task lines", () => {
    const doc = parseTodoDoc("# ToDo\n\nsome prose\n-[ ]tight\n*  [ ]  star bullet\n> quote\n");
    expect(doc.open.map((i) => i.text)).toEqual(["tight", "star bullet"]);
  });

  it("drops empty task bodies and surrounding whitespace", () => {
    const doc = parseTodoDoc("# ToDo\n\n- [ ]   \n- [ ]   spaced item   \n");
    expect(doc.open.map((i) => i.text)).toEqual(["spaced item"]);
  });

  it("parses CRLF line endings", () => {
    const doc = parseTodoDoc("# ToDo\r\n\r\n- [ ] windows\r\n\r\n## Done\r\n\r\n- [x] old (done: 2026-01-01)\r\n");
    expect(doc.open.map((i) => i.text)).toEqual(["windows"]);
    expect(doc.done[0]).toEqual({ text: "old", doneOn: "2026-01-01" });
  });
});

describe("serializeTodoDoc", () => {
  it("emits the canonical form and ends with exactly one newline", () => {
    const out = serializeTodoDoc(parseTodoDoc(SAMPLE));
    expect(out).toBe(SAMPLE);
    expect(out.endsWith("\n")).toBe(true);
    expect(out.endsWith("\n\n")).toBe(false);
  });

  it("omits the `## Done` heading when there are no done items", () => {
    const out = serializeTodoDoc({ open: [{ text: "a", doneOn: null }], done: [] });
    expect(out).toBe("# ToDo\n\n- [ ] a\n");
  });

  it("writes a done item without a date when doneOn is null", () => {
    const out = serializeTodoDoc({ open: [], done: [{ text: "x", doneOn: null }] });
    expect(out).toBe("# ToDo\n\n\n## Done\n\n- [x] x\n");
  });

  it("round-trips: serialize(parse(x)) is stable", () => {
    const once = serializeTodoDoc(parseTodoDoc(SAMPLE));
    const twice = serializeTodoDoc(parseTodoDoc(once));
    expect(twice).toBe(once);
  });
});

describe("mutations", () => {
  const base: TodoDoc = {
    open: [
      { text: "one", doneOn: null },
      { text: "two", doneOn: null },
    ],
    done: [{ text: "old", doneOn: "2026-09-01" }],
  };

  it("addTodo appends to the end of the open list", () => {
    expect(addTodo(base, "  three  ").open.map((i) => i.text)).toEqual(["one", "two", "three"]);
  });

  it("addTodo ignores a blank entry", () => {
    expect(addTodo(base, "   ")).toBe(base);
  });

  it("completeTodo moves the item to the front of done, stamped with the date", () => {
    const next = completeTodo(base, 0, "2026-09-04");
    expect(next.open.map((i) => i.text)).toEqual(["two"]);
    expect(next.done).toEqual([
      { text: "one", doneOn: "2026-09-04" },
      { text: "old", doneOn: "2026-09-01" },
    ]);
  });

  it("completeTodo strips a stray trailing (done: …) from the text", () => {
    const doc: TodoDoc = { open: [{ text: "task (done: 2020-01-01)", doneOn: null }], done: [] };
    expect(completeTodo(doc, 0, "2026-09-04").done[0]).toEqual({ text: "task", doneOn: "2026-09-04" });
  });

  it("completeTodo is a no-op for an out-of-range index", () => {
    expect(completeTodo(base, 9, "2026-09-04")).toBe(base);
  });

  it("reopenTodo moves a done item back to the end of open, dropping its date", () => {
    const next = reopenTodo(base, 0);
    expect(next.done).toEqual([]);
    expect(next.open.map((i) => i.text)).toEqual(["one", "two", "old"]);
    expect(next.open[next.open.length - 1].doneOn).toBeNull();
  });

  it("deleteOpen / deleteDone remove by index", () => {
    expect(deleteOpen(base, 0).open.map((i) => i.text)).toEqual(["two"]);
    expect(deleteDone(base, 0).done).toEqual([]);
    expect(deleteOpen(base, 5)).toBe(base);
  });
});

describe("todayIso", () => {
  it("formats a local date as YYYY-MM-DD", () => {
    expect(todayIso(new Date(2026, 8, 4))).toBe("2026-09-04");
    expect(todayIso(new Date(2026, 0, 1))).toBe("2026-01-01");
  });
});
