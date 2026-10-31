/**
 * The app's handbook on the frontend (`docs/plans/chat-knows-app.md`). The pages themselves are the build's
 * (`axiomata_core::handbook`, served by name through `handbook_pages` / `handbook_page`); this module holds what the
 * shell needs around them: which page explains which place of the app, how a link between pages is read, and the way
 * the Help panel is opened (`shell:handbook`).
 */

import { lang } from "../i18n/lang.svelte";
import { invokeBackend } from "./backend";
import { emit } from "./bus";

/** A page as the table of contents lists it. */
export interface HandbookEntry {
  name: string;
  title: string;
  summary: string;
}

/** A page with its Markdown. */
export interface HandbookPage {
  name: string;
  title: string;
  text: string;
}

/** The shell-bus event that opens the Help panel, optionally on `{ page }`. */
export const HANDBOOK_EVENT = "shell:handbook";

/** Every page, in the handbook's order. */
export function listHandbook(): Promise<HandbookEntry[]> {
  return invokeBackend<HandbookEntry[]>("handbook_pages", { language: lang() });
}

/** One page by its name. */
export function readHandbookPage(name: string): Promise<HandbookPage> {
  return invokeBackend<HandbookPage>("handbook_page", { name, language: lang() });
}

/** Opens the Help panel, on `page` when given (else on the page last shown, or the overview). */
export function openHandbook(page?: string): void {
  emit(HANDBOOK_EVENT, page ? { page } : {});
}

/**
 * The page a link in a page points to: `studio.md`, `./studio.md` or `studio.md#section`, when `known` has that name.
 * Anything else — a web address, a path into a folder, a page the build does not have — is no page link.
 */
export function pageOfLink(href: string, known: ReadonlySet<string>): string | undefined {
  const match = /^(?:\.\/)?([a-z0-9-]+)\.md(?:#.*)?$/.exec(href.trim());
  return match && known.has(match[1]) ? match[1] : undefined;
}

/**
 * A page's Markdown with its bare page links written `./name.md`: the shared sanitiser (`core/markdown.ts`) keeps only
 * hrefs that start with a scheme, `#`, `/` or `.`, so `[Studio](studio.md)` would lose its target.
 */
export function withPageLinks(markdown: string): string {
  return markdown.replace(/\]\(([a-z0-9-]+\.md(?:#[^)\s]*)?)\)/g, "](./$1)");
}

/**
 * The page `/help <topic>` means: a page's name, else the one whose title is the topic, else the only one whose
 * name or title starts with it. `undefined` when none or several fit — the caller lists the pages then.
 */
export function pageForTopic(topic: string, entries: readonly HandbookEntry[]): string | undefined {
  const wanted = topic.trim().toLowerCase();
  if (!wanted) return undefined;
  const exact = entries.find((entry) => entry.name === wanted || entry.title.toLowerCase() === wanted);
  if (exact) return exact.name;
  const starting = entries.filter(
    (entry) => entry.name.startsWith(wanted) || entry.title.toLowerCase().startsWith(wanted),
  );
  return starting.length === 1 ? starting[0].name : undefined;
}

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
