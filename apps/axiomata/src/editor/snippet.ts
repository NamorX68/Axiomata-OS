/**
 * Jumping through a snippet's tab stops (board card #36, the rest of L9): after
 * a snippet is inserted, ⇥ selects the next placeholder and ⇧⇥ the one before,
 * so typing replaces it — as LuaSnip / blink.cmp in the owner's Neovim. No DOM.
 *
 * * **The stops follow the text**: every change moves them (`mapPos`); the
 *   stop being edited grows with what is typed at its end, so a replaced
 *   placeholder stays one stop however long the new text gets.
 * * **The session ends** at the last stop (`$0`, or the snippet's end), when
 *   the cursor leaves every stop, or when the caller ends it (Esc, another
 *   completion). A repeated `$1` is not mirrored.
 */
import type { EditorDocument, TextChange } from "./document";
import { mapPos } from "./multicursor";
import { comparePos, pos, range, selectionRange, type Pos, type Range, type Selection } from "./position";

/** `p` after `change`, counting an insertion right at `p` as before it — the end of the stop being typed into. */
function mapEndInclusive(p: Pos, change: TextChange): Pos {
  if (comparePos(p, change.start) < 0) return p;
  if (comparePos(p, change.oldEnd) <= 0) return change.newEnd;
  if (p.line === change.oldEnd.line) return pos(change.newEnd.line, change.newEnd.col + (p.col - change.oldEnd.col));
  return pos(p.line + change.newEnd.line - change.oldEnd.line, p.col);
}

export class SnippetSession {
  private stops: Range[];
  private index = 0;
  private readonly unsubscribe: () => void;

  /** `stops` in jump order, where they are in `doc` right now; the first is the current one. */
  constructor(doc: EditorDocument, stops: readonly Range[]) {
    this.stops = [...stops];
    this.unsubscribe = doc.onTextChange((change) => this.follow(change));
  }

  /** The stop being edited. */
  get current(): Range | null {
    return this.stops[this.index] ?? null;
  }

  /** Whether `selection` is still inside one of the stops (their edges included). */
  holds(selection: Selection): boolean {
    const r = selectionRange(selection);
    return this.stops.some((s) => comparePos(s.start, r.start) <= 0 && comparePos(r.end, s.end) <= 0);
  }

  /** The next stop to select, or `null` at the end; at the last stop the caller ends the session. */
  next(): Range | null {
    if (this.index + 1 >= this.stops.length) return null;
    this.index++;
    return this.stops[this.index];
  }

  /** The stop before, or `null` at the first. */
  previous(): Range | null {
    if (this.index === 0) return null;
    this.index--;
    return this.stops[this.index];
  }

  /** Whether the current stop is the last one. */
  get atLast(): boolean {
    return this.index >= this.stops.length - 1;
  }

  dispose(): void {
    this.unsubscribe();
  }

  private follow(change: TextChange): void {
    const activeEnd = this.stops[this.index]?.end;
    this.stops = this.stops.map((s, i) => {
      if (i === this.index) return range(mapPos(s.start, change), mapEndInclusive(s.end, change));
      // A stop right at the active one's end (`${1:x}$0`, the implicit end after the last placeholder) stays after
      // what is typed there, not before it.
      const touches = activeEnd !== undefined && comparePos(s.start, activeEnd) === 0;
      const map = touches ? mapEndInclusive : mapPos;
      return range(map(s.start, change), map(s.end, change));
    });
  }
}
