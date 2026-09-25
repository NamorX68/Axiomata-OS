/**
 * The Vi machine, attached to one editor surface (`docs/plans/editor.md`, ED3.2,
 * V1, V5, V8, V12): it turns the surface's key presses, typed text and dead keys
 * into machine keys, and the machine's state back into what the surface draws —
 * the cursor's shape, the Visual selection, the mode pill.
 *
 * Kept out of `EditorSurface.svelte` so the surface stays about drawing; this
 * class is about meaning. Effects the surface can carry out itself (scrolling,
 * the bell) come back to it through `host`; the rest go to the owner.
 */

import type { KeyInput, Effect } from "../editor/keymap";
import type { EditorDocument } from "../editor/document";
import { parseKeys, type ViKey } from "../editor/vi/keys";
import { ViMachine, type ViEffect, type ViMode, type ViShared, type SyntaxObjects } from "../editor/vi/machine";
import type { ViContext } from "../editor/vi/motions";
import { blockCols } from "../editor/vi/ops";
import { pos, range, type Pos, type Range } from "../editor/position";
import { viKeyFor } from "./viKeys";

/** What the mode pill shows (V8). */
export interface ViStatus {
  mode: ViMode;
  pending: string;
  recording: string | null;
}

export interface ViSurfaceHost {
  ctx(): ViContext;
  /** Redraw after the machine changed text or cursor. */
  changed(): void;
  /** Everything the machine asks the view to do. */
  effect(effect: ViEffect): void;
  status(status: ViStatus): void;
  readOnly: boolean;
  fileName?: string;
  fileKey?: string;
  syntaxObjects?: SyntaxObjects;
}

/** How `keydown` went: `handled` means the browser's default must not happen. */
export type ViKeyResult = { handled: true } | { handled: false } | { handled: true; effect: Effect };

export class ViSurface {
  readonly machine: ViMachine;
  /** A dead key's composition was taken as a key; its end must not type it again. */
  private swallowComposition = false;

  constructor(
    doc: EditorDocument,
    shared: ViShared,
    private readonly host: ViSurfaceHost,
  ) {
    this.machine = new ViMachine(doc, shared, {
      ctx: () => host.ctx(),
      effect: (e) => host.effect(e),
      readOnly: host.readOnly,
      fileName: host.fileName,
      fileKey: host.fileKey,
      syntaxObjects: host.syntaxObjects,
    });
  }

  dispose(): void {
    this.machine.dispose();
  }

  /** Insert or Replace mode: typed text is text, and an input method may compose. */
  get typing(): boolean {
    return this.machine.mode === "insert" || this.machine.mode === "replace";
  }

  private feed(keys: readonly ViKey[]): void {
    for (const key of keys) this.machine.feed(key);
    this.host.changed();
    this.host.status(this.machine.status());
  }

  /** A key press. Characters are left to the textarea (V12) and come back through `typed`. */
  keydown(input: KeyInput): ViKeyResult {
    this.swallowComposition = false;
    const decision = viKeyFor(input, this.machine.mode);
    if (decision === null || decision.kind === "text") return { handled: false };
    if (decision.kind === "effect") return { handled: true, effect: decision.effect };
    this.feed(decision.kind === "key" ? [decision.key] : parseKeys(decision.notation));
    return { handled: true };
  }

  /** Text the textarea received: typed text in Insert mode, keys in the others. */
  typed(text: string): void {
    if (!text) return;
    if (this.swallowComposition) {
      this.swallowComposition = false;
      return;
    }
    this.feed(this.typing ? [{ text }] : [...text]);
  }

  /**
   * A dead key began composing (`^` on a German keyboard). Outside Insert mode
   * there is nothing to compose: its character is taken as the key at once
   * (V12), and the caller cancels the composition. Returns whether it did.
   */
  deadKey(data: string): boolean {
    if (this.typing || !data) return false;
    this.swallowComposition = true;
    this.feed([...data]);
    return true;
  }

  /** The cursor's shape for the surface (V8). */
  cursorShape(): "block" | "bar" | "underline" {
    return this.machine.cursorShape();
  }

  /** The Visual selection as ranges to draw — one per line for a block (V2); `null` outside Visual mode. */
  visualRanges(doc: EditorDocument, tabSize: number): Range[] | null {
    const region = this.machine.visualRegion();
    if (!region) return null;
    const store = doc.store;
    if (region.kind === "char") return [range(region.start, region.end)];
    if (region.kind === "line") return [range(pos(region.first, 0), pos(region.last, store.line(region.last).length))];
    const out: Range[] = [];
    for (let l = region.first; l <= region.last; l++) {
      const [s, e] = blockCols(store.line(l), region, tabSize);
      out.push(range(pos(l, s), pos(l, Math.max(s, e))));
    }
    return out;
  }

  /** Whether a Visual line selection is showing — drawn to the full width, not just the text. */
  get lineVisual(): boolean {
    return this.machine.mode === "visualLine";
  }

  placeCursor(at: Pos): void {
    this.machine.placeCursor(at);
    this.host.status(this.machine.status());
  }

  selectVisual(anchor: Pos, head: Pos): void {
    this.machine.selectVisual(anchor, head);
    this.host.status(this.machine.status());
  }

  /** Text pasted with the Mac's paste (⌘V in Insert, Edit ▸ Paste). */
  pasted(text: string): void {
    if (this.typing) this.feed([{ text }]);
  }

  status(): ViStatus {
    return this.machine.status();
  }
}
