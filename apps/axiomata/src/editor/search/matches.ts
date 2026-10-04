/**
 * Finding every match of a regular expression (`docs/plans/editor.md`, ED5,
 * T5): line by line, or — for a pattern that names a line break — over the
 * whole joined text. Pure, and the same code on both sides of the worker:
 * the worker runs it first against a fixed version of the text with a time
 * limit (`guard.ts`), the main thread only once that came back in time.
 */

/** Every match on one line, as `[start, end)` columns. Empty matches count once per position. */
export function lineMatches(line: string, re: RegExp, limit = Infinity): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  re.lastIndex = 0;
  while (out.length < limit) {
    const m = re.exec(line);
    if (!m) break;
    out.push([m.index, m.index + m[0].length]);
    // An empty match must not match at the same place forever.
    if (m[0].length === 0) re.lastIndex = m.index + (line.codePointAt(m.index)! > 0xffff ? 2 : 1);
    if (re.lastIndex > line.length) break;
  }
  return out;
}

/**
 * Whether `re` is searched over the joined text rather than line by line: it
 * names a line break (`\n`, T5). Anything else — `\s` included — never sees
 * one, the way Vim's `/` does not. Deliberately generous: an escaped
 * backslash before an `n` (`\\n`, a literal backslash and n) counts too — it
 * only means that pattern is searched over the joined text, where it finds
 * the same matches.
 */
export function spansLines(re: RegExp): boolean {
  return /\\n|\n/.test(re.source);
}

/**
 * Matches found: the first ones as `[start, end)` UTF-16 offsets into the
 * joined text, how many there are in all, and whether that is more than
 * `offsets` holds.
 */
export interface AllMatches {
  offsets: Int32Array;
  count: number;
  truncated: boolean;
}

/**
 * Every match of `re` in `text` (lines joined with `\n`): over the whole text
 * if the pattern {@link spansLines}, else line by line so a match never
 * reaches across a break. Only the first `limit` are kept, but the scan always
 * runs to the end — so a verdict proves the *whole* text finishes in time,
 * however many matches it has (the guard's promise, `guard.ts`).
 */
export function allMatches(text: string, re: RegExp, limit: number): AllMatches {
  const found: number[] = [];
  let count = 0;
  const add = (start: number, end: number): void => {
    if (count < limit) found.push(start, end);
    count++;
  };
  if (spansLines(re)) {
    for (const [s, e] of lineMatches(text, re)) add(s, e);
  } else {
    let lineStart = 0;
    while (lineStart <= text.length) {
      const nl = text.indexOf("\n", lineStart);
      const lineEnd = nl === -1 ? text.length : nl;
      for (const [s, e] of lineMatches(text.slice(lineStart, lineEnd), re)) add(lineStart + s, lineStart + e);
      if (nl === -1) break;
      lineStart = nl + 1;
    }
  }
  return { offsets: Int32Array.from(found), count, truncated: count > limit };
}
