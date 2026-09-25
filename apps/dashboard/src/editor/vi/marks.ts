/**
 * Marks and the jump list of one document (`docs/plans/editor.md`, ED3, D17).
 *
 * * Marks `a`–`z` are set with `m`; the machine keeps its own too: `'` (before
 *   the last jump), `.` (the last change), `^` (where Insert mode ended),
 *   `[`/`]` (the last changed or yanked text), `<`/`>` (the last Visual
 *   selection).
 * * The jump list holds the places a jump (`G`, `%`, `'a`, a search …) left;
 *   Ctrl-o walks back along it, Ctrl-i forward again.
 * * Both follow the text: lines added or removed above a mark move it, so it
 *   stays on its line (a mark inside removed lines lands where they began).
 */

import type { TextChange } from "../document";
import { pos, posEqual, type Pos } from "../position";

export class Marks {
  private readonly marks = new Map<string, Pos>();
  private jumps: Pos[] = [];
  /** Where Ctrl-o/Ctrl-i are in `jumps`; `jumps.length` means "at the newest". */
  private index = 0;

  get(name: string): Pos | null {
    return this.marks.get(name === "`" ? "'" : name) ?? null;
  }

  set(name: string, at: Pos): void {
    this.marks.set(name === "`" ? "'" : name, at);
  }

  /** Before a jump: remember where it started (and make that the `''` mark). */
  pushJump(from: Pos): void {
    this.set("'", from);
    this.jumps = this.jumps.slice(0, this.index).filter((p) => p.line !== from.line);
    this.jumps.push(from);
    this.index = this.jumps.length;
  }

  /** Ctrl-o: `n` places back, starting from `here`; `null` if there are not that many. */
  older(n: number, here: Pos): Pos | null {
    if (this.index === this.jumps.length) {
      const newest = this.jumps[this.jumps.length - 1];
      if (!newest || !posEqual(newest, here)) this.jumps.push(here);
      this.index = this.jumps.length - 1;
    }
    const target = this.index - n;
    if (target < 0) return null;
    this.index = target;
    return this.jumps[target];
  }

  /** Ctrl-i: `n` places forward again; `null` past the newest. */
  newer(n: number): Pos | null {
    const target = this.index + n;
    if (target >= this.jumps.length) return null;
    this.index = target;
    return this.jumps[target];
  }

  /** Moves marks and jumps with lines added or removed above them. */
  follow(change: TextChange): void {
    const delta = change.newEnd.line - change.oldEnd.line;
    if (delta === 0) return;
    const shift = (p: Pos): Pos => {
      if (p.line > change.oldEnd.line) return pos(p.line + delta, p.col);
      if (p.line > change.start.line) return pos(change.start.line, 0);
      return p;
    };
    for (const [name, p] of this.marks) this.marks.set(name, shift(p));
    this.jumps = this.jumps.map(shift);
  }
}
