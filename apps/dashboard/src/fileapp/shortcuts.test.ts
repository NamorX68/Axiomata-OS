import { describe, expect, it } from "vitest";

import { foldKeyAction, keyAction, type KeyInput } from "../editor/keymap";
import { EX_COMMAND_NAMES } from "../editor/vi/ex";
import { VI_GRAMMAR } from "../editor/vi/parse";
import { keyPresses, SHORTCUTS, visibleShortcuts } from "./shortcuts";

/** What the key map does with a press, as one comparable value; `null` for nothing. */
function actionOf(input: KeyInput): string | null {
  const action = keyAction(input);
  if (action) return JSON.stringify(action);
  const fold = foldKeyAction(input);
  return fold ? `fold:${fold}` : null;
}

/** Every press a checked entry stands for, ⇧ added for the moves that extend. */
function listedPresses(): { entry: string; input: KeyInput }[] {
  const out: { entry: string; input: KeyInput }[] = [];
  for (const group of SHORTCUTS.filter((g) => g.checked)) {
    for (const item of group.items) {
      // A note like "⇧ + any move" stands for no single key.
      if (item.keys.includes(" + ")) continue;
      const presses = keyPresses(item.keys);
      if (!presses) throw new Error(`"${item.keys}" in ${group.title} is not a key the test can press`);
      for (const input of presses) {
        out.push({ entry: item.keys, input });
        if (item.extends) out.push({ entry: item.keys, input: { ...input, shift: true } });
      }
    }
  }
  return out;
}

const KEYS = [
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Backspace",
  "Delete",
  "Tab",
  "Enter",
  "Escape",
  ..."abcdefghijklmnopqrstuvwxyz0123456789/[]-=.,;'\\`",
];

describe("the shortcut list (LK1, K2)", () => {
  it("lists only keys the editor really answers to", () => {
    for (const { entry, input } of listedPresses()) {
      expect(actionOf(input), `${entry}: ${JSON.stringify(input)}`).not.toBeNull();
    }
  });

  it("lists everything the key map does", () => {
    const listed = new Set(listedPresses().map(({ input }) => actionOf(input)));
    const missing = new Set<string>();
    for (const key of KEYS) {
      for (let mods = 0; mods < 8; mods++) {
        const input = { key, meta: !!(mods & 1), alt: !!(mods & 2), shift: !!(mods & 4), ctrl: false };
        const action = actionOf(input);
        if (action && !listed.has(action)) missing.add(`${JSON.stringify(input)} → ${action}`);
      }
    }
    expect([...missing]).toEqual([]);
  });

  it("names every key of the Vi grammar and every : command", () => {
    const tokens = new Set(
      SHORTCUTS.filter((g) => g.vi)
        .flatMap((g) => g.items)
        .flatMap((s) => s.keys.split(/\s+/))
        // `f{c}`, `'{a-z}` and `ys{motion}{c}` stand for their key.
        .map((t) => t.replace(/\{[^}]*\}/g, "")),
    );
    const spoken: Record<string, string> = { " ": "<Space>" };
    const grammar = Object.values(VI_GRAMMAR).flat();
    const missing = grammar.filter((key) => !tokens.has(spoken[key] ?? key));
    expect(missing).toEqual([]);
    // `:w[rite]` names `write`; `:s[ubstitute]/a/b/gc` names `substitute`.
    const ex = new Set(
      [...tokens].filter((t) => t.startsWith(":")).map((t) => t.replace(/[[\]!]/g, "").slice(1).split("/")[0]),
    );
    expect(EX_COMMAND_NAMES.filter((name) => !ex.has(name))).toEqual([]);
  });

  it("shows the Vi groups only with Vi on, and narrows by keys or words", () => {
    const titles = (groups: { title: string }[]) => groups.map((g) => g.title);
    expect(titles(visibleShortcuts(false, "")).some((t) => t.startsWith("Vi"))).toBe(false);
    expect(titles(visibleShortcuts(true, "")).some((t) => t.startsWith("Vi"))).toBe(true);
    const found = visibleShortcuts(false, "rename");
    expect(found.flatMap((g) => g.items).map((s) => s.keys)).toEqual(["F2"]);
    expect(visibleShortcuts(false, "⌘D").flatMap((g) => g.items).map((s) => s.what)).toEqual([
      "Add the next occurrence",
    ]);
  });

  it("turns written keys into presses", () => {
    expect(keyPresses("⌥⇧↑ / ⌘/")).toEqual([
      { key: "ArrowUp", meta: false, alt: true, shift: true, ctrl: false },
      { key: "/", meta: true, alt: false, shift: false, ctrl: false },
    ]);
    expect(keyPresses("⌘-click")).toBeNull();
  });
});
