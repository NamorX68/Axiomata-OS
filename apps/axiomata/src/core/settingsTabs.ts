/**
 * The Settings dialog's tabs, in order. Their own module so the handbook's coverage test (`core/handbook.test.ts`)
 * reads the same list the dialog draws — a tab added here without a handbook page fails that test.
 */

import { tSettings } from "../i18n/settings";

export type SettingsTabId = "appearance" | "workspace" | "providers" | "storage" | "about";

/** A tab's label is read when drawn, so it follows a language switch (`i18n/settings.ts`). */
export const SETTINGS_TABS: readonly { id: SettingsTabId; label: () => string }[] = [
  { id: "appearance", label: () => tSettings("tabAppearance") },
  { id: "workspace", label: () => tSettings("tabWorkspace") },
  { id: "providers", label: () => tSettings("tabProviders") },
  { id: "storage", label: () => tSettings("tabStorage") },
  { id: "about", label: () => tSettings("tabAbout") },
];
