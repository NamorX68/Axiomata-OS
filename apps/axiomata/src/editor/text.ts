/**
 * Characters as a person sees them (`docs/plans/editor.md`, ED1, F3): grapheme
 * and word boundaries within one line, and display columns with tabs expanded.
 *
 * Positions are UTF-16 offsets (`position.ts`), but the cursor must never stop
 * between the two halves of a surrogate pair or between a letter and its
 * combining accent. Graphemes come from `Intl.Segmenter`; words use our own
 * character classes (see `WORD_CHAR` for why not the segmenter's).
 */

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/** The grapheme boundary after `col`, or the line's length at its end. */
export function nextGrapheme(line: string, col: number): number {
  if (col >= line.length) return line.length;
  for (const seg of graphemes.segment(line)) {
    const end = seg.index + seg.segment.length;
    if (end > col) return end;
  }
  return line.length;
}

/** The grapheme boundary before `col`, or 0 at the line's start. */
export function prevGrapheme(line: string, col: number): number {
  if (col <= 0) return 0;
  let prev = 0;
  for (const seg of graphemes.segment(line)) {
    if (seg.index >= col) break;
    prev = seg.index;
  }
  return prev;
}

/** How many graphemes precede `col` — what a person would call the column. */
export function graphemeCount(text: string): number {
  let n = 0;
  for (const _ of graphemes.segment(text)) n++;
  return n;
}

/**
 * Word characters for ⌥←/→, ⌥⌫ and double-click: letters, digits, combining
 * marks and `_`. Deliberately not `Intl.Segmenter`'s word granularity, which
 * follows prose rules (UAX #29) and treats `bar.baz` or `3.14` as one word —
 * wrong in code, where ⌥→ should stop at the dot.
 */
const WORD_CHAR = /[\p{L}\p{N}\p{M}_]/u;

type CharClass = "word" | "space" | "other";

function classAt(line: string, col: number): CharClass {
  const ch = String.fromCodePoint(line.codePointAt(col) ?? 0);
  if (WORD_CHAR.test(ch)) return "word";
  return ch === " " || ch === "\t" ? "space" : "other";
}

/** The start of the code point before `col` (steps over a surrogate pair). */
function prevCodePoint(line: string, col: number): number {
  const low = line.charCodeAt(col - 1);
  return col >= 2 && low >= 0xdc00 && low <= 0xdfff ? col - 2 : col - 1;
}

function nextCodePoint(line: string, col: number): number {
  const cp = line.codePointAt(col) ?? 0;
  return col + (cp > 0xffff ? 2 : 1);
}

/** The end of the next word after `col` (⌥→): skips non-word characters, then the word. */
export function nextWordEnd(line: string, col: number): number {
  let i = col;
  while (i < line.length && classAt(line, i) !== "word") i = nextCodePoint(line, i);
  while (i < line.length && classAt(line, i) === "word") i = nextCodePoint(line, i);
  return i;
}

/** The start of the word before `col` (⌥←), or 0 if there is none. */
export function prevWordStart(line: string, col: number): number {
  let i = col;
  while (i > 0 && classAt(line, prevCodePoint(line, i)) !== "word") i = prevCodePoint(line, i);
  while (i > 0 && classAt(line, prevCodePoint(line, i)) === "word") i = prevCodePoint(line, i);
  return i;
}

/**
 * What a double-click at `col` selects: the run of word characters, of
 * whitespace, or of other characters it falls in. At the very end of the line
 * it takes the run before.
 */
export function wordAt(line: string, col: number): { start: number; end: number } {
  if (line.length === 0) return { start: 0, end: 0 };
  const at = col >= line.length ? prevCodePoint(line, line.length) : col;
  const kind = classAt(line, at);
  let start = at;
  while (start > 0 && classAt(line, prevCodePoint(line, start)) === kind) start = prevCodePoint(line, start);
  let end = at;
  while (end < line.length && classAt(line, end) === kind) end = nextCodePoint(line, end);
  return { start, end };
}

/**
 * East Asian wide and fullwidth characters and emoji take two cells in a
 * monospace font; everything else one. A coarse table rather than the full
 * Unicode width data — it covers what shows up in notes and code, and a miss
 * only shifts the cursor on that one line.
 */
const WIDE_RANGES = [
  "\\u1100-\\u115F", // Hangul Jamo
  "\\u2E80-\\u303E\\u3041-\\u33FF", // CJK radicals, punctuation, kana
  "\\u3400-\\u4DBF\\u4E00-\\u9FFF\\uA000-\\uA4CF", // CJK ideographs, Yi
  "\\uAC00-\\uD7A3", // Hangul syllables
  "\\uF900-\\uFAFF\\uFE30-\\uFE4F", // CJK compatibility
  "\\uFF00-\\uFF60\\uFFE0-\\uFFE6", // fullwidth forms
  "\\u{1F300}-\\u{1FAFF}", // emoji and pictographs
  "\\u{20000}-\\u{3FFFD}", // CJK extensions
].join("");
const WIDE = new RegExp(`[${WIDE_RANGES}]|\\p{Extended_Pictographic}`, "u");

/** Cells one grapheme takes at display column `column` (a tab runs to the next stop). */
export function cellWidth(grapheme: string, column: number, tabSize: number): number {
  if (grapheme === "\t") return tabSize - (column % tabSize);
  return WIDE.test(grapheme) ? 2 : 1;
}

/** The display column of `col`: cells before it, tabs expanded to `tabSize` stops. */
export function displayColumn(line: string, col: number, tabSize: number): number {
  let column = 0;
  for (const seg of graphemes.segment(line.slice(0, col))) column += cellWidth(seg.segment, column, tabSize);
  return column;
}

/**
 * The UTF-16 column nearest to display column `target` — what ↑/↓ use to keep
 * the cursor's visual column across lines of different content.
 */
export function colForDisplayColumn(line: string, target: number, tabSize: number): number {
  let column = 0;
  for (const seg of graphemes.segment(line)) {
    const width = cellWidth(seg.segment, column, tabSize);
    if (column + width > target) {
      return target - column < column + width - target ? seg.index : seg.index + seg.segment.length;
    }
    column += width;
  }
  return line.length;
}

/** The leading whitespace of a line. */
export function leadingWhitespace(line: string): string {
  return /^[ \t]*/.exec(line)?.[0] ?? "";
}
