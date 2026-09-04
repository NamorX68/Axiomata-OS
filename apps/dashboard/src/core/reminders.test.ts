import { describe, expect, it } from "vitest";

import type { RunRecord, RunSummary } from "./backend";
import { completeReminderTask, createReminderTask, defaultList, deleteReminderTask, loadLatestReminderDigest, parseReminderDigest, tasksForList } from "./reminders";

const DIGEST_JSON = JSON.stringify({
  lists: ["Einkaufen", "Arbeit", "Baumarkt"],
  tasks: [
    { id: "t-1", title: "Milch kaufen", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
    { id: "t-2", title: "Bericht abgeben", list: "Arbeit", notes: "bis Freitag", dueDate: "2026-09-11", priority: "high" },
    { id: "t-3", title: "Schrauben besorgen", list: "Baumarkt", notes: null, dueDate: null, priority: "low" },
  ],
});

describe("parseReminderDigest", () => {
  it("parses a well-formed digest", () => {
    const d = parseReminderDigest(DIGEST_JSON);
    expect(d.lists).toEqual(["Einkaufen", "Arbeit", "Baumarkt"]);
    expect(d.tasks).toHaveLength(3);
    expect(d.tasks[1]).toEqual({ id: "t-2", title: "Bericht abgeben", list: "Arbeit", notes: "bis Freitag", dueDate: "2026-09-11", priority: "high" });
  });

  it("strips a ```json fence the model added despite being told not to", () => {
    const fenced = "```json\n" + DIGEST_JSON + "\n```";
    expect(parseReminderDigest(fenced)).toEqual(parseReminderDigest(DIGEST_JSON));
  });

  it("throws the skill's own error message when it reports no reminders tool", () => {
    const noTool = JSON.stringify({ lists: [], tasks: [], error: "no reminders tool available" });
    expect(() => parseReminderDigest(noTool)).toThrow("no reminders tool available");
  });

  it("throws on unparseable output", () => {
    expect(() => parseReminderDigest("nope")).toThrow(/not valid JSON/);
  });

  it("drops a task with an invalid priority instead of throwing on the whole digest", () => {
    const mixed = JSON.stringify({
      lists: ["Einkaufen"],
      tasks: [
        { id: "t-1", title: "ok", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
        { id: "t-2", title: "bad priority", list: "Einkaufen", notes: null, dueDate: null, priority: "urgent!!" },
        { title: "missing id and list" },
      ],
    });
    const d = parseReminderDigest(mixed);
    expect(d.tasks).toHaveLength(1);
    expect(d.tasks[0].title).toBe("ok");
  });
});

describe("tasksForList", () => {
  it("returns only the tasks on the given list", () => {
    const { tasks } = parseReminderDigest(DIGEST_JSON);
    expect(tasksForList(tasks, "Arbeit").map((t) => t.title)).toEqual(["Bericht abgeben"]);
  });

  it("returns an empty list for a list with no open tasks", () => {
    const { tasks } = parseReminderDigest(DIGEST_JSON);
    expect(tasksForList(tasks, "Geschenke")).toEqual([]);
  });
});

describe("defaultList", () => {
  it("picks the alphabetically first list", () => {
    expect(defaultList(["Baumarkt", "Arbeit", "Einkaufen"])).toBe("Arbeit");
  });

  it("returns null for no lists at all", () => {
    expect(defaultList([])).toBeNull();
  });
});

describe("loadLatestReminderDigest", () => {
  const summary = (over: Partial<RunSummary>): RunSummary => ({
    id: 1,
    skill_name: "reminders-digest",
    backend: "claude-code",
    status: "success",
    exit_code: 0,
    duration_ms: 100,
    error: null,
    started_at: "2026-09-05T09:00:00Z",
    source: "manual",
    ...over,
  });

  function fakeInvoke(runs: RunSummary[], records: Record<number, RunRecord>) {
    return (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd === "list_runs") return runs as unknown as T;
      if (cmd === "get_run") return (records[(args as { id: number }).id] ?? null) as unknown as T;
      throw new Error(`unexpected cmd ${cmd}`);
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  }

  it("returns an empty digest with no error when the skill has never run", async () => {
    const result = await loadLatestReminderDigest(fakeInvoke([], {}));
    expect(result).toEqual({ run: null, digest: { lists: [], tasks: [] }, error: null });
  });

  it("parses the latest successful run", async () => {
    const records = { 1: { ...summary({}), stdout: DIGEST_JSON, stderr: "", finished_at: "" } };
    const result = await loadLatestReminderDigest(fakeInvoke([summary({})], records));
    expect(result.digest.lists).toEqual(["Einkaufen", "Arbeit", "Baumarkt"]);
    expect(result.error).toBeNull();
  });

  it("surfaces a failed run's own error", async () => {
    const runs = [summary({ status: "failed", error: "agent timed out" })];
    const result = await loadLatestReminderDigest(fakeInvoke(runs, {}));
    expect(result.error).toBe("agent timed out");
  });
});

describe("createReminderTask / completeReminderTask / deleteReminderTask", () => {
  /** A fake `invoke` that answers `assistant_send` with a fixed reply,
   *  and records the exact instruct message it was sent. */
  function fakeAssistant(reply: { reply_markdown: string; is_error?: boolean }) {
    const calls: Record<string, unknown>[] = [];
    const invoke = (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (cmd !== "assistant_send") throw new Error(`unexpected cmd ${cmd}`);
      calls.push(args ?? {});
      return { session_id: "s", is_error: false, cost_usd: null, usage: null, duration_ms: 1, ...reply } as unknown as T;
    }) as <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    return { invoke, calls };
  }

  it("creates a task with no due date/notes and returns it with the reported id, priority none", async () => {
    const { invoke, calls } = fakeAssistant({ reply_markdown: "NEW-TASK-1234567890AB" });
    const task = await createReminderTask(invoke, { title: "Milch kaufen", list: "Einkaufen", dueDate: null, notes: null });
    expect(task).toEqual({ id: "NEW-TASK-1234567890AB", title: "Milch kaufen", list: "Einkaufen", notes: null, dueDate: null, priority: "none" });
    const message = String(calls[0].message);
    expect(message).toContain('action="create"');
    expect(message).toContain('title="Milch kaufen"');
    expect(message).toContain('targetList="Einkaufen"');
    expect(message).not.toContain("dueDate=");
    expect(message).not.toContain("note=");
    expect(calls[0].allowedTools).toBe("mcp__apple-reminders__reminders_tasks");
  });

  it("includes dueDate/note only when given", async () => {
    const { invoke, calls } = fakeAssistant({ reply_markdown: "NEW-TASK-9876543210CD" });
    const task = await createReminderTask(invoke, { title: "Bericht", list: "Arbeit", dueDate: "2026-09-11", notes: "bis Freitag" });
    expect(task.dueDate).toBe("2026-09-11");
    const message = String(calls[0].message);
    expect(message).toContain('dueDate="2026-09-11"');
    expect(message).toContain('note="bis Freitag"');
  });

  it("throws a soft error when the create reply doesn't look like a plausible id", async () => {
    const { invoke } = fakeAssistant({ reply_markdown: "Done! I added it." });
    await expect(createReminderTask(invoke, { title: "x", list: "Einkaufen", dueDate: null, notes: null })).rejects.toThrow(/could not be confirmed/);
  });

  it("marks a task complete by id, scoped to the write tool", async () => {
    const { invoke, calls } = fakeAssistant({ reply_markdown: "OK" });
    await completeReminderTask(invoke, "t-2");
    const message = String(calls[0].message);
    expect(message).toContain('action="update"');
    expect(message).toContain('id="t-2"');
    expect(message).toContain("completed=true");
    expect(calls[0].allowedTools).toBe("mcp__apple-reminders__reminders_tasks");
  });

  it("rejects a complete whose turn 'succeeded' but whose reply doesn't actually confirm it — a non-erroring turn isn't the same as a successful write", async () => {
    const { invoke } = fakeAssistant({ reply_markdown: "I couldn't find a reminder with that id." });
    await expect(completeReminderTask(invoke, "t-2")).rejects.toThrow(/didn't confirm the write/);
  });

  it("deletes a task by id", async () => {
    const { invoke, calls } = fakeAssistant({ reply_markdown: "OK" });
    await deleteReminderTask(invoke, "t-3");
    const message = String(calls[0].message);
    expect(message).toContain('action="delete"');
    expect(message).toContain('id="t-3"');
  });

  it("throws the agent's own error text on a failed write", async () => {
    const { invoke } = fakeAssistant({ reply_markdown: "list not found", is_error: true });
    await expect(deleteReminderTask(invoke, "nope")).rejects.toThrow("list not found");
  });
});
