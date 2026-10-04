/**
 * Syntax colours for a diff document (`docs/plans/git-layer.md`, H2): each row
 * takes its colours from the highlighter of the side it shows, which has parsed
 * that side's whole text — a parser reading removed and added lines mixed
 * together would colour them wrongly.
 */

import type { Span } from "../syntax/paint";
import type { LineSource } from "./view";

/** Anything that colours lines — a `SyntaxHighlighter`, or another of these. */
export interface SpanSource {
  spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]>;
}

export class DiffHighlight implements SpanSource {
  constructor(
    private readonly sources: readonly (LineSource | null)[],
    private readonly sides: { old: SpanSource | null; new: SpanSource | null },
  ) {}

  spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]> {
    const out = new Map<number, Span[]>();
    const end = Math.min(last, this.sources.length - 1);
    // Consecutive document lines showing consecutive lines of one side are asked
    // for in one go; a fold or a switch of side starts a new run.
    let run: { side: "old" | "new"; from: number; to: number; docFrom: number } | null = null;
    const flush = () => {
      if (!run) return;
      const spans = this.sides[run.side]?.spans(run.from, run.to, options);
      if (spans) {
        for (const [line, s] of spans) out.set(run.docFrom + (line - run.from), s);
      }
      run = null;
    };
    for (let doc = Math.max(0, first); doc <= end; doc++) {
      const src = this.sources[doc];
      if (!src) {
        flush();
        continue;
      }
      if (run && run.side === src.side && src.line === run.to + 1 && doc === run.docFrom + (run.to - run.from) + 1) {
        run.to = src.line;
        continue;
      }
      flush();
      run = { side: src.side, from: src.line, to: src.line, docFrom: doc };
    }
    flush();
    return out;
  }
}
