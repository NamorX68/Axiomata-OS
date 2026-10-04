import { afterEach, describe, expect, it, vi } from "vitest";

import { run, UNWRAPPED } from "../editor/commands";
import { EditorDocument } from "../editor/document";
import { parseItem, type CompletionAnswer, type CompletionItem } from "../editor/lsp/completion";
import { cursor, pos, type Pos } from "../editor/position";
import { CompletionMenu, RESOLVE_LIMIT_MS, wordStart, type CompletionPort } from "./completionMenu";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null };

function doc(text: string, at: Pos): EditorDocument {
  const d = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
  d.setSelection(cursor(at));
  return d;
}

function type(d: EditorDocument, text: string, menu: CompletionMenu): void {
  run(d, { type: "insert", text }, ctx);
  menu.typed(d, text);
}

const items = (...labels: string[]): CompletionItem[] => labels.map((label) => parseItem({ label })!);

/** Lets queued promise callbacks run. */
async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

/** A server whose answers the test hands out by hand. */
function port(triggers: string[] = ["."]) {
  const asked: { at: Pos; trigger: string | null; again: boolean; answer: (a: CompletionAnswer) => void }[] = [];
  const resolve = vi.fn(async (item: CompletionItem) => item);
  const p: CompletionPort = {
    ask: (at, trigger, again) => new Promise((answer) => asked.push({ at, trigger, again, answer })),
    resolve,
    triggers: () => triggers,
  };
  return { p, asked, resolve };
}

describe("CompletionMenu (ED6.4)", () => {
  afterEach(() => vi.useRealTimers());

  it("finds where the word begins", () => {
    expect(wordStart("let fooBar", 10)).toBe(4);
    expect(wordStart("x.", 2)).toBe(2);
  });

  it("opens on a word, narrows as it grows, and closes when the word is left", async () => {
    const { p, asked } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("", pos(0, 0));
    type(d, "p", menu);
    expect(asked).toHaveLength(1);
    asked[0].answer({ items: items("push", "pop", "len"), incomplete: false });
    await settle();
    // Equally good matches keep the server's order (here its sort text, the label).
    expect(menu.view?.items.map((i) => i.label)).toEqual(["pop", "push"]);
    expect(menu.view?.anchor).toEqual(pos(0, 0));
    type(d, "u", menu);
    expect(menu.view?.items.map((i) => i.label)).toEqual(["push"]);
    expect(asked).toHaveLength(1);
    type(d, "(", menu);
    expect(menu.isOpen).toBe(false);
  });

  it("opens after a trigger character with an empty word, and asks again while the list is incomplete", async () => {
    const { p, asked } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("v", pos(0, 1));
    type(d, ".", menu);
    expect(asked[0]).toMatchObject({ trigger: ".", again: false });
    asked[0].answer({ items: items("len", "iter"), incomplete: true });
    await settle();
    expect(menu.view?.anchor).toEqual(pos(0, 2));
    type(d, "i", menu);
    expect(asked[1]).toMatchObject({ trigger: null, again: true });
    asked[1].answer({ items: items("iter", "into_iter"), incomplete: false });
    await settle();
    expect(menu.view?.items.map((i) => i.label)).toEqual(["into_iter", "iter"]);
  });

  it("drops an answer that a newer request overtook, and one for a word already left", async () => {
    const { p, asked } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("", pos(0, 0));
    type(d, "a", menu);
    menu.invoke(d);
    asked[0].answer({ items: items("abc"), incomplete: false });
    await settle();
    expect(menu.isOpen).toBe(false);
    run(d, { type: "insert", text: " " }, ctx);
    asked[1].answer({ items: items("abc"), incomplete: false });
    await settle();
    expect(menu.isOpen).toBe(false);
  });

  it("chooses round the ends and takes the chosen item as an edit", async () => {
    const { p, asked } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("  x", pos(0, 3));
    menu.invoke(d);
    asked[0].answer({ items: items("x1", "x2"), incomplete: false });
    await settle();
    menu.move(-1);
    expect(menu.view?.selected).toBe(1);
    const taking = menu.take(d);
    const edit = await taking;
    expect(edit).toMatchObject({ before: 1, after: 0, text: "x2" });
    expect(menu.isOpen).toBe(false);
  });

  it("waits a little for an item's extra edits, and gives up when the text changed meanwhile", async () => {
    vi.useFakeTimers();
    const slow = port();
    slow.resolve.mockImplementation(() => new Promise<CompletionItem>(() => {}));
    const menu = new CompletionMenu(slow.p, () => {});
    const d = doc("H", pos(0, 1));
    menu.invoke(d);
    slow.asked[0].answer({ items: items("HashMap"), incomplete: false });
    await settle();
    const taking = menu.take(d);
    vi.advanceTimersByTime(RESOLVE_LIMIT_MS);
    expect(await taking).toMatchObject({ text: "HashMap", extra: [] });

    const menu2 = new CompletionMenu(slow.p, () => {});
    menu2.invoke(d);
    slow.asked[1].answer({ items: items("HashMap"), incomplete: false });
    await settle();
    const taking2 = menu2.take(d);
    run(d, { type: "insert", text: "a" }, ctx);
    vi.advanceTimersByTime(RESOLVE_LIMIT_MS);
    expect(await taking2).toBeNull();
  });

  it("reuses the resolve the documentation started when the item is taken", async () => {
    const { p, asked, resolve } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("H", pos(0, 1));
    menu.invoke(d);
    asked[0].answer({ items: items("HashMap"), incomplete: false });
    await settle();
    void menu.details(menu.view!.items[0]);
    await menu.take(d);
    expect(resolve).toHaveBeenCalledTimes(1);
  });

  it("offers nothing with several cursors", () => {
    const { p, asked } = port();
    const menu = new CompletionMenu(p, () => {});
    const d = doc("a\nb", pos(0, 1));
    d.setSelections(cursor(pos(0, 1)), [cursor(pos(1, 1))]);
    menu.invoke(d);
    expect(asked).toHaveLength(0);
  });
});
