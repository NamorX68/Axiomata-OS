/**
 * One colour per language (`docs/plans/editor-look.md`, K14): the file app's and
 * the IDE's tabs are tinted with it, the file tree's icons (LK4) use the same
 * table. Each colour is a token (`--ax-lang-<id>`, `themes/tokens.css`), so a
 * theme — or the owner's own `theme.css` — can change it.
 */

import { detectLanguage } from "../editor/syntax/languages";

/** The languages with a colour of their own; everything else is `other`. */
export const LANGUAGE_COLOR_IDS = [
  "rust",
  "javascript",
  "typescript",
  "json",
  "css",
  "html",
  "python",
  "bash",
  "markdown",
  "toml",
  "yaml",
  "lua",
  "svelte",
  "swift",
  "sql",
  "other",
] as const;

export type LanguageColorId = (typeof LANGUAGE_COLOR_IDS)[number];

/** The colour group of a file, by its name (and first line, for a shebang). */
export function languageColorId(fileName: string, firstLine = ""): LanguageColorId {
  const language = detectLanguage(fileName, firstLine);
  // TSX is TypeScript to the eye, as JSX is JavaScript.
  const id = language === "tsx" ? "typescript" : language;
  return (LANGUAGE_COLOR_IDS as readonly string[]).includes(id ?? "") ? (id as LanguageColorId) : "other";
}

/** The CSS colour for a file: the token of its language. */
export function languageColor(fileName: string): string {
  return `var(--ax-lang-${languageColorId(fileName)})`;
}
