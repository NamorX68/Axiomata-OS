/**
 * The languages the editor highlights (`docs/plans/editor.md`, G2, G10–G12):
 * which grammar each uses, which query files make up its highlighting, and how
 * a file — or a Markdown code fence — names it.
 *
 * Query files come from the grammars' own repositories (G12). Some grammars are
 * written as additions to another one: TypeScript's queries only add to
 * JavaScript's, and Svelte's start with `; inherits: html` — an nvim-treesitter
 * convention `web-tree-sitter` does not follow. So a language lists every file
 * it needs, base first; `*.local.scm` files are our own additions (G12).
 */

export interface LanguageSpec {
  id: string;
  /** Grammar file under `public/grammars/` (without `.wasm`). */
  grammar: string;
  /** Highlight query files, in order, relative to `public/grammars/`. */
  highlights: string[];
  /** Injection query file, if the language embeds others (G10). */
  injections?: string;
}

export const LANGUAGES: readonly LanguageSpec[] = [
  { id: "rust", grammar: "rust", highlights: ["rust/highlights.scm"], injections: "rust/injections.scm" },
  {
    id: "javascript",
    grammar: "javascript",
    highlights: ["javascript/highlights.scm", "javascript/highlights-jsx.scm"],
  },
  {
    id: "typescript",
    grammar: "typescript",
    highlights: ["javascript/highlights.scm", "typescript/highlights.scm"],
  },
  {
    id: "tsx",
    grammar: "tsx",
    highlights: ["javascript/highlights.scm", "javascript/highlights-jsx.scm", "typescript/highlights.scm"],
  },
  { id: "json", grammar: "json", highlights: ["json/highlights.scm"] },
  { id: "css", grammar: "css", highlights: ["css/highlights.scm"] },
  { id: "html", grammar: "html", highlights: ["html/highlights.scm"], injections: "html/injections.scm" },
  { id: "python", grammar: "python", highlights: ["python/highlights.scm"] },
  { id: "bash", grammar: "bash", highlights: ["bash/highlights.scm"] },
  {
    id: "markdown",
    grammar: "markdown",
    highlights: ["markdown/highlights.scm"],
    injections: "markdown/injections.scm",
  },
  {
    id: "markdown_inline",
    grammar: "markdown_inline",
    highlights: ["markdown_inline/highlights.scm"],
    injections: "markdown_inline/injections.scm",
  },
  { id: "toml", grammar: "toml", highlights: ["toml/highlights.scm"] },
  { id: "yaml", grammar: "yaml", highlights: ["yaml/highlights.scm"] },
  { id: "lua", grammar: "lua", highlights: ["lua/highlights.scm"] },
  {
    id: "svelte",
    grammar: "svelte",
    highlights: ["html/highlights.scm", "svelte/highlights.scm"],
    injections: "svelte/injections.scm",
  },
  { id: "swift", grammar: "swift", highlights: ["swift/highlights.scm"] },
  { id: "sql", grammar: "sql", highlights: ["sql/highlights.scm"] },
];

const BY_ID = new Map(LANGUAGES.map((l) => [l.id, l]));

export function languageSpec(id: string): LanguageSpec | undefined {
  return BY_ID.get(id);
}

const EXTENSIONS: Record<string, string> = {
  rs: "rust",
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  jsx: "javascript",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "tsx",
  json: "json",
  jsonc: "json",
  css: "css",
  html: "html",
  htm: "html",
  py: "python",
  pyi: "python",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  md: "markdown",
  markdown: "markdown",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
  lua: "lua",
  svelte: "svelte",
  swift: "swift",
  sql: "sql",
};

/** Whole file names that say more than their extension. */
const FILE_NAMES: Record<string, string> = {
  "Cargo.lock": "toml",
  ".zshrc": "bash",
  ".zprofile": "bash",
  ".bashrc": "bash",
  ".bash_profile": "bash",
  ".profile": "bash",
};

/** Interpreters named on a shebang line. */
const SHEBANGS: Record<string, string> = {
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  python: "python",
  python3: "python",
  node: "javascript",
  lua: "lua",
};

/**
 * The language of a file: its whole name, then its extension, then — for a
 * file without one — the interpreter on a `#!` first line (G11). `null` means
 * plain text.
 */
export function detectLanguage(fileName: string, firstLine = ""): string | null {
  const base = fileName.split("/").pop() ?? fileName;
  if (FILE_NAMES[base]) return FILE_NAMES[base];
  const dot = base.lastIndexOf(".");
  if (dot > 0) {
    const byExt = EXTENSIONS[base.slice(dot + 1).toLowerCase()];
    if (byExt) return byExt;
  }
  const shebang = /^#!\s*(\S+)(?:\s+(\S+))?/.exec(firstLine);
  if (shebang) {
    const program = shebang[1].endsWith("/env") ? shebang[2] : shebang[1];
    return SHEBANGS[(program ?? "").split("/").pop() ?? ""] ?? null;
  }
  return null;
}

/** Names a code fence or an injection uses, beyond the ids themselves. */
const ALIASES: Record<string, string> = {
  rs: "rust",
  js: "javascript",
  jsx: "javascript",
  ts: "typescript",
  py: "python",
  sh: "bash",
  shell: "bash",
  zsh: "bash",
  console: "bash",
  md: "markdown",
  yml: "yaml",
  jsonc: "json",
  scss: "css",
};

/** The language a fence info string or injection names, or `null` if not bundled. */
export function languageForName(name: string): string | null {
  const key = name.trim().toLowerCase();
  const id = ALIASES[key] ?? key;
  return BY_ID.has(id) ? id : null;
}
