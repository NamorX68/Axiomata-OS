/**
 * The command line of `:`, `/` and `?` (`docs/plans/editor.md`, ED3, V6, V7):
 * one line of text with its own cursor, edited by keys the machine hands it.
 *
 * * Typing inserts; `<BS>` on an empty line leaves, as in Vim.
 * * `<Left>` `<Right>` `<Home>` `<End>` move; `<C-w>` and `<C-u>` delete the
 *   word and everything before the cursor; ⌥⌫/⌘⌫ and ⌘←/⌘→ from the Mac map do the same.
 * * `<Up>`/`<Down>` walk the history, keeping only entries that begin with
 *   what was typed; `<Tab>`/`<S-Tab>` cycle the completions the owner gives.
 * * `<C-r>{register}` inserts a register's text (the machine resolves it).
 *
 * The line reports back what happened; running it is the machine's.
 */

import type { Command } from "../commands";
import type { ViKey } from "./keys";
import { isWordChar } from "./search";

export type CmdlineKind = ":" | "/" | "?";

/** What a key did to the line. */
export type CmdlineResult =
  | { kind: "edited" }
  | { kind: "submit"; text: string }
  | { kind: "cancel" }
  /** `<C-r>` then this register name: the machine inserts its text with {@link insert}. */
  | { kind: "register"; name: string };

export class CommandLine {
  text: string;
  /** Index in `text` (UTF-16). */
  cursor: number;
  private historyAt = -1;
  private historyPrefix = "";
  private completions: string[] | null = null;
  private completionAt = -1;
  private completionBase = "";
  private awaitingRegister = false;

  constructor(
    readonly kind: CmdlineKind,
    initial: string,
    private readonly history: readonly string[],
    private readonly complete: (text: string) => string[] = () => [],
  ) {
    this.text = initial;
    this.cursor = initial.length;
  }

  /** Whether `<C-r>` is waiting for its register name (the pill shows `"`). */
  get pendingRegister(): boolean {
    return this.awaitingRegister;
  }

  /** `<C-r>` again: its register could not be read yet, and the name will be fed once more. */
  expectRegister(): void {
    this.awaitingRegister = true;
  }

  /** Inserts `text` at the cursor. */
  insert(text: string): void {
    // A line break cannot be part of one command line.
    const clean = text.replace(/\r?\n/g, " ");
    this.text = this.text.slice(0, this.cursor) + clean + this.text.slice(this.cursor);
    this.cursor += clean.length;
    this.resetWalks();
  }

  /** One key: edits the line, or says it was submitted, cancelled or wants a register. */
  key(key: ViKey): CmdlineResult {
    if (this.awaitingRegister) {
      this.awaitingRegister = false;
      if (typeof key === "string" && key.length === 1) return { kind: "register", name: key };
      return { kind: "edited" };
    }
    if (typeof key !== "string") {
      if ("text" in key) this.insert(key.text);
      else this.macCommand(key.command);
      return { kind: "edited" };
    }
    switch (key) {
      case "<CR>":
        return { kind: "submit", text: this.text };
      case "<Esc>":
      case "<C-[>":
      case "<C-c>":
        return { kind: "cancel" };
      case "<BS>":
        if (this.text === "") return { kind: "cancel" };
        this.deleteBack(1);
        return { kind: "edited" };
      case "<Del>":
        this.text = this.text.slice(0, this.cursor) + this.text.slice(this.cursor + 1);
        this.resetWalks();
        return { kind: "edited" };
      case "<Left>":
        this.cursor = Math.max(0, this.cursor - 1);
        return { kind: "edited" };
      case "<Right>":
        this.cursor = Math.min(this.text.length, this.cursor + 1);
        return { kind: "edited" };
      case "<Home>":
      case "<C-b>":
        this.cursor = 0;
        return { kind: "edited" };
      case "<End>":
      case "<C-e>":
        this.cursor = this.text.length;
        return { kind: "edited" };
      case "<C-w>":
        this.deleteBack(this.cursor - wordStartBefore(this.text, this.cursor));
        return { kind: "edited" };
      case "<C-u>":
        this.deleteBack(this.cursor);
        return { kind: "edited" };
      case "<C-r>":
        this.awaitingRegister = true;
        return { kind: "edited" };
      case "<Up>":
      case "<Down>":
        this.walkHistory(key === "<Up>" ? 1 : -1);
        return { kind: "edited" };
      case "<Tab>":
      case "<S-Tab>":
        this.cycleCompletion(key === "<Tab>" ? 1 : -1);
        return { kind: "edited" };
    }
    // Any other special key (`<C-x>`, `<PageUp>` …) does nothing here; a character is typed.
    if (key.length > 1 && key.startsWith("<")) return { kind: "edited" };
    this.insert(key);
    return { kind: "edited" };
  }

  private macCommand(cmd: Command): void {
    if (cmd.type === "insert") this.insert(cmd.text);
    else if (cmd.type === "deleteBackward") {
      const word = this.cursor - wordStartBefore(this.text, this.cursor);
      this.deleteBack(cmd.unit === "line" ? this.cursor : cmd.unit === "word" ? word : 1);
    } else if (cmd.type === "move") {
      if (cmd.motion === "lineStart" || cmd.motion === "docStart") this.cursor = 0;
      else if (cmd.motion === "lineEnd" || cmd.motion === "docEnd") this.cursor = this.text.length;
      else if (cmd.motion === "charLeft") this.cursor = Math.max(0, this.cursor - 1);
      else if (cmd.motion === "charRight") this.cursor = Math.min(this.text.length, this.cursor + 1);
      else if (cmd.motion === "wordLeft") this.cursor = wordStartBefore(this.text, this.cursor);
      else if (cmd.motion === "wordRight") this.cursor = wordEndAfter(this.text, this.cursor);
    }
  }

  private deleteBack(n: number): void {
    if (n <= 0) return;
    const from = Math.max(0, this.cursor - n);
    this.text = this.text.slice(0, from) + this.text.slice(this.cursor);
    this.cursor = from;
    this.resetWalks();
  }

  private resetWalks(): void {
    this.historyAt = -1;
    this.completions = null;
  }

  /** `<Up>` (1) goes back in time, `<Down>` (-1) forward, among entries starting with the typed prefix. */
  private walkHistory(dir: 1 | -1): void {
    if (this.historyAt === -1) this.historyPrefix = this.text;
    let at = this.historyAt;
    for (;;) {
      at += dir;
      if (at < 0) {
        // Past the newest entry: back to what was typed.
        this.historyAt = -1;
        this.setText(this.historyPrefix);
        return;
      }
      if (at >= this.history.length) return;
      const entry = this.history[this.history.length - 1 - at];
      if (entry.startsWith(this.historyPrefix)) {
        this.historyAt = at;
        this.setText(entry);
        return;
      }
    }
  }

  private cycleCompletion(dir: 1 | -1): void {
    if (this.completions === null) {
      this.completionBase = this.text;
      this.completions = this.complete(this.text);
      this.completionAt = -1;
    }
    const n = this.completions.length;
    if (n === 0) return;
    // One step past either end shows what was typed again.
    let at = this.completionAt + dir;
    if (at >= n) at = -1;
    else if (at < -1) at = n - 1;
    this.completionAt = at;
    this.setText(at === -1 ? this.completionBase : this.completions[at]);
  }

  private setText(text: string): void {
    this.text = text;
    this.cursor = text.length;
  }
}

/** Where `<C-w>` deletes back to: spaces, then one word or one run of other characters. */
function wordStartBefore(text: string, at: number): number {
  let i = at;
  while (i > 0 && /\s/.test(text[i - 1])) i--;
  if (i > 0 && isWordChar(text[i - 1])) while (i > 0 && isWordChar(text[i - 1])) i--;
  else if (i > 0) i--;
  return i;
}

function wordEndAfter(text: string, at: number): number {
  let i = at;
  while (i < text.length && !isWordChar(text[i])) i++;
  while (i < text.length && isWordChar(text[i])) i++;
  return i;
}
