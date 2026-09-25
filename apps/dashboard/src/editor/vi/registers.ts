/**
 * Vi's registers (`docs/plans/editor.md`, ED3, V3, V4).
 *
 * * `"` — the unnamed register. While the clipboard is shared with the Mac
 *   (the default, D17) it *is* the Mac clipboard: every yank and delete lands
 *   there, and `p` pastes whatever was last copied anywhere.
 * * `"0` the last yank, `"1`–`"9` the last multi-line deletes (shifting down),
 *   `"-` the last small delete, `"a`–`"z` named (`"A`–`"Z` append), `"_` the
 *   black hole, `"+`/`"*` always the Mac clipboard.
 * * `".` `"%` `":` `"/` are read-only: the last inserted text, the file name,
 *   the last command line, the last search — their owners keep them current.
 *
 * The Mac clipboard is text only, and reading it is asynchronous (it goes
 * through Rust, V3). What kind of text it was — characters, lines or a block —
 * is remembered for the text last written; text copied elsewhere counts as
 * lines when it ends with a line break, as in Vim.
 */

/** How a register's text goes back in: within a line, as whole lines, or as a column block. */
export type RegisterKind = "char" | "line" | "block";

export interface RegisterContent {
  text: string;
  kind: RegisterKind;
}

/** The Mac clipboard, as the machine reaches it. */
export interface ClipboardPort {
  /** The clipboard's text — at once if the port has it, else when it arrives. */
  read(): string | Promise<string>;
  write(text: string): void;
}

/** A clipboard read the machine has to wait for before it can go on (see `ViMachine`). */
export class ClipboardPending {
  constructor(readonly text: Promise<string>) {}
}

const NAMED = /^[a-z]$/;
const APPEND = /^[A-Z]$/;
const NUMBERED = /^[0-9]$/;
const READ_ONLY = new Set([".", "%", ":", "/"]);

export function isRegisterName(name: string): boolean {
  return /^[a-zA-Z0-9"\-_+*.%:/]$/.test(name);
}

export class Registers {
  private readonly store = new Map<string, RegisterContent>();
  /** What was last written to the clipboard, so reading it back keeps its kind. */
  private lastClip: RegisterContent | null = null;
  /** A clipboard text fetched for the command being executed now; cleared after it. */
  private fresh: string | null = null;
  /** Owners of the read-only registers. */
  readonly readOnly: Record<string, () => string> = {};

  constructor(
    private readonly clipboard: ClipboardPort | null,
    /** Whether `"` is the Mac clipboard (V4, D17); off when the setting separates them. */
    public shared = true,
  ) {}

  /** Named registers only — what survives a restart (V4). */
  named(): Record<string, RegisterContent> {
    const out: Record<string, RegisterContent> = {};
    for (const [name, content] of this.store) if (NAMED.test(name)) out[name] = content;
    return out;
  }

  restoreNamed(saved: Record<string, RegisterContent>): void {
    for (const [name, content] of Object.entries(saved)) if (NAMED.test(name)) this.store.set(name, content);
  }

  /** Hands the machine a clipboard text it waited for; used by the next `get` only. */
  provide(text: string): void {
    this.fresh = text;
  }

  /** Forgets a provided clipboard text once the command that needed it is done. */
  settle(): void {
    this.fresh = null;
  }

  private isClipboard(name: string): boolean {
    return name === "+" || name === "*" || (name === '"' && this.shared);
  }

  /**
   * The content of register `name`, `null` if empty. A clipboard register may
   * throw {@link ClipboardPending}: the machine waits for the text, hands it
   * over with {@link provide} and runs the command again.
   */
  get(name: string): RegisterContent | null {
    const reg = name.length === 1 && APPEND.test(name) ? name.toLowerCase() : name;
    if (READ_ONLY.has(reg)) {
      const text = this.readOnly[reg]?.() ?? "";
      return text ? { text, kind: "char" } : null;
    }
    if (this.clipboard && this.isClipboard(reg)) return this.readClipboard();
    if (reg === '"') return this.store.get('"') ?? null;
    return this.store.get(reg) ?? null;
  }

  private readClipboard(): RegisterContent | null {
    let text = this.fresh;
    if (text === null) {
      const read = this.clipboard!.read();
      if (typeof read !== "string") throw new ClipboardPending(read);
      text = read;
    }
    if (!text) return null;
    if (this.lastClip && this.lastClip.text === text) return this.lastClip;
    return { text, kind: text.endsWith("\n") ? "line" : "char" };
  }

  /**
   * Stores a yank (`yank`) or a delete (`delete`) the way Vim does: the named
   * register if one was given, else `"0` for a yank and `"1`–`"9` or `"-` for
   * a delete — and always the unnamed register, unless it went to `"_`.
   */
  put(name: string | null, content: RegisterContent, how: "yank" | "delete"): void {
    // The black hole keeps nothing; the read-only registers take nothing (their owners write them).
    if (name === "_" || (name !== null && READ_ONLY.has(name))) return;
    if (name && APPEND.test(name)) {
      const lower = name.toLowerCase();
      const before = this.store.get(lower);
      const joined = before ? append(before, content) : content;
      this.store.set(lower, joined);
      this.setUnnamed(joined);
      return;
    }
    if (name && (NAMED.test(name) || NUMBERED.test(name))) this.store.set(name, content);
    else if (name === "+" || name === "*") this.writeClipboard(content);
    else if (how === "yank") this.store.set("0", content);
    else if (content.kind === "line" || content.text.includes("\n")) this.shiftDeletes(content);
    else this.store.set("-", content);
    this.setUnnamed(content);
  }

  private shiftDeletes(content: RegisterContent): void {
    for (let n = 9; n > 1; n--) {
      const prev = this.store.get(String(n - 1));
      if (prev) this.store.set(String(n), prev);
    }
    this.store.set("1", content);
  }

  private setUnnamed(content: RegisterContent): void {
    this.store.set('"', content);
    if (this.clipboard && this.shared) this.writeClipboard(content);
  }

  private writeClipboard(content: RegisterContent): void {
    this.lastClip = content;
    this.clipboard?.write(content.text);
  }

  /** Sets a register directly — a recorded macro (`q`), or a test. */
  set(name: string, content: RegisterContent): void {
    if (APPEND.test(name)) {
      const lower = name.toLowerCase();
      const before = this.store.get(lower);
      this.store.set(lower, before ? append(before, content) : content);
    } else {
      this.store.set(name, content);
    }
  }
}

/** `b` added to `a`, as appending to a register (`"Ayw`) does. */
function append(a: RegisterContent, b: RegisterContent): RegisterContent {
  if (a.kind === "line" || b.kind === "line") {
    const head = a.text.endsWith("\n") ? a.text : `${a.text}\n`;
    return { text: head + (b.text.endsWith("\n") ? b.text : `${b.text}\n`), kind: "line" };
  }
  return { text: a.text + b.text, kind: a.kind };
}
