/**
 * The last search, shared by every find bar and by Vi (`docs/plans/editor.md`,
 * ED5, T15): a search used in one bar is what the next bar opens with — in any
 * surface — and what Vi's `n`, `N` and `/`'s history continue with; a search
 * made with Vi's `/` is what the next bar opens with.
 *
 * Both speak the same pattern language (`editor/search/find.ts`), so the bar
 * writes its query to Vi as a pattern with the case spelled out, and takes a
 * pattern Vi searched for since as a regular expression.
 */

import { fromViPattern, viPatternFor, type FindOptions } from "../editor/search/find";
import { viShared, viStateChanged } from "./viShared";

let last: { query: string; options: FindOptions } | null = null;
/** The pattern the bar last handed to Vi: a different one there means Vi searched since. */
let handedToVi: string | null = null;

/** A bar used `query`: it is the last search now, for the bars and for Vi. */
export function rememberFind(query: string, options: FindOptions): void {
  last = { query, options: { ...options } };
  const pattern = viPatternFor(query, options);
  const memory = viShared().search;
  memory.last = { pattern, backward: false };
  memory.highlight = true;
  memory.remember("search", pattern);
  handedToVi = pattern;
  viStateChanged();
}

/** What a bar opens with: Vi's last search if it came after the bars', else the bars' own. */
export function lastFind(): { query: string; options: Partial<FindOptions> } | null {
  const viPattern = viShared().search.last?.pattern;
  if (viPattern && viPattern !== handedToVi) return fromViPattern(viPattern);
  return last;
}
