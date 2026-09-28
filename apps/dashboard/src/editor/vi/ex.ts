/**
 * Vi's ex commands (`docs/plans/editor.md`, ED3, V6): the `:` line parsed into
 * a line range and a command. What exists — deliberately short:
 *
 * `:w` `:q` `:q!` `:wq` `:x` `:e!` `:{n}` `:s` (and `:&`, `:&&`) `:noh`
 * `:set wrap nu rnu list` (and their `no` forms and `!` toggles); since ED5
 * (T12) `:g`/`:g!`/`:v` with a command for each matching line, `:d` and
 * `:norm[al]`.
 *
 * Ranges: `%`, `.`, `$`, a number, a mark (`'a`, `'<`, `'>`), each with
 * `+n`/`-n` offsets, one or two joined by `,` (both from the cursor) or `;`
 * (the second from the first). Not there: search addresses (`/pat/`), `:m`,
 * `:t`, `:j`.
 */

/** Lines `first`–`last`, zero-based and in order. */
export interface LineRange {
  first: number;
  last: number;
}

/** The options `:set` knows (V6). */
export type SetOption = "wrap" | "number" | "relativenumber" | "list";

export type ExCommand =
  | { name: "write"; quit: boolean; onlyIfChanged: boolean }
  | { name: "quit"; force: boolean }
  | { name: "reload" }
  /** `:format` (ED6.5, L7): the file through its formatter. */
  | { name: "format" }
  /** `:rename {name}` (ED6.5, L17): the symbol under the cursor, through the language server. */
  | { name: "rename"; newName: string }
  | { name: "goto"; line: number }
  | { name: "substitute"; range: LineRange; args: string }
  /** `:&` repeats the last `:s` without its flags, `:&&` with them. */
  | { name: "repeatSubstitute"; range: LineRange; keepFlags: boolean }
  | { name: "nohlsearch" }
  | { name: "set"; option: SetOption; value: boolean | "toggle" }
  /** `:g/pat/cmd` runs `command` on each line matching `pattern` (`invert`: each one not matching — `:v`, `:g!`). */
  | { name: "global"; range: LineRange; pattern: string; invert: boolean; command: string }
  /** `:d [x]`: the lines, into register `x` (the unnamed one without). */
  | { name: "delete"; range: LineRange; register: string | null }
  /** `:norm[al] {keys}`: the keys in Normal mode, on each line of the range (the cursor's line without one). */
  | { name: "normal"; range: LineRange | null; keys: string };

/** What a range address can refer to. */
export interface ExContext {
  /** The cursor's line. */
  line: number;
  lineCount: number;
  /** A mark's line in this document, `null` if unset. */
  mark(name: string): number | null;
}

/** Full names and the shortest abbreviation each accepts, in Tab-completion order. */
const COMMANDS: ReadonlyArray<{ name: string; min: number }> = [
  { name: "write", min: 1 },
  { name: "wq", min: 2 },
  { name: "xit", min: 1 },
  { name: "quit", min: 1 },
  { name: "edit", min: 1 },
  { name: "substitute", min: 1 },
  { name: "nohlsearch", min: 3 },
  { name: "set", min: 2 },
  { name: "delete", min: 1 },
  { name: "normal", min: 4 },
  { name: "format", min: 3 },
  { name: "rename", min: 3 },
];

const OPTIONS: ReadonlyArray<{ name: SetOption; short: string }> = [
  { name: "wrap", short: "wrap" },
  { name: "number", short: "nu" },
  { name: "relativenumber", short: "rnu" },
  { name: "list", short: "list" },
];

/** A full command name for what was typed (`w`, `wri`, `noh`), or `null`. */
function commandName(typed: string): string | null {
  for (const c of COMMANDS) if (typed.length >= c.min && c.name.startsWith(typed)) return c.name;
  return null;
}

/** Parses one `:` line. An error is Vim-style text for the status line. */
export function parseEx(line: string, ctx: ExContext): ExCommand | { error: string } {
  // Only the start: a trailing space can be part of `:normal`'s keys.
  const text = line.trimStart().replace(/^:+/, "");
  const ranged = parseRange(text, ctx);
  if ("error" in ranged) return ranged;
  const { range, rest } = ranged;
  const body = rest.trimStart();
  // `:{n}` alone: go to that line.
  if (body.trim() === "") {
    if (!range) return { error: "" };
    return { name: "goto", line: range.last };
  }
  const current = range ?? { first: ctx.line, last: ctx.line };
  const substitute = parseSubstituteCommand(body, current);
  if (substitute) return substitute;
  const global = parseGlobal(body, range ?? { first: 0, last: ctx.lineCount - 1 });
  if (global) return global;

  const m = /^([a-zA-Z]+)(!?)\s*(.*)$/.exec(body);
  if (!m) return { error: `E492: Not an editor command: ${body}` };
  const [, typed, bang, arg] = m;
  const name = commandName(typed);
  if (!name) return { error: `E492: Not an editor command: ${body}` };
  if (range && name !== "substitute" && name !== "delete" && name !== "normal") {
    return { error: "E481: No range allowed" };
  }
  switch (name) {
    case "write":
      // Writing elsewhere (`:w other.txt`) is not the editor's; the file app has Save As.
      return arg === "" ? { name: "write", quit: false, onlyIfChanged: false } : badArg(arg);
    case "wq":
      return { name: "write", quit: true, onlyIfChanged: false };
    case "xit":
      return { name: "write", quit: true, onlyIfChanged: true };
    case "quit":
      return { name: "quit", force: bang === "!" };
    case "edit":
      // Only `:e!` — opening files goes through the file app's own ways (⌘O).
      return bang === "!" && arg === "" ? { name: "reload" } : { error: "E32: Only :e! (reload) is supported" };
    case "nohlsearch":
      return { name: "nohlsearch" };
    case "set":
      return parseSet(arg);
    case "delete": {
      const register = arg.trim();
      if (register !== "" && !/^[a-zA-Z0-9"_+*-]$/.test(register)) return badArg(arg);
      return { name: "delete", range: current, register: register || null };
    }
    case "normal":
      if (!arg) return { error: "E471: Argument required" };
      return { name: "normal", range, keys: arg };
    case "format":
      return arg === "" ? { name: "format" } : badArg(arg);
    case "rename": {
      const newName = arg.trim();
      if (!newName) return { error: "E471: Argument required" };
      return { name: "rename", newName };
    }
  }
  return { error: `E492: Not an editor command: ${body}` };
}

/**
 * `:s`, `:&` and `:&&`, special-cased before the generic name matching below:
 * `:s` takes any non-letter as its delimiter, straight after the name
 * (`:s/a/b/`, `:s#a#b#`), which a bare word match cannot express. `null` when
 * `body` is none of them.
 */
function parseSubstituteCommand(body: string, range: LineRange): ExCommand | null {
  const sub = /^s(?:u(?:b(?:s(?:t(?:i(?:t(?:u(?:t(?:e)?)?)?)?)?)?)?)?)?(?=$|[^a-zA-Z])/.exec(body);
  if (sub) return { name: "substitute", range, args: body.slice(sub[0].length) };
  if (body === "&" || body === "&&") return { name: "repeatSubstitute", range, keepFlags: body === "&&" };
  return null;
}

/** `:g` … `:global`, `:v` … `:vglobal`, an optional `!`, then the delimiter (not a letter, digit, space). */
const GLOBAL = /^(g(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?|v(?:g(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?)?)(!?)(?=[^a-zA-Z0-9\s"|])/;

/**
 * `:g/pat/cmd`, `:g!/pat/cmd`, `:v/pat/cmd` (any non-letter delimiter, as for
 * `:s`); the range is the whole file unless one was given. `null` when `body`
 * is not one of them.
 */
function parseGlobal(body: string, range: LineRange): ExCommand | { error: string } | null {
  const m = GLOBAL.exec(body);
  if (!m) return null;
  const invert = m[1].startsWith("v") || m[2] === "!";
  const text = body.slice(m[0].length);
  const delim = text[0];
  let pattern = "";
  let i = 1;
  for (; i < text.length && text[i] !== delim; i++) {
    // `\{delim}` is the delimiter itself; every other escape is the pattern's.
    if (text[i] === "\\" && i + 1 < text.length) {
      pattern += text[i + 1] === delim ? delim : text.slice(i, i + 2);
      i++;
    } else pattern += text[i];
  }
  const command = text.slice(i + 1).trimStart();
  if (!command.trim()) {
    return { error: "E471: Argument required: :g needs a command (:g/pat/d, :g/pat/s//x/, :g/pat/normal …)" };
  }
  return { name: "global", range, pattern, invert, command };
}

function badArg(arg: string): { error: string } {
  return { error: `E488: Trailing characters: ${arg}` };
}

/** `:set wrap`, `:set nowrap`, `:set wrap!`, `:set invwrap`, `:set nu`. */
function parseSet(arg: string): ExCommand | { error: string } {
  const m = /^(no|inv)?([a-z]+)(!?)$/.exec(arg.trim());
  if (!m) return { error: arg ? `E518: Unknown option: ${arg}` : "E471: Argument required" };
  const [, prefix, name, bang] = m;
  const option = OPTIONS.find((o) => o.name === name || o.short === name);
  if (!option) return { error: `E518: Unknown option: ${arg.trim()}` };
  const value = prefix === "inv" || bang === "!" ? "toggle" : prefix !== "no";
  return { name: "set", option: option.name, value };
}

/** One address: `%` is handled by the caller; this reads `.`, `$`, `12`, `'a` and offsets after them. */
function parseAddress(text: string, ctx: ExContext): { line: number; rest: string } | { error: string } | null {
  let rest = text;
  let line: number | null = null;
  const base = /^(\.|\$|\d+|'.)/.exec(rest);
  if (base) {
    const b = base[0];
    rest = rest.slice(b.length);
    if (b === ".") line = ctx.line;
    else if (b === "$") line = ctx.lineCount - 1;
    else if (b.startsWith("'")) {
      line = ctx.mark(b[1]);
      if (line === null) return { error: "E20: Mark not set" };
    } else line = Math.max(0, Number(b) - 1);
  }
  // `+2`, `-`, `+` after an address, or on their own (relative to the cursor).
  while (/^[+-]/.test(rest)) {
    const off = /^([+-])(\d*)/.exec(rest)!;
    rest = rest.slice(off[0].length);
    const n = off[2] === "" ? 1 : Number(off[2]);
    line = (line ?? ctx.line) + (off[1] === "+" ? n : -n);
  }
  return line === null ? null : { line, rest };
}

function parseRange(
  text: string,
  ctx: ExContext,
): { range: LineRange | null; rest: string } | { error: string } {
  if (text.startsWith("%")) return { range: { first: 0, last: ctx.lineCount - 1 }, rest: text.slice(1) };
  const first = parseAddress(text, ctx);
  if (first === null) return { range: null, rest: text };
  if ("error" in first) return first;
  let range = { first: first.line, last: first.line };
  let rest = first.rest;
  if (/^[,;]/.test(rest)) {
    // `;` counts the second address from the first (`5;+2` is 5–7); `,` from the cursor, as in Vim.
    const from = rest[0] === ";" ? { ...ctx, line: first.line } : ctx;
    const second = parseAddress(rest.slice(1), from);
    if (second === null) return { error: "E14: Invalid address" };
    if ("error" in second) return second;
    range = { first: first.line, last: second.line };
    rest = second.rest;
  }
  if (range.first > range.last) range = { first: range.last, last: range.first };
  if (range.first < 0 || range.last >= ctx.lineCount) {
    // `:999` goes to the last line, as in Vim; a range past the end is an error.
    if (range.first === range.last && rest.trim() === "") {
      const line = Math.max(0, Math.min(range.last, ctx.lineCount - 1));
      return { range: { first: line, last: line }, rest };
    }
    return { error: "E16: Invalid range" };
  }
  return { range, rest };
}

/**
 * Tab completion: every way to finish what was typed, in order — command names
 * at the start (after a range), option names after `set `. The typed prefix is
 * kept; the candidates are whole lines.
 */
export function completeEx(line: string): string[] {
  const set = /^(.*\bse(?:t)?\s+)(no|inv)?([a-z]*)$/.exec(line);
  if (set) {
    const [, head, prefix = "", typed] = set;
    const names = OPTIONS.flatMap((o) => [o.name, o.short]).filter((n, i, all) => all.indexOf(n) === i);
    return names.filter((n) => n.startsWith(typed) && n !== typed).map((n) => head + prefix + n);
  }
  const cmd = /^([%.$\d',;+-]*)([a-zA-Z]*)$/.exec(line);
  if (!cmd) return [];
  const [, range, typed] = cmd;
  return COMMANDS.map((c) => c.name)
    .filter((n) => n.startsWith(typed) && n !== typed)
    .map((n) => range + n);
}
