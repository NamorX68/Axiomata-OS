import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeBackend = vi.fn();
vi.mock("./backend", () => ({ invokeBackend: (...args: unknown[]) => invokeBackend(...args) }));

const { busy, newSession, noteToolCall, send, toolCalls, turns } = await import("./chat");
type ChatTurn = import("./chat").ChatTurn;

function lastTurn(): ChatTurn | undefined {
  const list = get(turns);
  return list[list.length - 1];
}

describe("tool chips of a chat turn", () => {
  beforeEach(() => {
    newSession();
    toolCalls.set([]);
    busy.set(false);
    invokeBackend.mockReset();
  });

  it("ignores an app action while no chat turn is in progress", () => {
    noteToolCall("calendar · create event", true);
    expect(get(toolCalls)).toEqual([]);
  });

  it("puts the actions called during a turn on its reply", async () => {
    invokeBackend.mockImplementation(async () => {
      noteToolCall("calendar · create event", true);
      noteToolCall("kanban · add card", false);
      return { session_id: "s1", reply_markdown: "Done.", is_error: false, cost_usd: null };
    });
    await send("plan my day", "chat");
    const reply = lastTurn();
    expect(reply?.text).toBe("Done.");
    expect(reply?.tools).toEqual([
      { label: "calendar · create event", ok: true },
      { label: "kanban · add card", ok: false },
    ]);
  });

  it("leaves a reply without actions without chips", async () => {
    invokeBackend.mockResolvedValue({ session_id: "s1", reply_markdown: "Hi.", is_error: false, cost_usd: null });
    await send("hi", "chat");
    expect(lastTurn()?.tools).toBeUndefined();
  });
});
