/**
 * The code-action menu's behaviour (`docs/plans/editor.md`, ED6.7, L20), apart
 * from how it is drawn (`CodeActionPopup.svelte`) and which keys reach it
 * (`EditorSurface.svelte`):
 *
 * * **It opens on what the server offered** for a place, in the menu's order
 *   (`orderActions`), the server's preferred action chosen — else the first
 *   one that can be taken.
 * * **Typed letters narrow it** unscharf (as the owner's Telescope dropdown);
 *   ⌫ takes the last one back.
 * * **1–9 take the entry with that number at once**; ⏎ takes the chosen one.
 *   An entry the server marked as not available (`disabled`) is shown but
 *   never taken.
 */

import { filterActions, orderActions, type CodeActionItem } from "../editor/lsp/codeActions";
import type { Diagnostic } from "../editor/lsp/diagnostics";
import type { Pos } from "../editor/position";

/** What the surface's menu needs from the owner (the editor around it, which has the server). */
export interface CodeActionPort {
  /** The actions for `from`..`to`; with `fix`, only the ones for that problem (the hover's "Fix…"). */
  ask(from: Pos, to: Pos, fix: Diagnostic | null): Promise<CodeActionItem[]>;
  /** Carries out a taken action (resolve, edit, command). */
  take(item: CodeActionItem): void;
  /** There was nothing to offer: the owner says so. */
  none(): void;
}

/** Entries that get a number to take them with (1–9). */
export const NUMBERED = 9;

export interface ActionMenuView {
  /** Where the menu is drawn below: the start of the range asked about. */
  anchor: Pos;
  items: CodeActionItem[];
  selected: number;
  /** What was typed to narrow the list. */
  query: string;
}

interface Open {
  anchor: Pos;
  all: CodeActionItem[];
  shown: CodeActionItem[];
  selected: number;
  query: string;
}

/** The entry to choose in `items`: the preferred one, else the first that can be taken, else the first. */
export function initialChoice(items: readonly CodeActionItem[]): number {
  const preferred = items.findIndex((item) => item.preferred && item.disabled === null);
  if (preferred >= 0) return preferred;
  return Math.max(0, items.findIndex((item) => item.disabled === null));
}

export class CodeActionMenu {
  private open: Open | null = null;

  constructor(private readonly changed: () => void) {}

  get view(): ActionMenuView | null {
    const o = this.open;
    return o ? { anchor: o.anchor, items: o.shown, selected: o.selected, query: o.query } : null;
  }

  get isOpen(): boolean {
    return this.open !== null;
  }

  /** Opens on `items` below `anchor`; with nothing to show it stays closed (`false`). */
  show(anchor: Pos, items: readonly CodeActionItem[]): boolean {
    const all = orderActions(items);
    if (all.length === 0) {
      this.close();
      return false;
    }
    this.open = { anchor, all, shown: all, selected: initialChoice(all), query: "" };
    this.changed();
    return true;
  }

  close(): void {
    if (!this.open) return;
    this.open = null;
    this.changed();
  }

  /** ↓/↑, ⌃N/⌃P: the next or previous entry, wrapping round. */
  move(delta: number): void {
    const o = this.open;
    if (!o || o.shown.length === 0) return;
    const n = o.shown.length;
    o.selected = (((o.selected + delta) % n) + n) % n;
    this.changed();
  }

  /** A typed character narrows the list; ⌫ (`null`) takes the last one back. */
  narrow(char: string | null): void {
    const o = this.open;
    if (!o) return;
    const query = char === null ? o.query.slice(0, -1) : o.query + char;
    if (query === o.query) return;
    o.query = query;
    o.shown = filterActions(o.all, query);
    o.selected = query === "" ? initialChoice(o.shown) : Math.max(0, o.shown.findIndex((i) => i.disabled === null));
    this.changed();
  }

  /**
   * Takes the chosen entry — or entry `number` (1–9) of the list as shown —
   * and closes the menu; `null` (and the menu stays) when there is none or
   * it is not available.
   */
  take(number: number | null = null): CodeActionItem | null {
    const o = this.open;
    if (!o) return null;
    const index = number === null ? o.selected : number - 1;
    const item = o.shown[index];
    if (!item || item.disabled !== null) return null;
    this.close();
    return item;
  }
}
