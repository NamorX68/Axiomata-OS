/**
 * The find bar's language (`docs/plans/editor.md`, ED5, T4, T15): what a query
 * and its three options (regular expression, match case, whole word) mean as a
 * pattern, and what a replacement template turns a match into.
 *
 * The bar speaks the same pattern language as Vi's `/` (`vi/search.ts`): a
 * JavaScript regular expression plus `\<` `\>` `\c` `\C`. So the last search
 * can go back and forth between them unchanged (T15), and a literal query is
 * simply one with every special character escaped. Pure, no DOM.
 */

import { compilePattern, type Compiled } from "../vi/search";

export interface FindOptions {
  /** The query is a regular expression; otherwise it is literal text. */
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  /** Replacing keeps the case shape of what it replaces (`foo`→`bar`, `Foo`→`Bar`, `FOO`→`BAR`). */
  preserveCase: boolean;
}

export const DEFAULT_FIND_OPTIONS: FindOptions = {
  regex: false,
  caseSensitive: false,
  wholeWord: false,
  preserveCase: false,
};

/** `text` with every character a pattern gives a meaning escaped. */
export function escapePattern(text: string): string {
  return text.replace(/[\\^$.*+?()[\]{}|/]/g, "\\$&");
}

/** The query as a pattern in Vi's language, case left to whoever compiles it. */
export function findPattern(query: string, options: FindOptions): string {
  const body = options.regex ? query : escapePattern(query);
  return options.wholeWord ? `\\<(?:${body})\\>` : body;
}

/** The query compiled, or the reason it cannot be; `null` for an empty query. */
export function compileFind(query: string, options: FindOptions): Compiled | { error: string } | null {
  if (!query) return null;
  return compilePattern(findPattern(query, options), options.caseSensitive ? "match" : "ignore");
}

/**
 * The same search as Vi writes it (for `n`, `N` and `/`'s history): the
 * pattern with the case spelled out, since Vi would otherwise apply smartcase.
 */
export function viPatternFor(query: string, options: FindOptions): string {
  return findPattern(query, options) + (options.caseSensitive ? "\\C" : "\\c");
}

/**
 * A search Vi made, as the bar shows it: a regular expression whose case
 * follows Vi's smartcase (a capital anywhere matches case). `\c`/`\C` inside
 * it keep working, since the bar compiles the same language.
 */
export function fromViPattern(pattern: string): { query: string; options: Partial<FindOptions> } {
  return { query: pattern, options: { regex: true, wholeWord: false, caseSensitive: /\p{Lu}/u.test(pattern) } };
}

/**
 * What a replacement template becomes for one match. In a regular expression
 * `$&` (or `$0`) is the match, `$1`–`$99` its groups (two digits only while that many
 * groups exist), `$<name>` a named group, `$$` a dollar, and `\n` `\t` `\\`
 * a line break, a tab and a backslash. Literal text is taken as it is.
 */
export function expandReplacement(template: string, match: RegExpExecArray, regex: boolean): string {
  if (!regex) return template;
  let out = "";
  for (let i = 0; i < template.length; i++) {
    const ch = template[i];
    const next = template[i + 1];
    if (ch === "\\" && next !== undefined) {
      out += next === "n" ? "\n" : next === "t" ? "\t" : next;
      i++;
    } else if (ch === "$" && next === "$") {
      out += "$";
      i++;
    } else if (ch === "$" && next === "&") {
      out += match[0];
      i++;
    } else if (ch === "$" && next === "<") {
      const close = template.indexOf(">", i + 2);
      const name = close === -1 ? "" : template.slice(i + 2, close);
      if (close === -1 || !match.groups || !(name in match.groups)) {
        out += ch;
        continue;
      }
      out += match.groups[name] ?? "";
      i = close;
    } else if (ch === "$" && next !== undefined && /[0-9]/.test(next)) {
      const two = template.slice(i + 1, i + 3);
      const digits = /^[1-9][0-9]$/.test(two) && Number(two) < match.length ? two : next;
      const index = Number(digits);
      if (index >= match.length) {
        out += ch;
        continue;
      }
      out += match[index] ?? "";
      i += digits.length;
    } else {
      out += ch;
    }
  }
  return out;
}

/**
 * `replacement` given the case shape of `matched`: all capitals, all small
 * letters, or a capital followed by small ones. Anything else (mixed case, no
 * letters at all) leaves the replacement as it was typed.
 */
export function preserveCase(matched: string, replacement: string): string {
  if (!/\p{L}/u.test(matched)) return replacement;
  if (matched === matched.toUpperCase() && matched !== matched.toLowerCase()) return replacement.toUpperCase();
  if (matched === matched.toLowerCase()) return replacement.toLowerCase();
  const first = matched.slice(0, 1);
  const rest = matched.slice(1);
  if (first === first.toUpperCase() && rest === rest.toLowerCase()) {
    return replacement.slice(0, 1).toUpperCase() + replacement.slice(1).toLowerCase();
  }
  return replacement;
}
