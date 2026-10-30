import { describe, expect, it } from "vitest";

import { migrationSummary, type MigrationReport } from "./agentsMigration";

const report = (over: Partial<MigrationReport> = {}): MigrationReport => ({
  dry_run: false,
  backup: "/h/.axiomata/agents-backup-2026-10-30",
  created: [],
  moved: [],
  kept: [],
  stripped: [],
  engines_mapped: {},
  models_switched_on: [],
  defaults_seeded: {},
  sessions_remapped: 0,
  skipped: [],
  ...over,
});

describe("the migration's notice", () => {
  it("says what moved, what was kept and what is automatic now", () => {
    const text = migrationSummary(
      report({
        moved: ["a-x", "b-y"],
        kept: [{ name: "c-z", base: "c" }],
        engines_mapped: { old: "new" },
        created: ["implementer"],
      }),
    );
    expect(text).toBe(
      "Roles and engines tidied: 2 copied roles moved, implementer added, 1 engine now automatic.",
    );
    expect(migrationSummary(report())).toContain("already tidy");
  });
});
