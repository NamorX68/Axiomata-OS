/**
 * A diff's rows as documents a read-only editor surface can draw
 * (`docs/plans/git-layer.md`, H3, H12): the text of each row, its decoration
 * (colour, gutter, word marks, fold buttons), and which side's line it shows —
 * for the colours of that side and for "open the file at this line".
 *
 * Rows without text of their own (a fold, a note, the blank opposite a line
 * the other side lacks) are empty lines in the document; their decoration
 * carries what is drawn there.
 */

import type { LineAction, LineDecoration, LineDecorations } from "../decorations";
import type { FoldAction, FoldRow, LineRow, NoteRow, SplitRow, UnifiedRow } from "./model";

/** One column (removed above added) or two (base left, change right) — H1. */
export type DiffLayout = "unified" | "split";

/** Where a document line's text comes from. */
export interface LineSource {
  side: "old" | "new";
  /** Zero-based line on that side. */
  line: number;
}

export interface DiffPane {
  /** The document's text: one line per row. */
  text: string;
  decorations: LineDecorations;
  /** Per document line; `null` for a fold, note or blank row. */
  sources: readonly (LineSource | null)[];
  /** The first document line of each hunk, in order (H9: ⌥↓/⌥↑). */
  hunkStarts: readonly number[];
}

const ACTION_LABELS: Record<FoldAction, (step: number) => LineAction["label"]> = {
  up: (step) => `↑ ${step}`,
  down: (step) => `↓ ${step}`,
  all: () => "All",
};

const ACTION_TITLES: Record<FoldAction, string> = {
  up: "Show more lines above the change below",
  down: "Show more lines below the change above",
  all: "Show all hidden lines",
};

/** A fold button's action id, and back. */
export function foldActionId(gap: number, action: FoldAction): string {
  return `fold:${gap}:${action}`;
}

export function parseFoldActionId(id: string): { gap: number; action: FoldAction } | null {
  const m = /^fold:(\d+):(up|down|all)$/.exec(id);
  return m ? { gap: Number(m[1]), action: m[2] as FoldAction } : null;
}

function foldDecoration(row: FoldRow, step: number): LineDecoration {
  const lines = `${row.hidden} unchanged line${row.hidden === 1 ? "" : "s"}`;
  return {
    kind: "fold",
    gutter: "⋯",
    label: row.label ? `${lines} · ${row.label}` : lines,
    actions: row.actions.map((a) => ({
      id: foldActionId(row.gap, a),
      label: ACTION_LABELS[a](step),
      title: ACTION_TITLES[a],
    })),
  };
}

function noteDecoration(row: NoteRow): LineDecoration {
  return { kind: "note", label: row.text };
}

const SIGN: Record<LineRow["kind"], string> = { context: " ", add: "+", remove: "-" };

function wordMarks(row: LineRow): LineDecoration["marks"] {
  if (row.kind === "context" || row.words.length === 0) return undefined;
  const kind = row.kind === "add" ? "add-word" : "remove-word";
  return row.words.map((w) => ({ from: w.from, to: w.to, kind }));
}

/** The widest line number any row shows, in digits (at least two). */
function numberDigits(rows: readonly (LineRow | null)[]): number {
  let max = 0;
  for (const r of rows) if (r) max = Math.max(max, (r.oldLine ?? -1) + 1, (r.newLine ?? -1) + 1);
  return Math.max(2, String(max).length);
}

const num = (line: number | null, width: number) => (line === null ? "" : String(line + 1)).padStart(width);

/** Collects rows into a pane: text, decorations, sources, hunk starts. */
class PaneBuilder {
  private readonly lines: string[] = [];
  private readonly decos: LineDecoration[] = [];
  private readonly sources: (LineSource | null)[] = [];
  private readonly hunkStarts: number[] = [];
  private lastHunk: number | null = null;

  push(text: string, deco: LineDecoration, source: LineSource | null, hunk: number | null): void {
    if (hunk !== null && hunk !== this.lastHunk) {
      this.hunkStarts.push(this.lines.length);
      this.lastHunk = hunk;
    }
    this.lines.push(text);
    this.decos.push(deco);
    this.sources.push(source);
  }

  build(gutterCells: number): DiffPane {
    const decos = this.decos;
    return {
      text: this.lines.join("\n"),
      decorations: { gutterCells, line: (line) => decos[line] },
      sources: this.sources,
      hunkStarts: this.hunkStarts,
    };
  }
}

/** The unified layout (H1): both line numbers and the sign in the gutter. */
export function unifiedPane(rows: readonly UnifiedRow[], step: number): DiffPane {
  const width = numberDigits(rows.map((r) => (r.type === "line" ? r : null)));
  const pane = new PaneBuilder();
  for (const row of rows) {
    if (row.type === "fold") pane.push("", foldDecoration(row, step), null, null);
    else if (row.type === "note") pane.push("", noteDecoration(row), null, null);
    else {
      const source: LineSource =
        row.newLine !== null ? { side: "new", line: row.newLine } : { side: "old", line: row.oldLine ?? 0 };
      const deco: LineDecoration = {
        kind: row.kind,
        gutter: `${num(row.oldLine, width)} ${num(row.newLine, width)} ${SIGN[row.kind]}`,
        marks: wordMarks(row),
      };
      pane.push(row.text, deco, source, row.hunk);
    }
  }
  // Two numbers, two spaces, a sign — and the surface's one cell of margin each side.
  return pane.build(2 * width + 3 + 2);
}

/** The split layout (H12): the base's lines left, the change's right, row for row. */
export function splitPanes(rows: readonly SplitRow[], step: number): { left: DiffPane; right: DiffPane } {
  const all = rows.flatMap((r) => (r.type === "pair" ? [r.left, r.right] : []));
  const width = numberDigits(all);
  const left = new PaneBuilder();
  const right = new PaneBuilder();
  const side = (pane: PaneBuilder, row: LineRow | null, which: "old" | "new", hunk: number | null) => {
    if (!row) {
      pane.push("", { kind: "blank" }, null, hunk);
      return;
    }
    const line = which === "old" ? row.oldLine : row.newLine;
    const deco: LineDecoration = {
      kind: row.kind,
      gutter: `${num(line, width)} ${SIGN[row.kind]}`,
      marks: wordMarks(row),
    };
    pane.push(row.text, deco, line === null ? null : { side: which, line }, hunk);
  };
  for (const row of rows) {
    if (row.type === "pair") {
      // Both sides count the pair towards the same hunk, so their starts line up.
      const hunk = row.left?.hunk ?? row.right?.hunk ?? null;
      side(left, row.left, "old", hunk);
      side(right, row.right, "new", hunk);
      continue;
    }
    const deco = row.type === "fold" ? foldDecoration(row, step) : noteDecoration(row);
    left.push("", deco, null, null);
    right.push("", deco, null, null);
  }
  const cells = width + 2 + 2;
  return { left: left.build(cells), right: right.build(cells) };
}

/**
 * The line of the changed side to open for document line `line` (H5, H9: ⏎):
 * the line itself when it shows the changed side; for a removed line, the
 * changed side's next line (where the removed text used to be), else the one
 * before; `null` when the pane shows no changed side at all (a deleted file).
 */
export function changedLineNear(pane: DiffPane, line: number): number | null {
  const at = pane.sources[line];
  if (at?.side === "new") return at.line;
  for (let i = line + 1; i < pane.sources.length; i++) {
    const s = pane.sources[i];
    if (s?.side === "new") return s.line;
  }
  for (let i = line - 1; i >= 0; i--) {
    const s = pane.sources[i];
    if (s?.side === "new") return s.line + 1;
  }
  return null;
}
