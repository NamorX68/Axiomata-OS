/**
 * The app-wide jump history across files (`docs/plans/tags-links.md`, Q2): every jump into another file — a followed
 * wikilink, a definition elsewhere — leaves the place it started from here, and ⌃- goes back along it, ⌃⇧- forward
 * again. Vi's `<C-o>`/`<C-i>` fall through to it once the document's own jump list (`editor/vi/marks.ts`) has nothing
 * further. One history for every editor of the app, as in a browser: going back opens the file where it is wanted.
 *
 * It walks like Vi's jump list: going back from the newest end first remembers where the cursor is, so forward
 * returns there; a new jump drops what was ahead.
 */

import type { Platform } from "../core/keys";

/** A place in a file: its root, its path, and the cursor's line and column. */
export interface JumpPlace {
  root: string;
  rel: string;
  line: number;
  col: number;
}

/** Most places kept; the oldest go first. */
export const MAX_JUMPS = 100;

const samePlace = (a: JumpPlace, b: JumpPlace) =>
  a.root === b.root && a.rel === b.rel && a.line === b.line && a.col === b.col;

export class JumpHistory {
  private places: JumpPlace[] = [];
  /** Where back/forward are in `places`; `places.length` means "at the newest end". */
  private index = 0;

  /** Before a jump into another file: remember where it started. */
  record(from: JumpPlace): void {
    this.places = this.places.slice(0, this.index);
    const last = this.places[this.places.length - 1];
    if (!last || !samePlace(last, from)) this.places.push(from);
    if (this.places.length > MAX_JUMPS) this.places = this.places.slice(-MAX_JUMPS);
    this.index = this.places.length;
  }

  /** One place back from `here`; `null` when there is none. */
  back(here: JumpPlace | null): JumpPlace | null {
    if (this.index === this.places.length && here) {
      const last = this.places[this.places.length - 1];
      if (!last || !samePlace(last, here)) this.places.push(here);
      this.index = this.places.length - 1;
    }
    if (this.index - 1 < 0) return null;
    this.index--;
    return this.places[this.index];
  }

  /** One place forward again; `null` past the newest. */
  forward(): JumpPlace | null {
    if (this.index + 1 >= this.places.length) return null;
    this.index++;
    return this.places[this.index];
  }
}

/** The one history of the app. */
export const jumpHistory = new JumpHistory();

/**
 * The keys that walk the history: ⌃- / ⌃⇧- on the Mac; elsewhere Ctrl+Alt+← / Ctrl+Alt+→ — Ctrl+- zooms there, Alt+←/→
 * moves by words, and Ctrl+Alt+←/→ is the one pair the editor leaves free (`editorKeys.ts`).
 */
export function jumpKey(
  e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
  on: Platform,
): "back" | "forward" | null {
  if (on === "mac") {
    if (!e.ctrlKey || e.metaKey || e.altKey) return null;
    if (e.key === "-" && !e.shiftKey) return "back";
    // ⇧ turns the minus key into `_` on the usual layouts.
    if ((e.key === "_" || e.key === "-") && e.shiftKey) return "forward";
    return null;
  }
  if (!e.altKey || !e.ctrlKey || e.metaKey || e.shiftKey) return null;
  if (e.key === "ArrowLeft") return "back";
  if (e.key === "ArrowRight") return "forward";
  return null;
}
