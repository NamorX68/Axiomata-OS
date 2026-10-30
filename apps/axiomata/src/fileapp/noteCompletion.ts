/**
 * Completion for a note of the vault (`docs/plans/tags-links.md`, Q4): after `[[` the vault's notes (matched loosely
 * on the name, the area beside it), after `#` the tag list's tags (with their one-line description) and then the tags
 * the vault's notes carry. A source beside the language server's, through the same `CompletionPort`: anywhere else in
 * the note the server (and the file's words) answer as before.
 */

import type { TagEntry } from "../core/backend";
import type { EditorDocument } from "../editor/document";
import type { CompletionAnswer, CompletionItem } from "../editor/lsp/completion";
import { pos, type Pos } from "../editor/position";
import type { CompletionPort, CompletionSpan } from "./completionMenu";
import { codeLines, codeSpans, fileStem, type NoteIndex } from "./noteLinks";
import { tEditor } from "../i18n/editor";

/** What a note name being typed after `[[` may hold: anything up to the `]]`, a `|` or a `#`. */
const LINK_CHAR = /[^[\]|#\n]/;
/** What a tag may hold (as `noteLinks.ts` reads one). */
const TAG_CHAR = /[\p{L}\p{M}\p{N}_\-/]/u;
/** A `#` at the line start or after whitespace, and the tag typed after it so far. */
const TAG_BEFORE = /(?:^|\s)#([\p{L}\p{M}\p{N}_\-/]*)$/u;

/** The protocol's item kinds the menu shows a tag for (`file`, `key`). */
const FILE_KIND = 17;
const KEY_KIND = 14;

/** Marks an item as this source's, so it is never sent to the language server to resolve. */
const OURS = Symbol("note completion");

/** Where the note sources come from — the vault's notes and its tag list. */
export interface NoteSources {
  notes(): Promise<NoteIndex | null>;
  tagList(): Promise<TagEntry[]>;
}

/** The note-name or tag span the cursor is in on `line`, or `null` (not in code, not past a `]]`). */
export function noteSpanAt(line: string, col: number): (CompletionSpan & { kind: "link" | "tag" }) | null {
  const before = line.slice(0, col);
  const code = codeSpans(line);
  // A backtick not closed yet is inline code being typed: nothing in it completes as a link.
  const typingCode = (i: number) => (line.slice(0, i).replace(/`+[^`]*`+/g, "").match(/`/g) ?? []).length > 0;
  const inCode = (i: number) => code.some(([from, to]) => i >= from && i < to) || typingCode(i);
  const open = before.lastIndexOf("[[");
  if (open >= 0 && !/[[\]|#]/.test(before.slice(open + 2)) && !inCode(open)) {
    return { kind: "link", start: open + 2, char: LINK_CHAR };
  }
  const tag = TAG_BEFORE.exec(before);
  // `#` alone at the line start is how a heading begins: a menu there would take ⏎ for a tag.
  if (tag && !/^\s*#+$/.test(before)) {
    const hash = col - tag[1].length - 1;
    if (!inCode(hash)) return { kind: "tag", start: hash + 1, char: TAG_CHAR };
  }
  return null;
}

/** The vault's Markdown notes as menu items; the note being edited is left out. */
export function noteItems(index: NoteIndex, self: string): CompletionItem[] {
  return index.files
    .filter((f) => f.is_markdown && f.path !== self)
    .map((f) => {
      const name = fileStem(f.path);
      const folder = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "";
      return item(name, f.area ?? (folder || "Vault"), FILE_KIND, `${index.linkText(f.path)}]]`, name, folder);
    });
}

/** The tag list's tags first (with their description), then the other tags the vault's notes carry. */
export function tagItems(list: readonly TagEntry[], vault: readonly string[]): CompletionItem[] {
  const listed = new Set<string>();
  const out: CompletionItem[] = [];
  for (const entry of list) {
    const tag = entry.tag.replace(/^#/, "").toLowerCase();
    if (!tag || listed.has(tag)) continue;
    listed.add(tag);
    // The description is the item's detail; the documentation beside the menu would only repeat it.
    out.push(item(tag, entry.description || tEditor("tagListed"), KEY_KIND, tag, `0${tag}`));
  }
  for (const tag of vault) {
    if (!listed.has(tag)) out.push(item(tag, tEditor("tagInVault"), KEY_KIND, tag, `1${tag}`));
  }
  return out;
}

function item(
  label: string,
  detail: string,
  kind: number,
  insertText: string,
  sortKey: string,
  filterExtra = "",
): CompletionItem {
  return {
    label,
    detail,
    kind,
    // The folder goes along after the name, so `ki mod` finds `KI/Modelle` too — the name still weighs most.
    filterText: filterExtra ? `${label} ${filterExtra}` : label,
    sortText: sortKey.toLowerCase(),
    insertText,
    snippet: false,
    range: null,
    additionalEdits: [],
    documentation: null,
    preselect: false,
    raw: OURS,
  };
}

/**
 * `server` with notes and tags for the note `rel` in `doc`: after `[[` and `#` this source answers alone; elsewhere
 * the server does (none: nothing). The text after the cursor already closing the link (`]]`) is not written twice.
 */
export function noteCompletion(
  server: CompletionPort | null,
  doc: EditorDocument,
  rel: string,
  sources: NoteSources,
): CompletionPort {
  const empty: CompletionAnswer = { items: [], incomplete: false };
  let codeAt = -1;
  let code = new Set<number>();
  const inCodeBlock = (line: number) => {
    if (codeAt !== doc.revision) {
      codeAt = doc.revision;
      code = codeLines({ lineCount: doc.store.lineCount(), line: (n) => doc.store.line(n) });
    }
    return code.has(line);
  };
  // The line first: reading the whole note for its code blocks is only worth it once a `[[` or `#` is there.
  const spanAt = (at: Pos) => {
    const found = noteSpanAt(doc.store.line(at.line), at.col);
    return found && !inCodeBlock(at.line) ? found : null;
  };
  return {
    ask: async (at, trigger, again, span) => {
      if (!span) return server ? server.ask(at, trigger, again) : empty;
      const found = spanAt(at);
      if (!found) return empty;
      if (found.kind === "tag") {
        const [list, index] = await Promise.all([sources.tagList(), sources.notes()]);
        return { items: tagItems(list, index?.tags() ?? []), incomplete: false };
      }
      const index = await sources.notes();
      if (!index) return empty;
      // A link already closed after the cursor: the item replaces up to and with its `]]`, so the cursor lands
      // after the link. The range is the cursor's of this ask — asked again on every key, it never goes stale.
      const closed = doc.store.line(at.line).slice(at.col).startsWith("]]");
      const items = noteItems(index, rel);
      if (!closed) return { items, incomplete: false };
      const range = { start: pos(at.line, found.start), end: pos(at.line, at.col + 2) };
      return { items: items.map((i) => ({ ...i, range })), incomplete: true };
    },
    resolve: (i) => (i.raw === OURS || !server ? Promise.resolve(i) : server.resolve(i)),
    triggers: () => server?.triggers() ?? [],
    span: (at) => {
      const found = spanAt(at);
      return found ? { start: found.start, char: found.char } : null;
    },
  };
}
