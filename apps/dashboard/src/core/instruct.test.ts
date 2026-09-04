import { describe, expect, it } from "vitest";

import { buildToolCallInstruction, quoteForInstruction, runInstructWrite } from "./instruct";

describe("quoteForInstruction", () => {
  it("wraps a plain value in quotes", () => {
    expect(quoteForInstruction("Zahnarzt")).toBe('"Zahnarzt"');
  });

  it("escapes an embedded quote", () => {
    expect(quoteForInstruction('Say "hi"')).toBe('"Say \\"hi\\""');
  });

  it("escapes an embedded backslash before escaping quotes (order matters)", () => {
    expect(quoteForInstruction('a\\"b')).toBe('"a\\\\\\"b"');
  });
});

describe("buildToolCallInstruction", () => {
  it("assembles the tool name, parameters and reply instruction into one message, for a literal reply", () => {
    const instruction = buildToolCallInstruction("calendar_events", "mcp__apple-reminders__calendar_events", ['action="create"', 'title="x"'], {
      kind: "literal",
      value: "OK",
    });
    expect(instruction.message).toContain("Call the calendar_events MCP tool (mcp__apple-reminders__calendar_events)");
    expect(instruction.message).toContain('action="create", title="x"');
    expect(instruction.message).toContain("reply with exactly OK and nothing else.");
    expect(instruction.message).toContain("Do not ask for confirmation, do not explain.");
    expect(instruction.expect).toEqual({ kind: "literal", value: "OK" });
  });

  it("asks for the created item's id when expect.kind is \"id\"", () => {
    const instruction = buildToolCallInstruction("reminders_tasks", "mcp__apple-reminders__reminders_tasks", ['action="create"'], { kind: "id" });
    expect(instruction.message).toContain("reply with exactly the created item's id and nothing else.");
    expect(instruction.expect).toEqual({ kind: "id" });
  });
});

describe("runInstructWrite", () => {
  function fakeInvoke(reply: { reply_markdown: string; is_error?: boolean }) {
    const calls: Record<string, unknown>[] = [];
    const invoke = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      calls.push({ cmd, ...args });
      return { session_id: "s", is_error: false, cost_usd: null, usage: null, duration_ms: 1, ...reply } as unknown as T;
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    return { invoke, calls };
  }

  const literalOk = buildToolCallInstruction("t", "tool", [], { kind: "literal", value: "OK" });
  const wantsId = buildToolCallInstruction("t", "tool", [], { kind: "id" });

  it("calls assistant_send as a fresh instruct turn scoped to allowedTools", async () => {
    const { invoke, calls } = fakeInvoke({ reply_markdown: "  OK  \n" });
    const result = await runInstructWrite(invoke, literalOk, "mcp__apple-reminders__reminders_tasks");
    expect(result).toBe("OK");
    expect(calls).toEqual([
      {
        cmd: "assistant_send",
        message: literalOk.message,
        sessionId: null,
        mode: "instruct",
        allowedTools: "mcp__apple-reminders__reminders_tasks",
      },
    ]);
  });

  it("accepts a literal reply case- and whitespace-insensitively", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "  ok\n" });
    await expect(runInstructWrite(invoke, literalOk, "tool")).resolves.toBe("ok");
  });

  it("throws when a literal-expecting reply doesn't match — the turn succeeding isn't the same as the write succeeding", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "I could not find that event." });
    await expect(runInstructWrite(invoke, literalOk, "tool")).rejects.toThrow(/didn't confirm the write/);
  });

  it("accepts a UUID-shaped id reply", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "11A9AA93-4EFF-4E4E-B693-8702BC20D97F" });
    await expect(runInstructWrite(invoke, wantsId, "tool")).resolves.toBe("11A9AA93-4EFF-4E4E-B693-8702BC20D97F");
  });

  it("accepts a colon-joined calendar-event-shaped id reply", async () => {
    const id = "452EF5AC-7639-48F1-85D4-B723AB5E2A18:8D0E8E79-A41E-43D7-B107-90EA5FC7A970";
    const { invoke } = fakeInvoke({ reply_markdown: id });
    await expect(runInstructWrite(invoke, wantsId, "tool")).resolves.toBe(id);
  });

  it("rejects a short natural-language reply that could pass as a single token", async () => {
    for (const word of ["Error", "Failed", "Denied", "None", "NotFound"]) {
      const { invoke } = fakeInvoke({ reply_markdown: word });
      await expect(runInstructWrite(invoke, wantsId, "tool")).rejects.toThrow(/could not be confirmed/);
    }
  });

  it("rejects an id-expecting reply with whitespace in it", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "Sure, I created the event for you!" });
    await expect(runInstructWrite(invoke, wantsId, "tool")).rejects.toThrow(/could not be confirmed/);
  });

  it("throws the agent's own error text when is_error is set", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "not allowed", is_error: true });
    await expect(runInstructWrite(invoke, literalOk, "tool")).rejects.toThrow("not allowed");
  });

  it("falls back to a generic message when an error reply has no text", async () => {
    const { invoke } = fakeInvoke({ reply_markdown: "  ", is_error: true });
    await expect(runInstructWrite(invoke, literalOk, "tool")).rejects.toThrow("The agent reported an error.");
  });
});
