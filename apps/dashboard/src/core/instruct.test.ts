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
  it("assembles the tool name, parameters and reply instruction into one message", () => {
    const message = buildToolCallInstruction("calendar_events", "mcp__apple-reminders__calendar_events", ['action="create"', 'title="x"'], "OK");
    expect(message).toContain("Call the calendar_events MCP tool (mcp__apple-reminders__calendar_events)");
    expect(message).toContain('action="create", title="x"');
    expect(message).toContain("reply with exactly OK and nothing else.");
    expect(message).toContain("Do not ask for confirmation, do not explain.");
  });
});

describe("runInstructWrite", () => {
  it("calls assistant_send as a fresh instruct turn scoped to allowedTools, returns the trimmed reply", async () => {
    const calls: Record<string, unknown>[] = [];
    const invoke = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      calls.push({ cmd, ...args });
      return { session_id: "s", reply_markdown: "  OK  \n", is_error: false, cost_usd: null, usage: null, duration_ms: 1 } as unknown as T;
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

    const result = await runInstructWrite(invoke, "do the thing", "mcp__apple-reminders__reminders_tasks");
    expect(result).toBe("OK");
    expect(calls).toEqual([
      {
        cmd: "assistant_send",
        message: "do the thing",
        sessionId: null,
        mode: "instruct",
        allowedTools: "mcp__apple-reminders__reminders_tasks",
      },
    ]);
  });

  it("throws the agent's own error text when is_error is set", async () => {
    const invoke = (async <T>(): Promise<T> => ({ session_id: "s", reply_markdown: "not allowed", is_error: true, cost_usd: null, usage: null, duration_ms: 1 }) as unknown as T) as <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ) => Promise<T>;
    await expect(runInstructWrite(invoke, "x", "tool")).rejects.toThrow("not allowed");
  });

  it("falls back to a generic message when an error reply has no text", async () => {
    const invoke = (async <T>(): Promise<T> => ({ session_id: "s", reply_markdown: "  ", is_error: true, cost_usd: null, usage: null, duration_ms: 1 }) as unknown as T) as <T>(
      cmd: string,
      args?: Record<string, unknown>,
    ) => Promise<T>;
    await expect(runInstructWrite(invoke, "x", "tool")).rejects.toThrow("The agent reported an error.");
  });
});
