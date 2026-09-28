/**
 * The completion menu's behaviour (`docs/plans/editor.md`, ED6.4), apart from
 * how it is drawn (`CompletionPopup.svelte`) and which keys drive it
 * (`EditorSurface.svelte`). Modelled on the owner's Neovim setup (blink.cmp):
 *
 * * **It opens by itself while typing** a word, or after one of the server's
 *   trigger characters (`.`, `::`), and on ⌃Space; the first item is chosen.
 * * **It follows the word**: every further character narrows the list
 *   (`filterItems`), and a server that said its list was incomplete is asked
 *   again. Leaving the word — another line, before its start, a character
 *   that is not part of a word — closes it.
 * * **Answers that come late are dropped**: every request carries a token,
 *   and only the newest one's answer is shown.
 * * **Taking an item** resolves it first when it has no extra edits yet (an
 *   auto-import is often filled in only then), within a short limit, and
 *   returns the edit — the surface runs it as the `complete` command.
 */

import type { EditorDocument } from "../editor/document";
import {
  completionEdit,
  filterItems,
  type CompletionAnswer,
  type CompletionEdit,
  type CompletionItem,
} from "../editor/lsp/completion";
import { pos, type Pos } from "../editor/position";
import { leadingWhitespace } from "../editor/text";

/** How long taking an item waits for its extra edits before going without them. */
export const RESOLVE_LIMIT_MS = 400;
/** Most items shown; the list narrows as the word grows. */
export const MAX_SHOWN = 200;

/** A character that belongs to a word being completed. */
const WORD_CHAR = /[\p{L}\p{N}_$]/u;

export interface CompletionPort {
  ask(at: Pos, trigger: string | null, again: boolean): Promise<CompletionAnswer>;
  resolve(item: CompletionItem): Promise<CompletionItem>;
  /** The server's trigger characters. */
  triggers(): readonly string[];
}

export interface MenuView {
  /** Where the word being completed begins — the menu is drawn below it. */
  anchor: Pos;
  items: CompletionItem[];
  selected: number;
}

interface Open {
  anchor: Pos;
  all: CompletionItem[];
  shown: CompletionItem[];
  selected: number;
  incomplete: boolean;
}

/** Where the word before `at` begins on its line. */
export function wordStart(line: string, col: number): number {
  let start = col;
  while (start > 0 && WORD_CHAR.test(line[start - 1])) start--;
  return start;
}

export class CompletionMenu {
  private open: Open | null = null;
  private token = 0;
  private readonly resolved = new Map<CompletionItem, Promise<CompletionItem>>();

  constructor(
    private readonly port: CompletionPort,
    private readonly changed: () => void,
  ) {}

  /** What to draw, or `null` when closed. */
  get view(): MenuView | null {
    const o = this.open;
    return o ? { anchor: o.anchor, items: o.shown, selected: o.selected } : null;
  }

  get isOpen(): boolean {
    return this.open !== null;
  }

  /**
   * Text was typed at the cursor: opens the menu on a word or a trigger
   * character, or narrows (or closes) an open one.
   */
  typed(doc: EditorDocument, text: string): void {
    if (this.open) {
      this.follow(doc);
      // Still in the word: done. Left it with a trigger character (`foo.`): a new list below.
      if (this.open) return;
    }
    const last = text.slice(-1);
    if (last === "") return;
    const triggers = this.port.triggers();
    const trigger = triggers.find((t) => t !== "" && lineBefore(doc).endsWith(t)) ?? null;
    if (trigger !== null && !WORD_CHAR.test(last)) return void this.request(doc, trigger, false);
    if (WORD_CHAR.test(last)) void this.request(doc, null, false);
  }

  /** ⌃Space: opens the menu here, word or not. */
  invoke(doc: EditorDocument): void {
    void this.request(doc, null, false);
  }

  /** The text or the cursor changed while open (a ⌫, an arrow): narrow the list, or close. */
  follow(doc: EditorDocument): void {
    const o = this.open;
    if (!o) return;
    const query = this.queryAt(doc, o.anchor);
    if (query === null) return this.close();
    if (o.incomplete) {
      void this.request(doc, null, true, o.anchor);
      return;
    }
    this.show(o, query);
  }

  /** ↓/↑, ⌃N/⌃P: another item, round the ends. */
  move(delta: number): void {
    const o = this.open;
    if (!o || o.shown.length === 0) return;
    o.selected = (o.selected + delta + o.shown.length) % o.shown.length;
    this.changed();
  }

  close(): void {
    if (!this.open) return;
    this.open = null;
    this.token++;
    this.resolved.clear();
    this.changed();
  }

  /** The chosen item with what the server fills in on request (its documentation). */
  details(item: CompletionItem): Promise<CompletionItem> {
    let found = this.resolved.get(item);
    if (!found) this.resolved.set(item, (found = this.port.resolve(item)));
    return found;
  }

  /**
   * Takes the chosen item: the edit to run, or `null` when there is none or
   * the text changed while its extra edits were being fetched. Closes the menu.
   */
  async take(doc: EditorDocument): Promise<CompletionEdit | null> {
    const o = this.open;
    const item = o?.shown[o.selected];
    // Taken before closing, which forgets them: a resolve the documentation already started is reused.
    const details = item && item.additionalEdits.length === 0 ? this.details(item) : null;
    this.close();
    if (!o || !item) return null;
    const revision = doc.revision;
    let final = item;
    if (details) {
      const limit = new Promise<CompletionItem>((resolve) => setTimeout(() => resolve(item), RESOLVE_LIMIT_MS));
      final = await Promise.race([details, limit]);
      if (doc.revision !== revision) return null;
    }
    const at = doc.selection.head;
    if (at.line !== o.anchor.line || at.col < o.anchor.col) return null;
    const indent = leadingWhitespace(doc.store.line(at.line));
    return completionEdit(final, at, o.anchor.col, indent);
  }

  /** The word typed since `anchor`, or `null` when the cursor left it. */
  private queryAt(doc: EditorDocument, anchor: Pos): string | null {
    const sel = doc.selection;
    const at = sel.head;
    if (doc.extra.length > 0 || at.line !== anchor.line || at.col < anchor.col) return null;
    if (sel.anchor.line !== at.line || sel.anchor.col !== at.col) return null;
    const typed = doc.store.line(at.line).slice(anchor.col, at.col);
    for (const ch of typed) if (!WORD_CHAR.test(ch)) return null;
    return typed;
  }

  private async request(doc: EditorDocument, trigger: string | null, again: boolean, keep?: Pos): Promise<void> {
    if (doc.extra.length > 0) return;
    const at = doc.selection.head;
    const anchor = keep ?? pos(at.line, trigger !== null ? at.col : wordStart(doc.store.line(at.line), at.col));
    const token = ++this.token;
    const answer = await this.port.ask(at, trigger, again);
    if (token !== this.token) return;
    const query = this.queryAt(doc, anchor);
    if (query === null || answer.items.length === 0) {
      if (this.open) this.close();
      return;
    }
    const open: Open = { anchor, all: answer.items, shown: [], selected: 0, incomplete: answer.incomplete };
    this.open = open;
    this.resolved.clear();
    this.show(open, query);
  }

  private show(o: Open, query: string): void {
    o.shown = filterItems(o.all, query).slice(0, MAX_SHOWN);
    if (o.shown.length === 0) return this.close();
    const preselected = query === "" ? o.shown.findIndex((i) => i.preselect) : -1;
    o.selected = Math.max(0, preselected);
    this.changed();
  }
}

/** The cursor's line up to the cursor. */
function lineBefore(doc: EditorDocument): string {
  const at = doc.selection.head;
  return doc.store.line(at.line).slice(0, at.col);
}
