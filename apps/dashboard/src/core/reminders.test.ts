import { describe, expect, it } from "vitest";

import type { RunRecord, RunSummary } from "./backend";
import { defaultList, loadLatestReminderDigest, parseReminderDigest, tasksForList } from "./reminders";

const DIGEST_JSON = JSON.stringify({
  lists: ["Einkaufen", "Arbeit", "Baumarkt"],
  tasks: [
    { title: "Milch kaufen", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
    { title: "Bericht abgeben", list: "Arbeit", notes: "bis Freitag", dueDate: "2026-09-11", priority: "high" },
    { title: "Schrauben besorgen", list: "Baumarkt", notes: null, dueDate: null, priority: "low" },
  ],
});

describe("parseReminderDigest", () => {
  it("parses a well-formed digest", () => {
    const d = parseReminderDigest(DIGEST_JSON);
    expect(d.lists).toEqual(["Einkaufen", "Arbeit", "Baumarkt"]);
    expect(d.tasks).toHaveLength(3);
    expect(d.tasks[1]).toEqual({ title: "Bericht abgeben", list: "Arbeit", notes: "bis Freitag", dueDate: "2026-09-11", priority: "high" });
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
        { title: "ok", list: "Einkaufen", notes: null, dueDate: null, priority: "none" },
        { title: "bad priority", list: "Einkaufen", notes: null, dueDate: null, priority: "urgent!!" },
        { title: "missing list" },
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
