/**
 * The app's handbook on the frontend (`docs/plans/chat-knows-app.md`). The pages themselves are the build's
 * (`axiomata_core::handbook`, served by name through `handbook_pages` / `handbook_page`); this module holds what the
 * shell needs around them: which page explains which place of the app.
 */

/**
 * Which handbook page explains each place the owner opens: every App Ring entry (registry module or `view:` entry)
 * and every Settings tab. `core/handbook.test.ts` checks this table against the real lists and the pages in
 * `docs/handbook/`, so a new ring entry or tab without a page fails the tests (AGENTS.md: a visible feature change
 * updates its page in the same commit).
 */
export const PAGE_FOR_PLACE: Readonly<Record<string, string>> = {
  // The App Ring's modules (`modules/index.ts`).
  "ring:memory-status": "second-brain",
  "ring:skills-deck": "skills",
  "ring:routines-board": "routinen",
  "ring:todo": "orbit",
  "ring:calendar": "verbindungen",
  "ring:reminders": "verbindungen",
  "ring:mail": "verbindungen",
  "ring:terminal": "terminal",
  // The ring's views (`core/apps.ts` `RING_VIEWS`).
  "ring:view:ide": "studio",
  "ring:view:kanban": "kanban",
  // The Settings tabs (`core/settingsTabs.ts`).
  "settings:appearance": "einstellungen",
  "settings:workspace": "einstellungen",
  "settings:providers": "provider",
  "settings:storage": "storage",
  "settings:about": "index",
};

/** The page that explains a ring entry (`type` as the ring stores it), if the table has one. */
export function pageForRingEntry(type: string): string | undefined {
  return PAGE_FOR_PLACE[`ring:${type}`];
}

/** The page that explains a Settings tab, if the table has one. */
export function pageForSettingsTab(tab: string): string | undefined {
  return PAGE_FOR_PLACE[`settings:${tab}`];
}
