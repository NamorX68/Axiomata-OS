/**
 * The one-time agents migration, frontend side (`docs/plans/agents-model.md`, AM6): what it did, the kept roles that
 * wait for the owner's decision, and its backup folders. Mirrors `axiomata_core::agents_migration`.
 */

import { invokeBackend as invoke } from "../core/backend";
import type { Role } from "../core/roster";

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
}

/** A kept role beside its base, with the fields that differ. */
export interface VariantDiff {
  variant: Role;
  base: Role;
  differs: ("instructions" | "permissions" | "limits" | "kind" | "creates")[];
}

export interface MigrationView {
  report: MigrationReport | null;
  notice_seen: boolean;
  pending: VariantDiff[];
}

export interface AgentsBackup {
  path: string;
  bytes: number;
}

export const agentsMigrationState = (): Promise<MigrationView> => invoke<MigrationView>("agents_migration_state");
export const keepRoleVariant = (name: string): Promise<void> => invoke<void>("keep_role_variant", { name });
export const foldRoleVariant = (name: string): Promise<void> => invoke<void>("fold_role_variant", { name });
export const dismissMigrationNotice = (): Promise<void> => invoke<void>("dismiss_agents_migration_notice");
export const listAgentsBackups = (): Promise<AgentsBackup[]> => invoke<AgentsBackup[]>("list_agents_backups");
export const deleteAgentsBackup = (path: string): Promise<void> => invoke<void>("delete_agents_backup", { path });

/** The one line the result notice says: what moved, what was kept, where the backup is. */
export function migrationSummary(report: MigrationReport): string {
  const parts: string[] = [];
  const moved = report.moved.length;
  if (moved > 0) parts.push(`${moved} copied role${moved === 1 ? "" : "s"} moved`);
  if (report.kept.length > 0) parts.push(`${report.kept.length} kept as own roles`);
  if (report.created.length > 0) parts.push(`${report.created.join(", ")} added`);
  const engines = Object.keys(report.engines_mapped).length;
  if (engines > 0) parts.push(`${engines} engine${engines === 1 ? "" : "s"} now automatic`);
  if (report.sessions_remapped > 0) parts.push(`${report.sessions_remapped} sessions moved along`);
  return parts.length > 0 ? `Roles and engines tidied: ${parts.join(", ")}.` : "Roles and engines were already tidy.";
}

/** The text of `field` of a role for the side-by-side view. */
export function fieldText(role: Role, field: VariantDiff["differs"][number]): string {
  switch (field) {
    case "instructions":
      return role.instructions;
    case "permissions":
      return role.permissions.join("\n");
    case "limits":
      return JSON.stringify(role.limits);
    case "kind":
      return role.kind;
    case "creates":
      return role.creates.join(", ");
  }
}
