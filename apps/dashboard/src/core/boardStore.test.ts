import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("./backend", () => ({ invokeBackend: invoke }));

const { boardStore, forgetBoard, refreshBoard, resetBoardStores } = await import("./boardStore");

/** Resolves only when released, so two loads can be made to overlap on purpose. */
function deferred<T>() {
  let release!: (value: T) => void;
  const promise = new Promise<T>((resolve) => (release = resolve));
  return { promise, release };
}

beforeEach(() => {
  invoke.mockReset();
  resetBoardStores();
});

function answerWith(board: unknown, columns: unknown[], cards: unknown[]) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_board") return Promise.resolve(board);
    if (cmd === "list_board_columns") return Promise.resolve(columns);
    return Promise.resolve(cards);
  });
}

describe("refreshBoard", () => {
  it("loads the board, its columns and its cards", async () => {
    answerWith({ id: 1, name: "B" }, [{ id: 7 }], [{ id: 70 }]);
    await refreshBoard(1);

    const data = get(boardStore(1));
    expect(data.board).toEqual({ id: 1, name: "B" });
    expect(data.columns).toHaveLength(1);
    expect(data.cards).toHaveLength(1);
    expect(data.loading).toBe(false);
    expect(data.error).toBe("");
  });

  /** The case the shared store exists for: a tile and a panel showing the same
   *  board mount together. Without in-flight tracking both fire their own
   *  three round-trips for the same answer. */
  it("joins a load already running instead of starting a second one", async () => {
    const gate = deferred<unknown>();
    invoke.mockImplementation(() => gate.promise);

    const first = refreshBoard(1);
    const second = refreshBoard(1);

    expect(invoke).toHaveBeenCalledTimes(3);

    gate.release([]);
    await Promise.all([first, second]);
    expect(invoke).toHaveBeenCalledTimes(3);
  });

  it("starts a fresh load once the previous one has finished", async () => {
    answerWith(null, [], []);
    await refreshBoard(1);
    await refreshBoard(1);
    expect(invoke).toHaveBeenCalledTimes(6);
  });

  it("keeps boards apart", async () => {
    answerWith(null, [], []);
    await Promise.all([refreshBoard(1), refreshBoard(2)]);
    expect(invoke).toHaveBeenCalledTimes(6);
  });

  it("records a backend failure on the board instead of throwing", async () => {
    invoke.mockRejectedValue("no such board");
    await refreshBoard(9);

    const data = get(boardStore(9));
    expect(data.error).toContain("no such board");
    expect(data.loading).toBe(false);
  });
});

describe("boardStore", () => {
  it("hands the same store to every caller for one board", () => {
    answerWith(null, [], []);
    expect(boardStore(3)).toBe(boardStore(3));
    expect(boardStore(3)).not.toBe(boardStore(4));
  });

  it("forgetBoard drops the cached store so a deleted board is not resurrected", async () => {
    answerWith({ id: 5, name: "weg" }, [], []);
    await refreshBoard(5);
    expect(get(boardStore(5)).board).not.toBeNull();

    forgetBoard(5);
    expect(get(boardStore(5)).board).toBeNull();
  });
});
