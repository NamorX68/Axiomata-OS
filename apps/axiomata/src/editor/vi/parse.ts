/**
 * The grammar of a Normal- or Visual-mode command (`docs/plans/editor.md`,
 * ED3, D17): `["x][count]` then an operator with a motion or text object, a
 * motion, or a command — `"a3dw`, `2d3w` (six words), `ci(`, `gUiw`, `yss)`.
 *
 * The keys typed so far are parsed from the start on every new key: the answer
 * is a whole command, "not yet" (`d2` waits for its motion) or "never" (`dq`),
 * and "never" throws the keys away as Vim does.
 *
 * The parse also returns the command's keys **without** its counts, so `.` can
 * replay it with a new count (`3.` after `d2w` deletes three words, not six).
 */

import { charOf, isEscape, type ViKey } from "./keys";
import { isRegisterName } from "./registers";

export type ParseMode = "normal" | "visual";

/** Operators, and the key that doubles each into a linewise command (`dd`, `gUU`, `gcc`). */
const OPERATORS: Record<string, string> = {
  d: "d",
  c: "c",
  y: "y",
  ">": ">",
  "<": "<",
  "=": "=",
  "g~": "~",
  gu: "u",
  gU: "U",
  gc: "c",
  ys: "s",
};

const MOTIONS = new Set([
  "h", "j", "k", "l", "<Left>", "<Right>", "<Up>", "<Down>", "<BS>", " ", "<C-n>", "<C-p>", "w", "W", "b", "B", "e",
  "E", "ge", "gE", "0", "^", "$", "g_", "|", "gg", "G", "{", "}", "(", ")", "%", ";", ",", "H", "M", "L", "+", "-",
  "_", "<CR>", "<Home>", "<End>", "gj", "gk", "n", "N", "*", "#",
  // A search: the pattern is typed on the command line before the motion runs (`cmdmode.ts`).
  "/", "?",
]);

/** Motions that take one more character: `f t F T` and the marks `' \``. */
const CHAR_MOTIONS = new Set(["f", "F", "t", "T", "'", "`"]);

/** Text objects after `i`/`a` (the tree-sitter ones `f c a` included, V9). */
const OBJECTS = new Set([
  ..."wWsp\"'`()b[]{}B<>t",
  // The tree-sitter objects: function, class, argument.
  ..."fca",
]);

/** Commands without an argument, in Normal mode. */
const NORMAL_COMMANDS = new Set([
  "x", "X", "s", "S", "D", "C", "Y", "p", "P", "gp", "gP", "J", "gJ", "~", "i", "a", "I", "A", "gi", "gI", "o", "O",
  "R", "u", "<C-r>", ".", "v", "V", "<C-v>", "gv", "<C-a>", "<C-x>", "<C-o>", "<C-i>", "<Tab>", "zt", "zz", "zb",
  "z<CR>", "z.", "z-", "<C-e>", "<C-y>", "<C-d>", "<C-u>", "<C-f>", "<C-b>", "<PageDown>", "<PageUp>", "]c", "[c",
  "]d", "[d", "gd", "K", "gf", "gri", "grr", "grt", "grn", "gra",
  "ZZ", "ZQ", ":", "@@", "&", "<Esc>", "<C-[>", "<C-c>", "gn", "gN", "zc", "zo", "za", "zR", "zM",
]);

/** Commands that exist only in Visual mode, or mean something else there. */
const VISUAL_COMMANDS = new Set([
  "o", "O", "I", "A", "x", "X", "D", "Y", "C", "S", "R", "s", "J", "gJ", "u", "U", "~", "p", "P", "v", "V", "<C-v>",
  "gv", ":", "<C-a>", "<C-x>", "<Esc>", "<C-[>", "<C-c>", "zt", "zz", "zb", "<C-e>", "<C-y>", "<C-d>",
  "<C-u>", "<C-f>", "<C-b>", "gn", "gN", "gra",
]);

/** Commands followed by one character: `r m q @` (and Visual `r`, surround `S`). */
const CHAR_COMMANDS = new Set(["r", "m", "q", "@"]);

/**
 * Every key the grammar knows, by kind — read by the shortcut list's test
 * (`fileapp/shortcuts.test.ts`) so the list cannot fall behind the machine.
 */
export const VI_GRAMMAR = {
  operators: Object.keys(OPERATORS),
  motions: [...MOTIONS],
  charMotions: [...CHAR_MOTIONS],
  objects: [...OBJECTS],
  normal: [...NORMAL_COMMANDS],
  visual: [...VISUAL_COMMANDS],
  charCommands: [...CHAR_COMMANDS],
} as const;

export type Target =
  | { kind: "motion"; name: string; char?: string }
  | { kind: "object"; inner: boolean; name: string }
  /** `gn` / `gN`: the last search's match under or after (before) the cursor (ED5, T12). */
  | { kind: "match"; backward: boolean }
  /** The doubled operator: `dd`, `gUU` — the current line (and count - 1 more). */
  | { kind: "line" };

export type Body =
  | { kind: "motion"; name: string; char?: string }
  | { kind: "operator"; op: string; target: Target; char?: string }
  | { kind: "command"; name: string; char?: string; char2?: string };

export interface Parsed {
  register: string | null;
  /** The product of the counts typed (`2d3w` → 6), `null` if none. */
  count: number | null;
  body: Body;
  /** The keys without the counts (and without the register), for `.`. */
  plain: ViKey[];
}

export type ParseResult = { status: "incomplete" } | { status: "invalid" } | { status: "done"; command: Parsed };

const INCOMPLETE: ParseResult = { status: "incomplete" };
const INVALID: ParseResult = { status: "invalid" };

/** Reads keys one at a time and remembers which of them were counts or the register. */
class Reader {
  i = 0;
  readonly skipped = new Set<number>();
  constructor(readonly keys: readonly ViKey[]) {}

  get done(): boolean {
    return this.i >= this.keys.length;
  }

  peek(offset = 0): ViKey | undefined {
    return this.keys[this.i + offset];
  }

  /** A count here (digits, not starting with 0), marking its keys as counts. */
  count(): number | null {
    let digits = "";
    while (!this.done) {
      const ch = charOf(this.peek());
      if (ch === null || !/[0-9]/.test(ch) || (digits === "" && ch === "0")) break;
      digits += ch;
      this.skipped.add(this.i);
      this.i++;
    }
    return digits ? Number(digits) : null;
  }

  /**
   * The longest token from `table` here, or `"incomplete"` when the keys so
   * far are only the start of one (`g` of `gu`, `gr` of `grr`) and none is
   * whole yet, or `null` when none matches.
   */
  token(table: ReadonlySet<string> | Record<string, unknown>): string | "incomplete" | null {
    const has = (t: string) => (table instanceof Set ? table.has(t) : t in table);
    const tokens = [...(table instanceof Set ? table : Object.keys(table))];
    let text = "";
    let best: { text: string; keys: number } | null = null;
    let longer = false;
    for (let n = 0; this.i + n < this.keys.length; n++) {
      const key = this.keyText(n);
      if (key === null) break;
      text += key;
      if (has(text)) best = { text, keys: n + 1 };
      longer = tokens.some((t) => t.length > text.length && t.startsWith(text));
      if (!longer) break;
    }
    if (best) {
      this.i += best.keys;
      return best.text;
    }
    // Every key read and still the start of a longer token: wait for more.
    return longer ? "incomplete" : null;
  }

  private keyText(offset: number): string | null {
    const key = this.keys[this.i + offset];
    return typeof key === "string" ? key : null;
  }

  plain(): ViKey[] {
    return this.keys.filter((_, index) => !this.skipped.has(index));
  }
}

function multiply(a: number | null, b: number | null): number | null {
  if (a === null) return b;
  if (b === null) return a;
  return a * b;
}

/** Parses `keys` typed in `mode`. `recording` makes a lone `q` a command (stop recording). */
export function parse(keys: readonly ViKey[], mode: ParseMode, recording = false): ParseResult {
  if (keys.length === 0) return INCOMPLETE;
  const r = new Reader(keys);
  let count = r.count();
  let register: string | null = null;
  if (charOf(r.peek()) === '"') {
    r.skipped.add(r.i);
    r.i++;
    if (r.done) return INCOMPLETE;
    const name = charOf(r.peek());
    if (name === null || !isRegisterName(name)) return INVALID;
    r.skipped.add(r.i);
    r.i++;
    register = name;
    count = multiply(count, r.count());
  }
  if (r.done) return INCOMPLETE;
  if (isEscape(r.peek())) {
    r.i++;
    return done(r, register, count, { kind: "command", name: "<Esc>" });
  }

  // A lone `q` while recording stops it; otherwise `q` takes a register.
  if (recording && charOf(r.peek()) === "q") {
    r.i++;
    return done(r, register, count, { kind: "command", name: "q" });
  }

  const op = r.token(OPERATORS);
  if (op === "incomplete") return INCOMPLETE;
  if (op !== null) return parseOperator(r, mode, register, count, op);
  return parseRest(r, mode, register, count);
}

function done(r: Reader, register: string | null, count: number | null, body: Body): ParseResult {
  if (!r.done) return INVALID;
  return { status: "done", command: { register, count, body, plain: r.plain() } };
}

/** After an operator: `[count]` then a motion, a text object, or the operator's own key again. */
function parseOperator(
  r: Reader,
  mode: ParseMode,
  register: string | null,
  count: number | null,
  op: string,
): ParseResult {
  // In Visual mode the selection is the target.
  if (mode === "visual") {
    if (op === "ys") return INVALID;
    return done(r, register, count, { kind: "operator", op, target: { kind: "motion", name: "<visual>" } });
  }
  const total = multiply(count, r.count());
  if (r.done) return INCOMPLETE;
  if (isEscape(r.peek())) return INVALID;
  const doubled = OPERATORS[op];
  const key = charOf(r.peek());
  // `dd`, `yy`, `gUU`, `gcc`, `yss`; `g~g~`, `gugu`, `gUgU` too.
  if (key === doubled || (op.length === 2 && key === op[0] && charOf(r.peek(1)) === op[1])) {
    r.i += key === doubled ? 1 : 2;
    return withSurroundChar(r, register, total, op, { kind: "line" });
  }
  if (op.length === 2 && key === op[0] && r.i + 1 >= r.keys.length) return INCOMPLETE;
  if (key === "i" || key === "a") {
    r.i++;
    if (r.done) return INCOMPLETE;
    const obj = charOf(r.peek());
    if (obj === null || !OBJECTS.has(obj)) return INVALID;
    r.i++;
    return withSurroundChar(r, register, total, op, { kind: "object", inner: key === "i", name: obj });
  }
  // `cs` and `ds` (surround) are commands that begin like an operator.
  if (op === "c" || op === "d") {
    if (key === "s") {
      r.i++;
      return surroundCommand(r, register, total, op === "c" ? "cs" : "ds");
    }
  }
  // `dgn`, `cgN`: the next (previous) match of the last search.
  if (key === "g") {
    if (r.i + 1 >= r.keys.length) return INCOMPLETE;
    const second = charOf(r.peek(1));
    if (second === "n" || second === "N") {
      r.i += 2;
      return withSurroundChar(r, register, total, op, { kind: "match", backward: second === "N" });
    }
  }
  const target = motionTarget(r);
  if (target === "incomplete") return INCOMPLETE;
  if (target === null) return INVALID;
  return withSurroundChar(r, register, total, op, target);
}

/** `ys{target}{char}`: the surround operator wants one more character. */
function withSurroundChar(
  r: Reader,
  register: string | null,
  count: number | null,
  op: string,
  target: Target,
): ParseResult {
  if (op !== "ys") return done(r, register, count, { kind: "operator", op, target });
  if (r.done) return INCOMPLETE;
  const ch = charOf(r.peek());
  if (ch === null) return INVALID;
  r.i++;
  return done(r, register, count, { kind: "operator", op, target, char: ch });
}

/** `cs{old}{new}` and `ds{char}`. */
function surroundCommand(r: Reader, register: string | null, count: number | null, name: "cs" | "ds"): ParseResult {
  if (r.done) return INCOMPLETE;
  const first = charOf(r.peek());
  if (first === null) return INVALID;
  r.i++;
  if (name === "ds") return done(r, register, count, { kind: "command", name, char: first });
  if (r.done) return INCOMPLETE;
  const second = charOf(r.peek());
  if (second === null) return INVALID;
  r.i++;
  return done(r, register, count, { kind: "command", name, char: first, char2: second });
}

/** A motion target, with its character for `f t F T ' \``. */
function motionTarget(r: Reader): Target | "incomplete" | null {
  const key = charOf(r.peek());
  if (key !== null && CHAR_MOTIONS.has(key)) {
    r.i++;
    if (r.done) return "incomplete";
    const ch = charOf(r.peek());
    if (ch === null) return null;
    r.i++;
    return { kind: "motion", name: key, char: ch };
  }
  const m = r.token(MOTIONS);
  if (m === "incomplete") return "incomplete";
  return m === null ? null : { kind: "motion", name: m };
}

/** Everything that is not an operator: a motion, a command, a command with a character. */
function parseRest(r: Reader, mode: ParseMode, register: string | null, count: number | null): ParseResult {
  const key = charOf(r.peek());
  if (key !== null && CHAR_COMMANDS.has(key)) {
    r.i++;
    if (r.done) return INCOMPLETE;
    const ch = r.peek();
    // `r<CR>` splits the line; otherwise one character.
    const arg = ch === "<CR>" ? "\n" : charOf(ch);
    if (arg === null) return INVALID;
    r.i++;
    return done(r, register, count, { kind: "command", name: key, char: arg });
  }
  if (mode === "visual") {
    // Visual `S{char}` surrounds the selection; `i`/`a` pick an object to select.
    if (key === "S") {
      r.i++;
      if (r.done) return INCOMPLETE;
      const ch = charOf(r.peek());
      if (ch === null) return INVALID;
      r.i++;
      return done(r, register, count, { kind: "command", name: "S", char: ch });
    }
    if (key === "i" || key === "a") {
      r.i++;
      if (r.done) return INCOMPLETE;
      const obj = charOf(r.peek());
      if (obj === null || !OBJECTS.has(obj)) return INVALID;
      r.i++;
      return done(r, register, count, { kind: "command", name: `v${key}`, char: obj });
    }
  }
  const target = motionTarget(r);
  if (target !== "incomplete" && target !== null && target.kind === "motion") {
    return done(r, register, count, { kind: "motion", name: target.name, char: target.char });
  }
  if (target === "incomplete") {
    // `g` might still be `gv`, `gi` …: try the command table before waiting.
    const saved = r.i;
    const cmd = r.token(mode === "visual" ? new Set([...VISUAL_COMMANDS, ...NORMAL_COMMANDS]) : NORMAL_COMMANDS);
    if (cmd === "incomplete" || cmd === null) return INCOMPLETE;
    r.i = saved;
  }
  const table = mode === "visual" ? new Set([...VISUAL_COMMANDS, ...NORMAL_COMMANDS]) : NORMAL_COMMANDS;
  const cmd = r.token(table);
  if (cmd === "incomplete") return INCOMPLETE;
  if (cmd === null) return INVALID;
  return done(r, register, count, { kind: "command", name: cmd });
}
