/**
 * File locations in program output (`src/main.rs:12:5`, `File "a.py", line 3`, `at f (/x/y.js:7:1)`):
 * the places a ⌘-click in a task's or agent's terminal opens in the editor.
 *
 * Pure text in, ranges out — `modules/terminal.svelte` asks for the range under the pointer, and the Studio
 * turns the path into a project file (`ide/outputPath.ts`). Deliberately conservative: a path without a
 * folder part must end in an extension of a language we know, so `localhost:8000` or `example.com:443`
 * never become links.
 */

export interface OutputRef {
  /** The path as printed (relative, or absolute). */
  path: string;
  /** One-based, as printed. */
  line: number;
  col: number | null;
  /** Range in the text, `end` exclusive — what to underline, and where a click counts. */
  start: number;
  end: number;
}

/** Extensions that count as a file name on their own (no `/` in the path). */
const KNOWN = new Set([
  "py", "pyi", "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte", "vue", "go", "java", "kt", "swift", "c", "h",
  "cc", "cpp", "hpp", "cs", "rb", "php", "lua", "sh", "zsh", "toml", "json", "yaml", "yml", "md", "css", "scss", "html",
  "sql", "txt", "ini", "cfg", "lock",
]);

// `path:line[:col]` — the shape of rustc, gcc, pytest, ruff, mypy, eslint, node, go and most others.
const COLON = /((?:[A-Za-z]:)?[\w.@~+-]*(?:[\\/][\w.@~+ -]+)*[\w.@~+-]+\.[A-Za-z][A-Za-z0-9]{0,7}):(\d+)(?::(\d+))?/g;
// Python tracebacks: `File "path", line 12`
const PYTHON = /File "([^"\n]+)", line (\d+)/g;
// tsc: `path(12,5)`
const PAREN = /((?:[A-Za-z]:)?[\w.@~+/\\-]+\.[A-Za-z][A-Za-z0-9]{0,7})\((\d+),(\d+)\)/g;

function plausible(path: string, before: string): boolean {
  if (before.endsWith("://") || before.endsWith("//")) return false; // a URL
  const base = path.split(/[\\/]/).pop() ?? path;
  const ext = base.includes(".") ? base.split(".").pop()!.toLowerCase() : "";
  if (!ext) return false;
  return /[\\/]/.test(path) || KNOWN.has(ext);
}

/** Every location in `text`, in order, without overlaps. */
export function findOutputRefs(text: string): OutputRef[] {
  const found: OutputRef[] = [];
  const add = (match: RegExpExecArray, pathGroup: number, lineGroup: number, colGroup: number | null) => {
    const path = match[pathGroup];
    const start = match.index;
    if (!plausible(path, text.slice(Math.max(0, start - 3), start))) return;
    const end = start + match[0].length;
    if (found.some((r) => start < r.end && end > r.start)) return;
    const line = Number(match[lineGroup]);
    if (!Number.isSafeInteger(line) || line < 1) return;
    const colText = colGroup === null ? undefined : match[colGroup];
    found.push({ path, line, col: colText ? Number(colText) : null, start, end });
  };
  for (const m of text.matchAll(PYTHON)) add(m, 1, 2, null);
  for (const m of text.matchAll(PAREN)) add(m, 1, 2, 3);
  for (const m of text.matchAll(COLON)) add(m, 1, 2, 3);
  return found.sort((a, b) => a.start - b.start);
}

/** The location under character index `col` of `text`, if any. */
export function outputRefAt(text: string, col: number): OutputRef | null {
  return findOutputRefs(text).find((r) => col >= r.start && col < r.end) ?? null;
}

/** Terminal cells as one string with one character per column (wide-character spacers and blanks are spaces). */
export function rowText(cells: readonly { ch: string }[]): string {
  return cells.map((c) => (c.ch.length === 1 ? c.ch : c.ch ? c.ch.slice(0, 1) : " ")).join("");
}
