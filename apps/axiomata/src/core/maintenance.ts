/**
 * Settings → Storage (`docs/plans/maintenance.md`, MT4): what Axiomata keeps of its own data, for how long, and what
 * a clean-up frees. The shapes mirror `axiomata_core::maintenance` (camelCase) and `config::MaintenanceSettings`
 * (snake_case, as `config.toml` spells it).
 */

import { tStorage } from "../i18n/storage";

/** The retention rules of `[maintenance]`. */
export interface MaintenanceSettings {
  enabled: boolean;
  runs_days: number;
  runs_keep: number;
  routine_runs_days: number;
  chat_turns_days: number;
  web_searches_days: number;
  card_events_days: number;
  session_spend_days: number;
  app_log_days: number;
  backups_keep: number;
  claude_sessions_days: number;
}

/** One line of the report. */
export interface MaintenanceItem {
  key: string;
  label: string;
  unit: string;
  bytes: number | null;
  count: number;
  freeableBytes: number | null;
  freeableCount: number;
  keep: string;
  error: string | null;
}

/** A worktree whose agent is gone; `dirty` null = git cannot say. */
export interface OrphanWorktree {
  path: string;
  bytes: number;
  dirty: boolean | null;
  repoRoot: string | null;
  removed: boolean;
}

/** An ended session without work of its own (agents-model.md, Q17): a clean-up removes it with its worktree. */
export interface EndedSession {
  id: number;
  name: string;
  projectId: number;
  removed: boolean;
}

export interface MaintenanceReport {
  dryRun: boolean;
  home: string;
  totalBytes: number;
  freeableBytes: number;
  lastRun: string | null;
  items: MaintenanceItem[];
  orphanWorktrees: OrphanWorktree[];
  /** Ended sessions without work: found (a report) or removed (a clean-up). */
  endedSessions: EndedSession[];
  error: string | null;
}

/** Glut's own report (`glut clean --dry-run --json`, format version 1). */
export interface GlutReport {
  version: number;
  dryRun: boolean;
  totalBytes: number;
  freeableBytes: number;
  lastRun: string | null;
  /** Fixed names (`snapshots`, `journals`, …); `keep` (`7d`) is absent for an item not kept by age. */
  items: {
    name: string;
    bytes: number;
    freeableBytes: number;
    count: number;
    freeableCount: number;
    keep: string | null;
  }[];
}

/**
 * Glut's row: known, an answer in another format, or Glut's own failure line (`glut clean: …`). No Glut that can
 * answer → no row at all (`null`).
 */
export type GlutRow =
  | { state: "known"; report: GlutReport }
  | { state: "unknown_format" }
  | { state: "failed"; message: string };

/** What "Clean up now" did: the bytes freed (Axiomata's and Glut's) and the state after it. */
export interface CleanOutcome {
  freed: number;
  /** Why Glut did not clean although there is a Glut (its failure line, another format). */
  glut_note: string | null;
  view: StorageView;
}

/** Glut names its items by fixed keys; the labels are Axiomata's. A name added later is shown as Glut spells it. */
const GLUT_LABELS: Readonly<Record<string, Parameters<typeof tStorage>[0]>> = {
  snapshots: "glutSnapshots",
  journals: "glutJournals",
  sessions: "glutSessions",
  results: "glutResults",
  images: "glutImages",
  exports: "glutExports",
  lean: "glutLean",
  other: "glutOther",
};

/** The label of one of Glut's items. */
export function glutLabel(name: string): string {
  const key = GLUT_LABELS[name];
  return key ? tStorage(key) : name;
}

/** The report's rows by their fixed keys; the backend's English label stands for a key added later. */
const ITEM_LABELS: Readonly<Record<string, Parameters<typeof tStorage>[0]>> = {
  runs: "itemRuns",
  routine_runs: "itemRoutineRuns",
  chat_turns: "itemChatTurns",
  web_searches: "itemWebSearches",
  card_events: "itemCardEvents",
  session_spend: "itemSessionSpend",
  database: "itemDatabase",
  ended_sessions: "itemEndedSessions",
  app_log: "itemAppLog",
  runs_log: "itemRunsLog",
  backups: "itemBackups",
  agent_events: "itemAgentEvents",
  claude_sessions: "itemClaudeSessions",
  worktrees: "itemWorktrees",
};

/** The label of a report row, in the app's language. */
export function itemLabel(key: string, fallback: string): string {
  const label = ITEM_LABELS[key];
  return label ? tStorage(label) : fallback;
}

const UNITS: Readonly<Record<string, Parameters<typeof tStorage>[0]>> = {
  rows: "unitRows",
  files: "unitFiles",
  sessions: "unitSessions",
  folders: "unitFolders",
  entries: "unitEntries",
  worktrees: "unitWorktrees",
};

/** `count` of a row's unit ("12 rows"); a unit added later is shown as the backend spells it. */
export function unitCount(count: number, unit: string): string {
  const key = UNITS[unit];
  return key ? tStorage(key, { n: count }) : `${count} ${unit}`;
}

/**
 * How long a row without an editable rule keeps its data. The backend words it in English from its own constants;
 * the numbers are read back out of that line, so a changed constant still shows right. A line of another shape is
 * shown as the backend wrote it.
 */
export function keepText(item: Pick<MaintenanceItem, "key" | "keep">): string {
  const number = (pattern: RegExp): number[] | null => {
    const match = pattern.exec(item.keep);
    return match ? match.slice(1).map(Number) : null;
  };
  switch (item.key) {
    case "database": {
      const found = number(/^compacted when over (\d+) % is free$/);
      return found ? tStorage("keepDatabase", { percent: found[0] }) : item.keep;
    }
    case "runs_log": {
      const found = number(/^rotated at (\d+) MB, the last (\d+) kept$/);
      return found ? tStorage("keepRunsLog", { mb: found[0], n: found[1] }) : item.keep;
    }
    case "ended_sessions": {
      const found = number(/quiet for (\d+) day\(s\)/);
      return found ? tStorage("keepEndedSessions", { n: found[0] }) : item.keep;
    }
    case "agent_events":
      return item.keep === "while their agent exists" ? tStorage("keepAgentEvents") : item.keep;
    case "worktrees":
      return item.keep === "removed with their agent; one with changes waits for you"
        ? tStorage("keepWorktrees")
        : item.keep;
    default:
      return item.keep;
  }
}

/** What the Storage section shows. */
export interface StorageView {
  axiomata: MaintenanceReport;
  glut: GlutRow | null;
  settings: MaintenanceSettings;
}

/** The numeric rules a field of the section edits. */
export type RuleField = Exclude<keyof MaintenanceSettings, "enabled">;

/** The editable rules of each report line, in the order shown: an age in days, and a count where one applies. */
export const ITEM_RULES: Readonly<
  Record<string, { field: RuleField; unit: "days" | "kept" }[]>
> = {
  runs: [
    { field: "runs_days", unit: "days" },
    { field: "runs_keep", unit: "kept" },
  ],
  routine_runs: [{ field: "routine_runs_days", unit: "days" }],
  chat_turns: [{ field: "chat_turns_days", unit: "days" }],
  web_searches: [{ field: "web_searches_days", unit: "days" }],
  card_events: [{ field: "card_events_days", unit: "days" }],
  session_spend: [{ field: "session_spend_days", unit: "days" }],
  app_log: [{ field: "app_log_days", unit: "days" }],
  backups: [{ field: "backups_keep", unit: "kept" }],
  claude_sessions: [{ field: "claude_sessions_days", unit: "days" }],
};

/** The bounds the backend checks too (`MaintenanceSettings::validate`); the fields refuse to go beyond them. */
export const RULE_BOUNDS: Readonly<
  Record<RuleField, { min: number; max: number }>
> = {
  runs_days: { min: 1, max: 3650 },
  runs_keep: { min: 0, max: 100_000 },
  routine_runs_days: { min: 1, max: 3650 },
  chat_turns_days: { min: 1, max: 3650 },
  web_searches_days: { min: 1, max: 3650 },
  card_events_days: { min: 1, max: 3650 },
  session_spend_days: { min: 1, max: 3650 },
  app_log_days: { min: 1, max: 3650 },
  backups_keep: { min: 1, max: 100 },
  claude_sessions_days: { min: 1, max: 3650 },
};

/** A rule as the section shows it: its row's label and what the number counts ("Chat turns (days)"). */
function ruleName(field: RuleField): string {
  for (const [key, rules] of Object.entries(ITEM_RULES)) {
    const rule = rules.find((r) => r.field === field);
    if (rule) {
      const label = itemLabel(key, key);
      return rule.unit === "days" ? tStorage("ruleDays", { label }) : tStorage("ruleKept", { label });
    }
  }
  return field;
}

/** The first rule out of its bounds, in words, or `null` when all are fine. */
export function ruleProblem(settings: MaintenanceSettings): string | null {
  for (const [field, { min, max }] of Object.entries(RULE_BOUNDS) as [
    RuleField,
    { min: number; max: number },
  ][]) {
    const value = settings[field];
    if (!Number.isInteger(value) || value < min || value > max) {
      return tStorage("ruleRange", { rule: ruleName(field), min, max });
    }
  }
  return null;
}

/** Whether two rule sets differ — the Save button is offered only then. */
export function rulesChanged(
  a: MaintenanceSettings,
  b: MaintenanceSettings,
): boolean {
  return (Object.keys(a) as (keyof MaintenanceSettings)[]).some(
    (key) => a[key] !== b[key],
  );
}

/** The orphaned worktrees still waiting for the owner: with changes, or ones git cannot judge. */
export function waitingWorktrees(report: MaintenanceReport): OrphanWorktree[] {
  return report.orphanWorktrees.filter(
    (orphan) => !orphan.removed && orphan.dirty !== false,
  );
}

/** What one click on "Clean up now" frees, Axiomata's part and Glut's together. */
export function freeableTotal(view: StorageView): number {
  const glut =
    view.glut?.state === "known" ? view.glut.report.freeableBytes : 0;
  return view.axiomata.freeableBytes + glut;
}
