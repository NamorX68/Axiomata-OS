/**
 * Words from the open file as a second completion source (board card #36), as blink.cmp's buffer source in the
 * owner's Neovim: the server's items first, then the file's own words — and without a server, the words alone, so
 * Markdown, notes and config files complete too (owner, 2026-10-19).
 *
 * * **Near words first**: lines are read outward from the cursor, up to a limit, so a large file stays fast and the
 *   words of the code around the cursor are the ones offered.
 * * **Only after a trigger the server speaks alone**: after `.` or `::` a file word is no member of anything.
 * * **A file word ranks below a server item** of the same match (`source: "file"`, see `filterItems`).
 */

import type { CompletionAnswer, CompletionItem } from "../editor/lsp/completion";
import type { Pos } from "../editor/position";
import type { CompletionPort } from "./completionMenu";

/** Lines read on each side of the cursor for words. */
export const MAX_WORD_LINES = 1000;
/** Most distinct words offered. */
export const MAX_WORDS = 2000;
/** Shortest word worth offering; shorter ones are quicker typed than chosen. */
export const MIN_WORD_LENGTH = 3;

/** The protocol's kind for plain text (`txt` in the menu). */
const TEXT_KIND = 1;

const WORD = /[\p{L}\p{N}_$]+/gu;

/** What the source reads of the document: a line by number, how many there are. */
export interface LineSource {
  lineCount: number;
  line(n: number): string;
}

/**
 * The words of `lines` nearest to `at` first: the cursor's line, then one above and one below, outward. The word the
 * cursor is in is left out — it is what is being typed.
 */
export function wordsAround(lines: LineSource, at: Pos): string[] {
  const seen = new Set<string>();
  const current = wordAt(lines.line(at.line), at.col);
  const take = (n: number) => {
    for (const match of lines.line(n).matchAll(WORD)) {
      const word = match[0];
      if (word.length >= MIN_WORD_LENGTH && word !== current && !/^\p{N}+$/u.test(word)) seen.add(word);
      if (seen.size >= MAX_WORDS) return;
    }
  };
  for (let d = 0; d < MAX_WORD_LINES && seen.size < MAX_WORDS; d++) {
    const up = at.line - d;
    const down = at.line + d;
    if (up < 0 && down >= lines.lineCount) break;
    if (up >= 0) take(up);
    if (d > 0 && down < lines.lineCount) take(down);
  }
  return [...seen];
}

/** The whole word around `col`, or `""`. */
function wordAt(line: string, col: number): string {
  for (const match of line.matchAll(WORD)) {
    const start = match.index ?? 0;
    if (start <= col && col <= start + match[0].length) return match[0];
  }
  return "";
}

/** A file word as a menu item. */
export function wordItem(word: string): CompletionItem {
  return {
    label: word,
    detail: "file",
    kind: TEXT_KIND,
    filterText: word,
    // After every server item when nothing is typed yet (the server's order rules then).
    sortText: `￿${word}`,
    insertText: word,
    snippet: false,
    range: null,
    additionalEdits: [],
    documentation: null,
    preselect: false,
    source: "file",
    raw: null,
  };
}

/**
 * The file's words for the menu, read once per word being typed: the menu asks again on every character while it is
 * closed, and reading two thousand lines each time is wasted while only the typed word grows. The list is read again
 * when a new word begins; the typed word itself is left out on every ask.
 */
export function fileWordSource(lines: LineSource): (at: Pos) => string[] {
  let key = "";
  let words: string[] = [];
  return (at) => {
    const line = lines.line(at.line);
    const current = wordAt(line, at.col);
    const next = `${at.line}:${wordStartAt(line, at.col)}`;
    if (next !== key) {
      key = next;
      words = wordsAround(lines, at);
    }
    return current === "" ? words : words.filter((w) => w !== current);
  };
}

/** Where the word before `col` begins. */
function wordStartAt(line: string, col: number): number {
  let start = col;
  while (start > 0 && /[\p{L}\p{N}_$]/u.test(line[start - 1])) start--;
  return start;
}

/** `server` with the file's words added (or the words alone when there is no server, or when it fails). */
export function withFileWords(server: CompletionPort | null, words: (at: Pos) => string[]): CompletionPort {
  return {
    ask: async (at, trigger, again): Promise<CompletionAnswer> => {
      const found = trigger === null ? words(at) : [];
      const empty: CompletionAnswer = { items: [], incomplete: false };
      const answer = server ? await server.ask(at, trigger, again).catch(() => empty) : empty;
      if (trigger !== null) return answer;
      const offered = new Set(answer.items.map((i) => i.label));
      const extra = found.filter((w) => !offered.has(w)).map(wordItem);
      return { items: [...answer.items, ...extra], incomplete: answer.incomplete };
    },
    resolve: (item) => (item.source === "file" || !server ? Promise.resolve(item) : server.resolve(item)),
    triggers: () => server?.triggers() ?? [],
  };
}
