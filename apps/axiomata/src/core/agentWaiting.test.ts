import { get, writable } from "svelte/store";
import { describe, expect, it } from "vitest";

import {
  askDecisions,
  badgeClick,
  parseAsk,
  parseWaiting,
  placeList,
  withoutAsk,
  waitedFor,
  waitingOnCard,
  watchWaiting,
  type WaitingAgent,
} from "./agentWaiting";

const agent = (id: number, cardId: number | null = null): WaitingAgent => ({
  agent_id: id,
  project_id: 1,
  name: `implementer-${id}`,
  card_id: cardId,
  card_title: cardId === null ? null : `Card ${cardId}`,
  since: null,
  reason: "prompt",
  ask: null,
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
      {
        agent_id: 3,
        project_id: 1,
        name: "a",
        card_id: 12,
        card_title: "Fix",
        since: "2026-10-12T10:00:00Z",
        reason: "prompt",
        ask: null,
      },
      { agent_id: 4, project_id: 1, name: "b", card_id: null, card_title: null, since: null, reason: "prompt", ask: null },
    ]);
  });

  it("drops an entry whose since is not a string, and keeps one whose since is null", () => {
    expect(
      parseWaiting([
        { agent_id: 8, project_id: 1, name: "e", since: 1_700_000_000_000 },
        { agent_id: 9, project_id: 1, name: "f", since: null },
      ]),
    ).toEqual([
      { agent_id: 9, project_id: 1, name: "f", card_id: null, card_title: null, since: null, reason: "prompt", ask: null },
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

  it("switches the unit exactly at the minute and the hour", () => {
    const start = Date.parse("2026-10-12T10:00:00Z");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 59_000)).toBe("now");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 60_000)).toBe("1 min");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 59 * 60_000)).toBe("59 min");
    expect(waitedFor("2026-10-12T10:00:00Z", start + 60 * 60_000)).toBe("1 h");
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

  it("rejects when listening fails, and does not read the list", async () => {
    const store = writable<WaitingAgent[]>([agent(7)]);
    let read = false;
    await expect(
      watchWaiting(
        async () => {
          read = true;
          return [agent(1)];
        },
        async () => {
          throw new Error("no channel");
        },
        store,
      ),
    ).rejects.toThrow("no channel");
    expect(read).toBe(false);
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

describe("permission requests in the waiting list", () => {
  it("reads a well-formed request and the plan-draft reason", () => {
    const [entry] = parseWaiting([
      {
        agent_id: 1,
        project_id: 1,
        name: "a",
        reason: "plan_draft",
        ask: { id: "7", source: "claude", tool: "Bash", detail: "npm ci" },
      },
    ]);
    expect(entry?.reason).toBe("plan_draft");
    expect(entry?.ask).toEqual({ id: "7", source: "claude", tool: "Bash", detail: "npm ci" });
  });

  it("drops a malformed request but keeps its agent", () => {
    const [entry] = parseWaiting([{ agent_id: 1, project_id: 1, name: "a", ask: { id: 7, source: "claude" } }]);
    expect(entry?.agent_id).toBe(1);
    expect(entry?.ask).toBeNull();
    expect(parseAsk({ id: "1", source: "other", tool: "x", detail: "" })).toBeNull();
    expect(parseAsk("nonsense")).toBeNull();
  });

  it("offers 'in terminal' only for Claude Code, whose pane prompt waits behind the hook", () => {
    expect(askDecisions({ id: "1", source: "claude", tool: "Bash", detail: "" })).toEqual(["allow", "deny", "terminal"]);
  });
});

describe("answering from the list", () => {
  const asking = (id: number): WaitingAgent => ({
    ...agent(id),
    ask: { id: "1", source: "claude", tool: "Bash", detail: "ls" },
  });

  it("opens the list even for one agent whose request can be answered there", () => {
    expect(badgeClick([asking(1)])).toEqual({ kind: "list" });
    expect(badgeClick([agent(1)]).kind).toBe("open");
  });

  it("takes an answered request out of the list and keeps its agent", () => {
    const after = withoutAsk([asking(1), asking(2)], 1);
    expect(after.map((a) => a.ask === null)).toEqual([true, false]);
    expect(after).toHaveLength(2);
  });
});
