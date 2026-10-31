/**
 * The question the delete dialog asks before an agent goes (`docs/plans/maintenance.md`, A3c): deleting an agent
 * takes its worktree and branch with it, so unsaved work there is named before the owner says yes — and only that
 * yes lets the backend throw it away.
 */

import { tAgents } from "../i18n/agents";

/** The plan's own words for the second question (A3c), in the current language. */
export const unsavedWorkQuestion = (): string => tAgents("unsavedWorkQuestion");

/** The confirm text: `question`, and the unsaved-work question below it when the worktree holds such work. */
export function removalQuestion(question: string, unsaved: boolean): string {
  return unsaved ? `${question}\n\n${unsavedWorkQuestion()}` : question;
}
