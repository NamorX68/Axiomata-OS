/**
 * Wikilinks and tags in a note, as Obsidian writes them (`docs/plans/tags-links.md`, Q1–Q3, TL2): where they are on a
 * line, what a link points at, and how a link is resolved against the vault's files — the same rule as the Rust scan
 * (`crates/axiomata-core/src/graph.rs`, `extract_links`), so the editor and the Second Brain agree on every edge.
 *
 * * **A wikilink** is `[[Target]]`, `[[Target|alias]]`, `[[Target#Heading]]` or `[[#Heading]]` (a heading of the note
 *   itself), on one line.
 * * **A tag** is `#` and then letters (umlauts too), digits, `_`, `-` and `/` (nested, `#rust/async`), at the start of a
 *   line or after whitespace, and not digits alone (`#1` is no tag). A heading's `#`s never start one: they are
 *   followed by a space or by another `#`.
 * * **Neither counts in code**: a fenced block (```` ``` ```` / `~~~`), an inline code span, or the frontmatter (where
 *   `#` starts a YAML comment and `tags:` is read by the scan instead).
 * * **Resolving**: the target as a relative path (as written, or with `.md`), from the linking note's folder or from
 *   the root, else the one file whose stem is the target's (ignoring case); two files of that stem are no match.
 */

/** A link or tag found on a line, `[from, to)` in UTF-16 columns. */
export type NoteMark =
  | { kind: "link"; from: number; to: number; target: string; heading: string | null; alias: string | null }
  | { kind: "tag"; from: number; to: number; tag: string };

/** What a document gives the scan: a line by number, and how many there are. */
export interface NoteLines {
  lineCount: number;
  line(n: number): string;
}

/** A file of the vault, as `get_workspace_graph` lists it. */
export interface NoteFile {
  path: string;
  area: string | null;
  is_markdown: boolean;
  tags?: string[];
}

/** One `[[…]]` on a line: `]]` closes it, and none of `[ ]` or a line break may come between. */
const WIKILINK = /\[\[([^[\]\n]+?)\]\]/g;
/** The characters of a tag after its `#`, Obsidian's set. */
const TAG_BODY = /[\p{L}\p{M}\p{N}_\-/]+/uy;
const DIGITS_ONLY = /^\p{N}+$/u;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

/** The parts of a wikilink's inner text (`Target#Heading|alias`). */
export function parseWikilink(inner: string): { target: string; heading: string | null; alias: string | null } {
  const bar = inner.indexOf("|");
  const head = bar < 0 ? inner : inner.slice(0, bar);
  const alias = bar < 0 ? null : inner.slice(bar + 1).trim() || null;
  const hash = head.indexOf("#");
  const target = (hash < 0 ? head : head.slice(0, hash)).trim();
  const heading = hash < 0 ? null : head.slice(hash + 1).trim() || null;
  return { target, heading, alias };
}

/** The inline code spans of a line, as `[from, to)` ranges: a run of backticks up to the next run of the same length. */
export function codeSpans(line: string): [number, number][] {
  const out: [number, number][] = [];
  let i = 0;
  while (i < line.length) {
    if (line[i] !== "`") {
      i++;
      continue;
    }
    let n = 0;
    while (line[i + n] === "`") n++;
    let close = -1;
    for (let j = i + n; j < line.length; ) {
      if (line[j] !== "`") {
        j++;
        continue;
      }
      let m = 0;
      while (line[j + m] === "`") m++;
      if (m === n) {
        close = j;
        break;
      }
      j += m;
    }
    if (close < 0) {
      // An unclosed run is plain text (CommonMark); what follows it may still hold code.
      i += n;
      continue;
    }
    out.push([i, close + n]);
    i = close + n;
  }
  return out;
}

/** Whether column `col` lies in one of `ranges`. */
function inside(ranges: readonly [number, number][], col: number): boolean {
  return ranges.some(([from, to]) => col >= from && col < to);
}

/** The links and tags of one line that is not in a code block, left to right. */
export function lineMarks(line: string): NoteMark[] {
  const code = codeSpans(line);
  const links: NoteMark[] = [];
  const taken: [number, number][] = [];
  for (const match of line.matchAll(WIKILINK)) {
    const from = match.index ?? 0;
    if (inside(code, from)) continue;
    const to = from + match[0].length;
    const parts = parseWikilink(match[1]);
    if (!parts.target && !parts.heading) continue;
    links.push({ kind: "link", from, to, ...parts });
    taken.push([from, to]);
  }
  const tags: NoteMark[] = [];
  for (let i = line.indexOf("#"); i >= 0; i = line.indexOf("#", i + 1)) {
    if (i > 0 && !/\s/u.test(line[i - 1])) continue;
    if (inside(code, i) || inside(taken, i)) continue;
    TAG_BODY.lastIndex = i + 1;
    const body = TAG_BODY.exec(line)?.[0];
    if (!body || DIGITS_ONLY.test(body)) continue;
    tags.push({ kind: "tag", from: i, to: i + 1 + body.length, tag: body.toLowerCase() });
  }
  return [...links, ...tags].sort((a, b) => a.from - b.from);
}

/**
 * Which lines hold no links or tags: the frontmatter at the very top and every fenced code block, fences included.
 * One pass over the whole text — the caller keeps it per state of the text.
 */
export function codeLines(lines: NoteLines): Set<number> {
  const out = new Set<number>();
  let start = 0;
  if (lines.lineCount > 0 && lines.line(0).replace(/^﻿/, "").trimEnd() === "---") {
    for (let n = 1; n < lines.lineCount; n++) {
      const t = lines.line(n).trimEnd();
      if (t === "---" || t === "...") {
        for (let k = 0; k <= n; k++) out.add(k);
        start = n + 1;
        break;
      }
    }
  }
  let fence: { char: string; length: number } | null = null;
  for (let n = start; n < lines.lineCount; n++) {
    const text = lines.line(n);
    const open = FENCE.exec(text);
    if (fence) {
      out.add(n);
      if (open && open[1][0] === fence.char && open[1].length >= fence.length && text.trim() === open[1].trim()) {
        fence = null;
      }
      continue;
    }
    if (open) {
      fence = { char: open[1][0], length: open[1].length };
      out.add(n);
    }
  }
  return out;
}

/** The link or tag under column `col` of `line` (its brackets and `#` included), if any. */
export function markAt(line: string, col: number): NoteMark | null {
  return lineMarks(line).find((m) => col >= m.from && col < m.to) ?? null;
}

/** A file's name without its folder and its last extension — the Rust scan's `file_stem`. */
export function fileStem(rel: string): string {
  const name = rel.slice(rel.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot < 0 ? name : name.slice(0, dot);
}

/** `folder/../x.md` → `x.md`; never above the root — the Rust scan's `normalise_rel`. */
export function normaliseRel(folder: string, candidate: string): string {
  const joined = !folder || candidate.startsWith("/") ? candidate.replace(/^\/+/, "") : `${folder}/${candidate}`;
  const parts: string[] = [];
  for (const part of joined.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}

/** The vault's files, for resolving links and offering notes and tags. */
export class NoteIndex {
  private readonly paths: Set<string>;
  private readonly byStem = new Map<string, string[]>();

  constructor(readonly files: readonly NoteFile[]) {
    this.paths = new Set(files.map((f) => f.path));
    for (const f of files) {
      const stem = fileStem(f.path).toLowerCase();
      const list = this.byStem.get(stem);
      if (list) list.push(f.path);
      else this.byStem.set(stem, [f.path]);
    }
  }

  /** The file a link from the note `from` to `target` means, or `null` when there is none (or two). */
  resolve(from: string, target: string): string | null {
    const t = target.trim();
    if (!t) return null;
    const folder = from.includes("/") ? from.slice(0, from.lastIndexOf("/")) : "";
    for (const candidate of [t, `${t}.md`]) {
      const normalised = normaliseRel(folder, candidate);
      if (this.paths.has(normalised)) return normalised;
      if (this.paths.has(candidate)) return candidate;
    }
    const found = this.byStem.get(fileStem(t).toLowerCase());
    return found && found.length === 1 ? found[0] : null;
  }

  /** Every file whose stem is `target`'s — more than one is why a bare name finds none (`[[Plan]]`, two `Plan.md`). */
  sameStem(target: string): string[] {
    return [...(this.byStem.get(fileStem(target.trim()).toLowerCase()) ?? [])];
  }

  /** Whether `path` names a file of the vault. */
  has(path: string): boolean {
    return this.paths.has(path);
  }

  /** What a new link to `path` says: its stem when that alone finds it, else its path without `.md`. */
  linkText(path: string): string {
    const stem = fileStem(path);
    if ((this.byStem.get(stem.toLowerCase()) ?? []).length === 1) return stem;
    return path.replace(/\.md$/i, "");
  }

  /** Every tag the vault's notes carry, sorted. */
  tags(): string[] {
    const seen = new Set<string>();
    for (const f of this.files) for (const t of f.tags ?? []) seen.add(t.toLowerCase());
    return [...seen].sort();
  }
}

/** The line of the heading `heading` in `text` (ignoring case and surrounding `#`s), or `null`. */
export function headingLine(text: string, heading: string): number | null {
  const wanted = heading.trim().toLowerCase();
  const lines = text.split("\n");
  for (let n = 0; n < lines.length; n++) {
    const m = /^ {0,3}#{1,6}\s+(.*?)\s*#*\s*$/.exec(lines[n]);
    if (m && m[1].trim().toLowerCase() === wanted) return n;
  }
  return null;
}

/**
 * Where a link into nothing creates its note (Q3): in the folder of the note it was followed from, named after the
 * target's last part — `null` when that is no usable file name.
 */
export function newNotePath(fromRel: string, target: string): string | null {
  const last = target.trim().split("/").pop()?.trim() ?? "";
  const name = last.replace(/\.md$/i, "").trim();
  if (!name || name === "." || name === ".." || name.startsWith(".") || name.includes("\u0000")) return null;
  const folder = fromRel.includes("/") ? fromRel.slice(0, fromRel.lastIndexOf("/")) : "";
  return folder ? `${folder}/${name}.md` : `${name}.md`;
}
