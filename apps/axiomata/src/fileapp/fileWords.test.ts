import { describe, expect, it } from "vitest";

import { filterItems, type CompletionItem } from "../editor/lsp/completion";
import { pos } from "../editor/position";
import type { CompletionPort } from "./completionMenu";
import { fileWordSource, MAX_WORDS, withFileWords, wordItem, wordsAround } from "./fileWords";

const source = (lines: string[]) => ({ lineCount: lines.length, line: (n: number) => lines[n] ?? "" });

function serverItem(label: string): CompletionItem {
  return { ...wordItem(label), detail: "", kind: 3, sortText: label, source: undefined, raw: { label } };
}

describe("wordsAround", () => {
  it("reads outward from the cursor, skips short words, numbers and the word being typed", () => {
    const lines = ["far away", "alpha beta", "gam", "delta 12345 epsilon", "zeta"];
    // The cursor sits in "gam" on line 2.
    expect(wordsAround(source(lines), pos(2, 3))).toEqual(["alpha", "beta", "delta", "epsilon", "far", "away", "zeta"]);
  });

  it("stops at the word limit", () => {
    const many = [Array.from({ length: MAX_WORDS + 50 }, (_, i) => `word${i}`).join(" ")];
    expect(wordsAround(source(many), pos(0, 0))).toHaveLength(MAX_WORDS);
  });
});

describe("fileWordSource", () => {
  it("reads once per word being typed and leaves the typed word out", () => {
    const lines = ["alpha alphabet", "alp"];
    let reads = 0;
    const counted = { lineCount: 2, line: (n: number) => (reads++, lines[n] ?? "") };
    const words = fileWordSource(counted);
    expect(words(pos(1, 3))).toEqual(["alpha", "alphabet"]);
    const after = reads;
    lines[1] = "alpha";
    // Still the same word (it began at column 0): no new read, and "alpha" is now the typed word.
    expect(words(pos(1, 5))).toEqual(["alphabet"]);
    expect(reads - after).toBeLessThanOrEqual(1);
  });
});

describe("withFileWords", () => {
  const words = () => ["alphabet", "helper"];

  it("keeps the words when the server fails", async () => {
    const failing: CompletionPort = {
      ask: async () => {
        throw new Error("server gone");
      },
      resolve: async (item) => item,
      triggers: () => [],
    };
    const answer = await withFileWords(failing, words).ask(pos(0, 0), null, false);
    expect(answer.items.map((i) => i.label)).toEqual(["alphabet", "helper"]);
  });

  it("offers the words alone without a server, and nothing after a trigger", async () => {
    const port = withFileWords(null, words);
    expect((await port.ask(pos(0, 0), null, false)).items.map((i) => i.label)).toEqual(["alphabet", "helper"]);
    expect((await port.ask(pos(0, 0), ".", false)).items).toEqual([]);
    expect(port.triggers()).toEqual([]);
  });

  it("puts the server's items first, drops a word the server already offers, and resolves only server items", async () => {
    const resolved: string[] = [];
    const server: CompletionPort = {
      ask: async () => ({ items: [serverItem("helper")], incomplete: true }),
      resolve: async (item) => {
        resolved.push(item.label);
        return item;
      },
      triggers: () => ["."],
    };
    const port = withFileWords(server, words);
    const answer = await port.ask(pos(0, 0), null, false);
    expect(answer.items.map((i) => [i.label, i.source ?? "server"])).toEqual([
      ["helper", "server"],
      ["alphabet", "file"],
    ]);
    expect(answer.incomplete).toBe(true);
    await port.resolve(answer.items[1]);
    await port.resolve(answer.items[0]);
    expect(resolved).toEqual(["helper"]);
  });

  it("ranks a file word below a server item that matches as well", () => {
    const ranked = filterItems([wordItem("helper"), serverItem("helpers")], "help");
    expect(ranked.map((i) => i.label)).toEqual(["helpers", "helper"]);
  });
});
