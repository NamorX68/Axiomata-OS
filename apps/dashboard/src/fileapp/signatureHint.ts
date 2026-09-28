/**
 * The signature hint's behaviour (`docs/plans/editor.md`, ED6.6), apart from
 * how it is drawn (`EditorSurface.svelte`):
 *
 * * **It opens by itself** when one of the server's trigger characters is
 *   typed (`(`, `,` — a call's arguments begin), and on ⇧⌘Space.
 * * **While open it follows**: every further character or cursor move asks
 *   again (the active parameter changes after a `,`); the server answering
 *   with nothing — the cursor left the call — closes it, as does leaving the
 *   line, leaving Insert mode, Esc, a click.
 * * **Late answers are dropped** (a token per request).
 */

import type { EditorDocument } from "../editor/document";
import type { SignatureView } from "../editor/lsp/signature";
import type { Pos } from "../editor/position";

export interface SignaturePort {
  ask(at: Pos, trigger: string | null, retrigger: boolean): Promise<SignatureView | null>;
  /** The characters that open the hint, and ones that refresh it while open. */
  triggers(): readonly string[];
  retriggers(): readonly string[];
}

export interface HintView {
  signature: SignatureView;
  /** Where it was asked — the hint shows above this line. */
  at: Pos;
}

export class SignatureHint {
  private open: HintView | null = null;
  private token = 0;

  constructor(
    private readonly port: SignaturePort,
    private readonly changed: () => void,
  ) {}

  get view(): HintView | null {
    return this.open;
  }

  get isOpen(): boolean {
    return this.open !== null;
  }

  /** Text was typed at the cursor: a trigger character opens the hint, anything refreshes an open one. */
  typed(doc: EditorDocument, text: string): void {
    const last = text.slice(-1);
    if (last === "") return;
    if (this.port.triggers().includes(last)) return void this.request(doc, last, this.open !== null);
    if (this.open) {
      const retrigger = this.port.retriggers().includes(last) ? last : null;
      void this.request(doc, retrigger, true);
    }
  }

  /** ⇧⌘Space: the signature of the call the cursor is in, if any. */
  invoke(doc: EditorDocument): void {
    void this.request(doc, null, this.open !== null);
  }

  /** The cursor moved or text was deleted while open: ask again, or close when the line was left. */
  follow(doc: EditorDocument): void {
    const o = this.open;
    if (!o) return;
    if (doc.selection.head.line !== o.at.line || doc.extra.length > 0) return this.close();
    void this.request(doc, null, true);
  }

  close(): void {
    this.token++;
    if (!this.open) return;
    this.open = null;
    this.changed();
  }

  private async request(doc: EditorDocument, trigger: string | null, retrigger: boolean): Promise<void> {
    if (doc.extra.length > 0) return;
    const at = doc.selection.head;
    const token = ++this.token;
    const signature = await this.port.ask(at, trigger, retrigger);
    if (token !== this.token) return;
    const now = doc.selection.head;
    if (!signature || now.line !== at.line) {
      if (this.open) {
        this.open = null;
        this.changed();
      }
      return;
    }
    this.open = { signature, at: now };
    this.changed();
  }
}
