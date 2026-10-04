/**
 * From tree-sitter capture names to the editor's 17 syntax tokens
 * (`docs/plans/editor.md`, G5). Each token is one `--ax-syntax-*` colour every
 * theme defines.
 *
 * Grammars do not agree on names: the official ones say `@string.special`,
 * Markdown's say `@text.title`, older ones `@conditional` or `@field`. A name is
 * tried as a whole, then with its last part dropped (`function.method.call` →
 * `function.method` → `function`), each step against this table; names that
 * mean "no colour" (`@none`, `@spell`, `@embedded`) resolve to `null`.
 */

export const SYNTAX_TOKENS = [
  "keyword",
  "string",
  "number",
  "comment",
  "function",
  "type",
  "variable",
  "constant",
  "property",
  "operator",
  "punctuation",
  "tag",
  "attribute",
  "heading",
  "link",
  "emphasis",
  "code",
] as const;

export type SyntaxToken = (typeof SYNTAX_TOKENS)[number];

// A Map built from pairs, not an object literal: `"constructor" in {}` is true
// through the prototype, which would turn capture names like `@constructor` or
// `@toString` into a function instead of a colour.
const TABLE = new Map<string, SyntaxToken | null>([
  // Synonyms across grammar generations.
  ["conditional", "keyword"],
  ["repeat", "keyword"],
  ["include", "keyword"],
  ["import", "keyword"],
  ["exception", "keyword"],
  ["storageclass", "keyword"],
  ["preproc", "keyword"],
  ["label", "keyword"],
  ["boolean", "constant"],
  ["character", "string"],
  ["escape", "constant"],
  ["float", "number"],
  ["method", "function"],
  ["constructor", "type"],
  ["namespace", "type"],
  ["module", "type"],
  ["field", "property"],
  ["parameter", "variable"],
  ["delimiter", "punctuation"],
  ["variable.member", "property"],
  ["variable.parameter", "variable"],
  ["variable.builtin", "constant"],
  ["string.special.key", "property"],
  ["string.escape", "constant"],
  ["string.regexp", "string"],
  ["comment.documentation", "comment"],
  // Markdown's text.* and markup.* vocabularies.
  ["text.title", "heading"],
  ["markup.heading", "heading"],
  ["text.uri", "link"],
  ["text.reference", "link"],
  ["markup.link", "link"],
  ["text.literal", "code"],
  ["markup.raw", "code"],
  ["text.emphasis", "emphasis"],
  ["text.strong", "emphasis"],
  ["markup.italic", "emphasis"],
  ["markup.strong", "emphasis"],
  // Nothing to colour.
  ["none", null],
  ["spell", null],
  ["nospell", null],
  ["embedded", null],
  ["text", null],
  ["", null],
]);
for (const token of SYNTAX_TOKENS) TABLE.set(token, token);

const cache = new Map<string, SyntaxToken | null>();

/** The token a capture colours with, or `null` for none. */
export function tokenFor(capture: string): SyntaxToken | null {
  const hit = cache.get(capture);
  if (hit !== undefined) return hit;
  let name = capture;
  let token: SyntaxToken | null = null;
  for (;;) {
    if (TABLE.has(name)) {
      token = TABLE.get(name) ?? null;
      break;
    }
    const dot = name.lastIndexOf(".");
    if (dot < 0) break;
    name = name.slice(0, dot);
  }
  cache.set(capture, token);
  return token;
}
