/**
 * The Settings dialog's tabs, in order. Their own module so the handbook's coverage test (`core/handbook.test.ts`) reads
 * the same list the dialog draws — a tab added here without a handbook page fails that test.
 */

export type SettingsTabId = "appearance" | "workspace" | "providers" | "storage" | "about";

export const SETTINGS_TABS: readonly { id: SettingsTabId; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "workspace", label: "Workspace" },
  { id: "providers", label: "AI providers" },
  { id: "storage", label: "Storage" },
  { id: "about", label: "About" },
];
