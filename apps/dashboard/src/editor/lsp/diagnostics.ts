/**
 * A document's diagnostics, as the surface needs them (`docs/plans/editor.md`,
 * ED6, L6): the stretches to underline per line, the worst severity per line
 * for the gutter, the messages under a position for the hover, and the next
 * and previous problem for F8 and `]d`/`[d`.
 *
 * Positions are the protocol's — lines and UTF-16 columns — which are also the
 * editor's own (`LineMark` columns are UTF-16), so nothing is converted.
 */

import type { Pos } from "../position";

/** The protocol's severities; lower is worse. */
export type Severity = 1 | 2 | 3 | 4;

export const SEVERITY_NAME: Record<Severity, "error" | "warning" | "info" | "hint"> = {
  1: "error",
  2: "warning",
  3: "info",
  4: "hint",
};

export interface Diagnostic {
  start: Pos;
  end: Pos;
  severity: Severity;
  message: string;
  source?: string;
  code?: string;
  /** The server's own object, handed back as a code action's context (ED6.7). */
  raw?: unknown;
}

/** One underlined stretch of one line. */
export interface DiagnosticMark {
  from: number;
  to: number;
  severity: Severity;
}

/** What one line carries. */
export interface LineDiagnostics {
  marks: DiagnosticMark[];
  /** The worst severity on the line, for its gutter marker. */
  worst: Severity;
  /** The first (worst) message on the line, for the "message at line end" setting. */
  message: string;
}

interface RawRange {
  start?: { line?: number; character?: number };
  end?: { line?: number; character?: number };
}

interface RawDiagnostic {
  range?: RawRange;
  severity?: number;
  message?: string;
  source?: string;
  code?: string | number | { value?: string | number };
}

/** Reads the diagnostics of a `textDocument/publishDiagnostics`, skipping malformed ones. */
export function parseDiagnostics(raw: unknown): Diagnostic[] {
  if (!Array.isArray(raw)) return [];
  const out: Diagnostic[] = [];
  for (const item of raw as RawDiagnostic[]) {
    const s = item?.range?.start;
    const e = item?.range?.end;
    if (typeof s?.line !== "number" || typeof s.character !== "number") continue;
    if (typeof e?.line !== "number" || typeof e.character !== "number") continue;
    if (typeof item.message !== "string") continue;
    const severity = [1, 2, 3, 4].includes(item.severity ?? 1) ? ((item.severity ?? 1) as Severity) : 1;
    const code = typeof item.code === "object" && item.code !== null ? item.code.value : item.code;
    out.push({
      start: { line: s.line, col: s.character },
      end: { line: e.line, col: e.character },
      severity,
      message: item.message,
      source: typeof item.source === "string" ? item.source : undefined,
      code: code === undefined ? undefined : String(code),
      raw: item,
    });
  }
  return out.sort((a, b) => a.start.line - b.start.line || a.start.col - b.start.col || a.severity - b.severity);
}

/** A document's diagnostics, indexed for the surface. */
export class DiagnosticSet {
  private readonly byLine = new Map<number, LineDiagnostics>();

  constructor(
    readonly all: readonly Diagnostic[],
    /** The length of each line, so a stretch to "end of line" and an empty range can be drawn. */
    lineLength: (line: number) => number,
  ) {
    for (const d of all) {
      for (let line = d.start.line; line <= d.end.line; line++) {
        const length = lineLength(line);
        let from = line === d.start.line ? d.start.col : 0;
        let to = line === d.end.line ? d.end.col : length;
        from = Math.min(from, length);
        to = Math.min(Math.max(to, from), length);
        // An empty range (a missing semicolon at the end) still gets one visible cell.
        if (to === from) {
          if (from < length) to = from + 1;
          else if (from > 0) from -= 1;
        }
        let entry = this.byLine.get(line);
        if (!entry) this.byLine.set(line, (entry = { marks: [], worst: d.severity, message: d.message }));
        entry.marks.push({ from, to, severity: d.severity });
        if (d.severity < entry.worst) {
          entry.worst = d.severity;
          entry.message = d.message;
        }
      }
    }
  }

  get size(): number {
    return this.all.length;
  }

  /** What `line` carries, if anything. */
  line(line: number): LineDiagnostics | undefined {
    return this.byLine.get(line);
  }

  /** The diagnostics whose range covers `p` (an empty range covers its own spot), worst first. */
  at(p: Pos): Diagnostic[] {
    return this.all
      .filter((d) => {
        const afterStart = p.line > d.start.line || (p.line === d.start.line && p.col >= d.start.col);
        const empty = d.start.line === d.end.line && d.start.col === d.end.col;
        const beforeEnd = empty
          ? p.line === d.end.line && p.col <= d.end.col + 1
          : p.line < d.end.line || (p.line === d.end.line && p.col < d.end.col);
        return afterStart && beforeEnd;
      })
      .sort((a, b) => a.severity - b.severity);
  }

  /** The next problem after `p` (wrapping round), or `null` when there are none. */
  next(p: Pos): Diagnostic | null {
    if (this.all.length === 0) return null;
    return this.all.find((d) => after(d.start, p)) ?? this.all[0];
  }

  /** The previous problem before `p` (wrapping round), or `null` when there are none. */
  previous(p: Pos): Diagnostic | null {
    if (this.all.length === 0) return null;
    for (let i = this.all.length - 1; i >= 0; i--) if (after(p, this.all[i].start)) return this.all[i];
    return this.all[this.all.length - 1];
  }

  /** How many there are of each severity, for the status line. */
  counts(): Record<"error" | "warning" | "info" | "hint", number> {
    const counts = { error: 0, warning: 0, info: 0, hint: 0 };
    for (const d of this.all) counts[SEVERITY_NAME[d.severity]]++;
    return counts;
  }
}

/** Whether `a` comes strictly after `b`. */
function after(a: Pos, b: Pos): boolean {
  return a.line > b.line || (a.line === b.line && a.col > b.col);
}

/** An empty set, for a document with no server or no diagnostics yet. */
export const NO_DIAGNOSTICS = new DiagnosticSet([], () => 0);
