import { describe, expect, it } from "vitest";

import {
  type MaintenanceReport,
  type MaintenanceSettings,
  type StorageView,
  freeableTotal,
  glutLabel,
  ITEM_RULES,
  ruleProblem,
  rulesChanged,
  waitingWorktrees,
} from "./maintenance";

const defaults: MaintenanceSettings = {
  enabled: true,
  runs_days: 90,
  runs_keep: 200,
  routine_runs_days: 30,
  chat_turns_days: 90,
  web_searches_days: 90,
  card_events_days: 90,
  session_spend_days: 365,
  app_log_days: 14,
  backups_keep: 2,
  claude_sessions_days: 30,
};

function report(over: Partial<MaintenanceReport> = {}): MaintenanceReport {
  return {
    dryRun: true,
    home: "/Users/o/.axiomata",
    totalBytes: 1000,
    freeableBytes: 100,
    lastRun: null,
    items: [],
    orphanWorktrees: [],
    error: null,
    ...over,
  };
}

describe("maintenance rules", () => {
  it("accepts the plan's defaults and refuses an age of zero or a fraction", () => {
    expect(ruleProblem(defaults)).toBeNull();
    expect(ruleProblem({ ...defaults, chat_turns_days: 0 })).toContain(
      "chat turns days",
    );
    expect(ruleProblem({ ...defaults, backups_keep: 0 })).toContain(
      "backups keep",
    );
    expect(ruleProblem({ ...defaults, runs_days: 1.5 })).not.toBeNull();
    expect(ruleProblem({ ...defaults, runs_keep: 0 })).toBeNull();
  });

  it("notices a change of any rule, the weekly switch included", () => {
    expect(rulesChanged(defaults, { ...defaults })).toBe(false);
    expect(rulesChanged(defaults, { ...defaults, app_log_days: 21 })).toBe(
      true,
    );
    expect(rulesChanged(defaults, { ...defaults, enabled: false })).toBe(true);
  });

  it("maps every editable rule onto a field that exists", () => {
    for (const rules of Object.values(ITEM_RULES)) {
      for (const rule of rules) expect(rule.field in defaults).toBe(true);
    }
  });
});

describe("storage view", () => {
  it("lists only the orphans that wait for the owner", () => {
    const orphans = [
      {
        path: "/w/p/a-1",
        bytes: 1,
        dirty: true,
        repoRoot: "/r",
        removed: false,
      },
      {
        path: "/w/p/b-2",
        bytes: 1,
        dirty: false,
        repoRoot: "/r",
        removed: false,
      },
      {
        path: "/w/p/c-3",
        bytes: 1,
        dirty: null,
        repoRoot: null,
        removed: false,
      },
      {
        path: "/w/p/d-4",
        bytes: 1,
        dirty: false,
        repoRoot: "/r",
        removed: true,
      },
    ];
    expect(
      waitingWorktrees(report({ orphanWorktrees: orphans })).map((o) => o.path),
    ).toEqual(["/w/p/a-1", "/w/p/c-3"]);
  });

  it("labels Glut's items and shows a name it adds later as it is", () => {
    expect(glutLabel("journals")).toBe("Run journals");
    expect(glutLabel("caches")).toBe("caches");
  });

  it("adds Glut's part only when Glut answered in the known format", () => {
    const base: StorageView = {
      axiomata: report(),
      glut: null,
      settings: defaults,
    };
    expect(freeableTotal(base)).toBe(100);
    expect(freeableTotal({ ...base, glut: { state: "unknown_format" } })).toBe(
      100,
    );
    expect(
      freeableTotal({
        ...base,
        glut: { state: "failed", message: "glut clean: busy" },
      }),
    ).toBe(100);
    const glut = {
      state: "known" as const,
      report: {
        version: 1,
        dryRun: true,
        totalBytes: 500,
        freeableBytes: 50,
        lastRun: null,
        items: [],
      },
    };
    expect(freeableTotal({ ...base, glut })).toBe(150);
  });
});
