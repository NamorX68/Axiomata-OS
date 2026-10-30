/**
 * The one-time agents migration, frontend side (`docs/plans/agents-model.md`, AM6): what it did and its backup
 * folders. Mirrors `axiomata_core::agents_migration`.
 */

import { invokeBackend as invoke } from "../core/backend";

/** A variant a pass before 2026-10-30 kept as an own role; folded at the next start. */
export interface KeptVariant {
  name: string;
  base: string;
}

export interface MigrationReport {
  dry_run: boolean;
  backup: string | null;
  created: string[];
  moved: string[];
  kept: KeptVariant[];
  stripped: string[];
  engines_mapped: Record<string, string>;
  models_switched_on: string[];
  defaults_seeded: Record<string, string>;
  sessions_remapped: number;
  /** What the pass left alone and why (a file it could not read, a role or engine changed while it waited). */
  skipped: string[];
}

export interface MigrationView {
  report: MigrationReport | null;
  notice_seen: boolean;
  /** Why the migration does not run: a state file this build cannot read, kept as it is. */
  problem: string | null;
}

export interface AgentsBackup {
  path: string;
  bytes: number;
}

export const agentsMigrationState = (): Promise<MigrationView> => invoke<MigrationView>("agents_migration_state");
export const dismissMigrationNotice = (): Promise<void> => invoke<void>("dismiss_agents_migration_notice");
export const listAgentsBackups = (): Promise<AgentsBackup[]> => invoke<AgentsBackup[]>("list_agents_backups");
export const deleteAgentsBackup = (path: string): Promise<void> => invoke<void>("delete_agents_backup", { path });

/** The one line the result notice says: what moved, what was added, what follows along. */
export function migrationSummary(report: MigrationReport): string {
  const parts: string[] = [];
  const moved = report.moved.length;
  if (moved > 0) parts.push(`${moved} copied role${moved === 1 ? "" : "s"} moved`);
  if (report.created.length > 0) parts.push(`${report.created.join(", ")} added`);
  const engines = Object.keys(report.engines_mapped).length;
  if (engines > 0) parts.push(`${engines} engine${engines === 1 ? "" : "s"} now automatic`);
  if (report.sessions_remapped > 0) parts.push(`${report.sessions_remapped} sessions moved along`);
  return parts.length > 0 ? `Roles and engines tidied: ${parts.join(", ")}.` : "Roles and engines were already tidy.";
}
