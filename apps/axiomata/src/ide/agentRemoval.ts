/**
 * The question the delete dialog asks before an agent goes (`docs/plans/maintenance.md`, A3c): deleting an agent
 * takes its worktree and branch with it, so unsaved work there is named before the owner says yes — and only that
 * yes lets the backend throw it away.
 */

/** The plan's own words for the second question (A3c). */
export const UNSAVED_WORK_QUESTION = "Unsaved work in the worktree — delete anyway?";

/** The confirm text: `question`, and the unsaved-work question below it when the worktree holds such work. */
export function removalQuestion(question: string, unsaved: boolean): string {
  return unsaved ? `${question}\n\n${UNSAVED_WORK_QUESTION}` : question;
}
