import { beforeAll, describe, expect, it } from "vitest";

import { registerBuiltins } from ".";
import { invokeAction, manifest } from "../core/registry";
import { boardPathPatch, boardToOpen } from "./kanbanApp";

describe("boardToOpen", () => {
  const boards = [{ id: 3 }, { id: 5 }];

  it("opens the board used last if it still exists", () => {
    expect(boardToOpen(boards, 5)).toBe(5);
  });

  it("falls back to the first board when the remembered one is gone or unknown", () => {
    expect(boardToOpen(boards, 9)).toBe(3);
    expect(boardToOpen(boards, undefined)).toBe(3);
  });

  it("is null when there is no board, so the panel offers to create the first one", () => {
    expect(boardToOpen([], 4)).toBeNull();
    expect(boardToOpen([], undefined)).toBeNull();
  });
});

describe("boardPathPatch", () => {
  it("renames a board panel after it switched board, so the ring entry raises it instead of opening a second one", () => {
    expect(boardPathPatch({ path: "board:3", boardId: 3 }, 5)).toEqual({ path: "board:5" });
    expect(boardPathPatch({ path: "board:3" }, null)).toEqual({ path: "board:none" });
    expect(boardPathPatch({ path: "board:none" }, 7)).toEqual({ path: "board:7" });
  });

  it("leaves a single card's panel and a panel without a path alone", () => {
    expect(boardPathPatch({ path: "card:9", boardId: 3 }, 5)).toEqual({});
    expect(boardPathPatch({ boardId: 3 }, 5)).toEqual({});
  });
});

describe("the Kanban app's assistant actions", () => {
  beforeAll(() => registerBuiltins());

  it("are shell actions, so they exist without any Kanban instance on the canvas", () => {
    const shell = manifest().find((entry) => entry.instance_id === "shell");
    const names = shell?.actions.map((a) => a.name) ?? [];
    expect(names).toEqual(expect.arrayContaining(["kanban_list_cards", "kanban_add_card", "kanban_move_card"]));
    // And no Kanban entry of its own is listed: there is nothing mounted to list.
    expect(manifest().some((entry) => entry.type === "kanban")).toBe(false);
  });

  it("run through the shell instance", async () => {
    await expect(invokeAction("shell", "kanban_nope", {})).rejects.toThrow(/no action/);
  });
});
