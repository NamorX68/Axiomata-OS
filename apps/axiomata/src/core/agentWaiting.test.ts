import { get, writable } from "svelte/store";
import { describe, expect, it } from "vitest";

import { badgeClick, parseWaiting, placeList, waitedFor, waitingOnCard, watchWaiting, type WaitingAgent } from "./agentWaiting";

const agent = (id: number, cardId: number | null = null): WaitingAgent => ({
  agent_id: id,
  project_id: 1,
  name: `implementer-${id}`,
  card_id: cardId,
  card_title: cardId === null ? null : `Card ${cardId}`,
  since: null,
});

describe("the waiting agents", () => {
  it("keeps only well-formed entries from the backend's answer", () => {
    expect(parseWaiting(null)).toEqual([]);
    expect(
      parseWaiting([
        { agent_id: 3, project_id: 1, name: "a", card_id: 12, card_title: "Fix", since: "2026-10-12T10:00:00Z" },
        { agent_id: 4, project_id: 1, name: "b" },
        { agent_id: "5", project_id: 1, name: "c" },
        { agent_id: 6, project_id: 1, name: "d", card_id: "12" },
        "nonsense",
      ]),
    ).toEqual([
      { agent_id: 3, project_id: 1, name: "a", card_id: 12, card_title: "Fix", since: "2026-10-12T10:00:00Z" },
      { agent_id: 4, project_id: 1, name: "b", card_id: null, card_title: null, since: null },
    ]);
  });

  it("finds the agents of one card", () => {
    expect(waitingOnCard([agent(1, 12), agent(2, 13), agent(3, 12)], 12).map((a) => a.agent_id)).toEqual([1, 3]);
  });

  it("opens the one agent directly and lists several", () => {
    expect(badgeClick([])).toEqual({ kind: "none" });
    expect(badgeClick([agent(1)])).toEqual({ kind: "open", agent: agent(1) });
    expect(badgeClick([agent(1), agent(2)])).toEqual({ kind: "list" });
  });

  it("says how long an agent has waited", () => {
    const start = Date.parse("2026-10-12T10:00:00Z");
    expect(waitedFor(null, start)).toBe("");
    expect(waitedFor("garbage", start)).toBe("");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 30_000)).toBe("now");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 4 * 60_000)).toBe("4 min");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 125 * 60_000)).toBe("2 h");
  });

  it("takes the first read, unless a push overtook it", async () => {
    const store = writable<WaitingAgent[]>([]);
    let push: (payload: unknown) => void = () => {};
    const listen = async (handler: (payload: unknown) => void): Promise<() => void> => {
      push = handler;
      return () => {};
    };
    await watchWaiting(async () => [agent(1)], listen, store);
    expect(get(store).map((a) => a.agent_id)).toEqual([1]);
    push([agent(2)]);
    expect(get(store).map((a) => a.agent_id)).toEqual([2]);

    const overtaken = writable<WaitingAgent[]>([]);
    await watchWaiting(
      async () => {
        push([agent(9)]);
        return [agent(1)];
      },
      listen,
      overtaken,
    );
    expect(get(overtaken).map((a) => a.agent_id)).toEqual([9]);
  });

  it("keeps the list when the first read fails", async () => {
    const store = writable<WaitingAgent[]>([agent(7)]);
    await watchWaiting(
      async () => {
        throw new Error("down");
      },
      async () => () => {},
      store,
    );
    expect(get(store).map((a) => a.agent_id)).toEqual([7]);
  });

  describe("where the list opens", () => {
    const viewport = { width: 1000, height: 800 };
    const size = { width: 300, height: 200 };
    const at = (left: number, top: number) => ({ left, top, right: left + 20, bottom: top + 20 });

    it("opens below the badge, aligned to its right edge", () => {
      expect(placeList(at(500, 100), size, viewport, 6, 8)).toEqual({ left: 220, top: 126 });
    });

    it("stays inside the window at the left and right edge", () => {
      expect(placeList(at(10, 100), size, viewport, 6, 8).left).toBe(8);
      expect(placeList(at(990, 100), size, viewport, 6, 8).left).toBe(692);
    });

    it("opens above when there is no room below, and below when there is room for neither", () => {
      expect(placeList(at(500, 700), size, viewport, 6, 8).top).toBe(494);
      expect(placeList(at(500, 100), size, { width: 1000, height: 250 }, 6, 8).top).toBe(42);
    });
  });
});
