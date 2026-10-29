import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

import {
  answerChatAsk,
  ARM_MS,
  ASK_TTL_MS,
  askerLabel,
  askOwner,
  chatAsks,
  dropStale,
  MAX_DETAIL,
  MAX_OPEN_ASKS,
} from "./chatAsks";

beforeEach(() => chatAsks.set([]));

const later = () => Date.now() + ARM_MS;

describe("chat asks", () => {
  it("runs an action only when the owner says yes, once", async () => {
    const run = vi.fn(async () => "ran");
    const answer = askOwner({ question: "Run the skill x", source: "start_skill", origin: "chat" }, run);
    expect(answer).toEqual({ asked: true, question: "Run the skill x" });
    expect(run).not.toHaveBeenCalled();
    const [ask] = get(chatAsks);
    await answerChatAsk(ask.id, true, later());
    expect(run).toHaveBeenCalledOnce();
    expect(get(chatAsks)).toEqual([]);
    await answerChatAsk(ask.id, true, later());
    expect(run).toHaveBeenCalledOnce();
  });

  it("drops an action the owner declines", async () => {
    const run = vi.fn(async () => undefined);
    askOwner({ question: "Start card #3", source: "start_card" }, run);
    await answerChatAsk(get(chatAsks)[0].id, false);
    expect(run).not.toHaveBeenCalled();
    expect(get(chatAsks)).toEqual([]);
  });

  it("takes no yes before the question is armed, nor after it went stale", async () => {
    const run = vi.fn(async () => undefined);
    askOwner({ question: "q", source: "s" }, run);
    const [ask] = get(chatAsks);
    await answerChatAsk(ask.id, true, ask.at);
    expect(get(chatAsks), "a stray click answers nothing").toHaveLength(1);
    await answerChatAsk(ask.id, true, ask.at + ASK_TTL_MS);
    expect(run).not.toHaveBeenCalled();
    askOwner({ question: "old", source: "s" }, run);
    expect(dropStale(Date.now() + ASK_TTL_MS)).toBe(1);
  });

  it("cannot allow a detail too long to show", async () => {
    const run = vi.fn(async () => undefined);
    askOwner({ question: "Create a routine", detail: "x".repeat(MAX_DETAIL + 1), source: "save_routine" }, run);
    const [ask] = get(chatAsks);
    expect(ask.complete).toBe(false);
    await answerChatAsk(ask.id, true, later());
    expect(run).not.toHaveBeenCalled();
  });

  it("shows plain lines without format characters, asks once, and limits how many wait", () => {
    askOwner({ question: "a\nb   c‮", source: "x" }, async () => undefined);
    expect(get(chatAsks)[0].what).toBe("a b c");
    askOwner({ question: "a b c", source: "x" }, async () => undefined);
    expect(get(chatAsks), "the same question waits once").toHaveLength(1);
    for (let i = 1; i < MAX_OPEN_ASKS; i++) askOwner({ question: `q${i}`, source: "x" }, async () => undefined);
    expect(() => askOwner({ question: "one too many", source: "x" }, async () => undefined)).toThrow(/questions open/);
  });

  it("names who asks", () => {
    expect(askerLabel("chat")).toBe("The chat asks");
    expect(askerLabel("agent:worker")).toBe("Agent worker asks");
    expect(askerLabel(null)).toBe("A command asks");
  });
});
