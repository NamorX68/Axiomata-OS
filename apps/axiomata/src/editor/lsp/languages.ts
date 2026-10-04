/**
 * The protocol's language ids for the editor's languages (`textDocument/didOpen`
 * names one). Most are the same word; the ones that are not are VS Code's
 * spellings, which every server expects.
 */
const LANGUAGE_IDS: Record<string, string> = {
  rust: "rust",
  python: "python",
  typescript: "typescript",
  tsx: "typescriptreact",
  javascript: "javascript",
  svelte: "svelte",
  css: "css",
  html: "html",
  json: "json",
  bash: "shellscript",
  lua: "lua",
  swift: "swift",
  toml: "toml",
  yaml: "yaml",
  markdown: "markdown",
  sql: "sql",
};

/** The protocol's id for an editor language, or `null` when no server speaks it. */
export function lspLanguageId(language: string | null): string | null {
  return language ? (LANGUAGE_IDS[language] ?? null) : null;
}
