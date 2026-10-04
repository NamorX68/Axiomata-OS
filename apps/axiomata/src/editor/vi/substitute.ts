/**
 * `:s` (`docs/plans/editor.md`, ED3, V6): `:[range]s/pattern/replacement/[g i I &]`.
 *
 * * Any non-letter, non-digit delimiter works (`:s#a#b#`); `\` before it
 *   makes it literal. An empty pattern reuses the last search.
 * * The replacement knows Vim's atoms: `&` and `\0` (the match), `\1`–`\9`,
 *   `~` (the previous replacement), `\r`/`\n` (a line break), `\t`, `\u` `\l`
 *   (next character's case), `\U` `\L` … `\E`/`\e` (case until the end).
 * * `:s` alone repeats the last substitution without its flags; `&` as the
 *   first flag keeps them. `c` asks at every match (ED5, T12).
 * * A pattern that names a line break (`\n`) is matched over the range's
 *   joined text, so a replacement can join lines (ED5, T5).
 *
 * All pure: this computes the new lines, the machine applies them.
 */

import type { TextStore } from "../buffer";
import { pos, range, type Range } from "../position";
import { compilePattern, type CaseMode } from "./search";
import type { LineRange } from "./ex";

export interface SubstituteSpec {
  /** Empty: the last search pattern. */
  pattern: string;
  replacement: string;
  global: boolean;
  caseMode: CaseMode;
  /** The `c` flag (ED5, T12): ask at every match. */
  confirm?: boolean;
}

/** The text after `:s`, parsed; `null` for a bare `:s` (repeat the last one). */
export function parseSubstitute(args: string, last: SubstituteSpec | null): SubstituteSpec | { error: string } | null {
  const text = args.trimStart();
  if (text === "" || /^[&giI]+$/.test(text)) {
    if (!last) return { error: "E35: No previous regular expression" };
    return withFlags({ ...last, global: false, caseMode: "smart" }, text, last);
  }
  const delim = text[0];
  if (/[a-zA-Z0-9\\"|\s]/.test(delim)) return { error: "E146: Regular expressions can't be delimited by letters" };
  const [pattern, afterPattern] = readPart(text, 1, delim);
  if (afterPattern >= text.length) return { pattern, replacement: "", global: false, caseMode: "smart" };
  const [replacement, afterReplacement] = readPart(text, afterPattern + 1, delim);
  const flags = afterReplacement >= text.length ? "" : text.slice(afterReplacement + 1).trim();
  return withFlags({ pattern, replacement, global: false, caseMode: "smart" }, flags, last);
}

function withFlags(
  spec: SubstituteSpec,
  flags: string,
  last: SubstituteSpec | null,
): SubstituteSpec | { error: string } {
  let out = { ...spec };
  for (let i = 0; i < flags.length; i++) {
    const f = flags[i];
    if (f === "&" && i === 0) {
      if (last) out = { ...out, global: last.global, caseMode: last.caseMode };
    } else if (f === "g") out.global = !out.global;
    else if (f === "i") out.caseMode = "ignore";
    else if (f === "I") out.caseMode = "match";
    else if (f === "c") out.confirm = true;
    else return { error: `E488: Trailing characters: ${flags.slice(i)}` };
  }
  return out;
}

/**
 * One part of `/pat/rep/` from `start` up to the next unescaped `delim`:
 * `\{delim}` becomes the delimiter itself (escaped again if the pattern
 * language needs it), every other escape is kept for the next stage.
 */
function readPart(text: string, start: number, delim: string): [string, number] {
  let out = "";
  let i = start;
  for (; i < text.length; i++) {
    const ch = text[i];
    if (ch === delim) break;
    if (ch === "\\" && i + 1 < text.length) {
      const next = text[i + 1];
      out += next === delim ? (/[\\^$.*+?()[\]{}|/]/.test(delim) ? `\\${delim}` : delim) : `\\${next}`;
      i++;
      continue;
    }
    out += ch;
  }
  return [out, i];
}

type CaseState = { one: "upper" | "lower" | null; all: "upper" | "lower" | null };

function applyCase(text: string, state: CaseState): string {
  let out = "";
  for (const ch of text) {
    let c = state.all === "upper" ? ch.toUpperCase() : state.all === "lower" ? ch.toLowerCase() : ch;
    if (state.one) {
      c = state.one === "upper" ? ch.toUpperCase() : ch.toLowerCase();
      state.one = null;
    }
    out += c;
  }
  return out;
}

/** The replacement for one match (`m`), with `~` standing for `previous`. */
export function expandReplacement(replacement: string, m: RegExpExecArray, previous: string): string {
  const state: CaseState = { one: null, all: null };
  let out = "";
  for (let i = 0; i < replacement.length; i++) {
    const ch = replacement[i];
    if (ch === "&") {
      out += applyCase(m[0], state);
      continue;
    }
    if (ch === "~") {
      out += applyCase(previous, state);
      continue;
    }
    if (ch !== "\\" || i + 1 >= replacement.length) {
      out += applyCase(ch, state);
      continue;
    }
    const next = replacement[++i];
    if (/[0-9]/.test(next)) out += applyCase(m[Number(next)] ?? "", state);
    else if (next === "n" || next === "r") out += "\n";
    else if (next === "t") out += "\t";
    else if (next === "u") state.one = "upper";
    else if (next === "l") state.one = "lower";
    else if (next === "U") state.all = "upper";
    else if (next === "L") state.all = "lower";
    else if (next === "E" || next === "e") state.all = null;
    else out += applyCase(next, state);
  }
  return out;
}

/** What a substitution did, line by line. */
export interface Substitution {
  /** Changed lines, bottom first (so applying them in order keeps line numbers valid). */
  lines: Array<{ line: number; text: string }>;
  /** Number of replacements. */
  count: number;
  /** The last line a replacement happened on — where the cursor goes. */
  lastLine: number;
}

/**
 * Runs `spec` (its pattern already resolved) over `range`, or returns an
 * error message: a bad pattern, or `E486` when nothing matched.
 */
export function substituteLines(
  store: TextStore,
  range: LineRange,
  spec: SubstituteSpec,
  previousReplacement: string,
): Substitution | { error: string } {
  const compiled = compilePattern(spec.pattern, spec.caseMode);
  if ("error" in compiled) return { error: `E486: ${compiled.error}` };
  const re = compiled.re;
  const lines: Array<{ line: number; text: string }> = [];
  let count = 0;
  let lastLine = -1;
  for (let line = range.last; line >= range.first; line--) {
    const result = substituteInLine(store.line(line), re, spec, previousReplacement);
    if (!result) continue;
    lines.push({ line, text: result.text });
    count += result.count;
    if (lastLine < 0) lastLine = line;
  }
  if (count === 0) return { error: `E486: Pattern not found: ${spec.pattern}` };
  return { lines, count, lastLine };
}

/** One line's substitutions (`spec.global` for every match, else just the first); `null` if none matched. */
function substituteInLine(
  text: string,
  re: RegExp,
  spec: SubstituteSpec,
  previousReplacement: string,
): { text: string; count: number } | null {
  let out = "";
  let copied = 0;
  let replaced = 0;
  re.lastIndex = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    out += text.slice(copied, m.index) + expandReplacement(spec.replacement, m, previousReplacement);
    copied = m.index + m[0].length;
    replaced++;
    if (!spec.global) break;
    if (m[0].length === 0) {
      // An empty match: keep the character after it and move on.
      if (m.index >= text.length) break;
      const step = text.codePointAt(m.index)! > 0xffff ? 2 : 1;
      out += text.slice(m.index, m.index + step);
      copied = m.index + step;
      re.lastIndex = copied;
    }
  }
  if (replaced === 0) return null;
  out += text.slice(copied);
  return { text: out, count: replaced };
}

/** A `:s` over lines joined into one text: the new text for the whole range. */
export interface SpanningSubstitution {
  /** The range's lines, from the first line's start to the last line's end. */
  range: Range;
  text: string;
  count: number;
  /** The line (in the new text) the last replacement starts on — where the cursor goes. */
  lastLine: number;
}

/**
 * `:s` with a pattern that names a line break (T5): matched over the range's
 * lines joined with `\n`. Without `g`, only the first match starting on each
 * line is replaced, as line by line.
 */
export function substituteSpanning(
  store: TextStore,
  lines: LineRange,
  spec: SubstituteSpec,
  previousReplacement: string,
): SpanningSubstitution | { error: string } {
  const compiled = compilePattern(spec.pattern, spec.caseMode);
  if ("error" in compiled) return { error: `E486: ${compiled.error}` };
  const re = compiled.re;
  const whole = range(pos(lines.first, 0), pos(lines.last, store.line(lines.last).length));
  const text = store.slice(whole);
  let out = "";
  let copied = 0;
  let count = 0;
  let lastLine = -1;
  // Which line a match starts on, counted as the scan goes (matches come in order).
  let line = lines.first;
  let scanned = 0;
  let replacedOn = -1;
  re.lastIndex = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    for (let nl = text.indexOf("\n", scanned); nl !== -1 && nl < m.index; nl = text.indexOf("\n", scanned)) {
      line++;
      scanned = nl + 1;
    }
    if (spec.global || line !== replacedOn) {
      out += text.slice(copied, m.index);
      lastLine = lines.first + (out.match(/\n/g)?.length ?? 0);
      out += expandReplacement(spec.replacement, m, previousReplacement);
      copied = m.index + m[0].length;
      count++;
      replacedOn = line;
    }
    if (m[0].length === 0) {
      if (m.index >= text.length) break;
      re.lastIndex = m.index + (text.codePointAt(m.index)! > 0xffff ? 2 : 1);
    }
  }
  if (count === 0) return { error: `E486: Pattern not found: ${spec.pattern}` };
  return { range: whole, text: out + text.slice(copied), count, lastLine };
}
