/**
 * Code actions as the editor shows them (`docs/plans/editor.md`, ED6.7,
 * L18–L24): what a `textDocument/codeAction` answer offers, in the order the
 * menu lists it, narrowed by what is typed.
 *
 * An answer holds `CodeAction`s (a title, a kind, an `edit`, a `command`, or
 * both) and bare `Command`s (only a command to run). Either may be taken; a
 * command runs only because Rust saw the server offer it (L18). Part of the
 * engine — no DOM, no app imports.
 */

import { matchScore } from "./completion";

/** A command to run on the server (`workspace/executeCommand`), as the server offered it. */
export interface ServerCommand {
  title: string;
  command: string;
  /** Left out when the server gave none — the echo Rust checks must match (L18). */
  arguments?: unknown[];
}

export interface CodeActionItem {
  title: string;
  /** The protocol's kind (`quickfix`, `refactor.extract`, `source.organizeImports` …); `""` for none. */
  kind: string;
  /** The server's pick for the place (`isPreferred`): chosen when the menu opens. */
  preferred: boolean;
  /** Why it cannot be taken now, when the server says so (L24's greyed entry); `null` when it can. */
  disabled: string | null;
  /** Whether it answers one of the diagnostics asked about (the "Fix…" button's list, L21). */
  fixes: boolean;
  /** The `WorkspaceEdit` it makes, raw; `null` when it has none (yet — `codeAction/resolve`). */
  edit: unknown;
  command: ServerCommand | null;
  /** The server's object, for `codeAction/resolve`; `null` for a bare `Command` (nothing to resolve). */
  raw: Record<string, unknown> | null;
}

/** The order of the kinds in the menu (L19): fixes first, then refactorings, then whole-file actions. */
const KIND_ORDER = ["quickfix", "refactor", "source"];

/** A server `Command`, or `null` when it is not one. */
function parseCommand(raw: unknown): ServerCommand | null {
  const c = raw as { title?: unknown; command?: unknown; arguments?: unknown } | null;
  if (typeof c?.command !== "string") return null;
  const command: ServerCommand = { title: typeof c.title === "string" ? c.title : c.command, command: c.command };
  if (Array.isArray(c.arguments)) command.arguments = c.arguments;
  return command;
}

/** One entry of a code-action answer, or `null` when it is malformed. */
export function parseCodeAction(raw: unknown): CodeActionItem | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.title !== "string") return null;
  // A bare `Command` names its command as a string.
  if (typeof r.command === "string") {
    const command = parseCommand(r);
    return command
      ? { title: r.title, kind: "", preferred: false, disabled: null, fixes: false, edit: null, command, raw: null }
      : null;
  }
  const disabled = r.disabled as { reason?: unknown } | undefined;
  return {
    title: r.title,
    kind: typeof r.kind === "string" ? r.kind : "",
    preferred: r.isPreferred === true,
    disabled: disabled && typeof disabled === "object" ? String(disabled.reason ?? "not available here") : null,
    fixes: Array.isArray(r.diagnostics) && r.diagnostics.length > 0,
    edit: r.edit ?? null,
    command: r.command === undefined ? null : parseCommand(r.command),
    raw: r,
  };
}

/** A code-action answer's entries, malformed ones skipped. */
export function parseCodeActions(result: unknown): CodeActionItem[] {
  if (!Array.isArray(result)) return [];
  const out: CodeActionItem[] = [];
  for (const entry of result) {
    const item = parseCodeAction(entry);
    if (item) out.push(item);
  }
  return out;
}

/** Where `kind` goes in the menu: its family's place in {@link KIND_ORDER}, unknown ones after. */
function kindRank(kind: string): number {
  const family = kind.split(".")[0];
  const at = KIND_ORDER.indexOf(family);
  return at < 0 ? KIND_ORDER.length : at;
}

/**
 * The menu's order (L19): by kind — fixes, refactorings, whole-file actions,
 * the rest — the preferred ones first within a kind, what cannot be taken
 * last; otherwise the server's order.
 */
export function orderActions(items: readonly CodeActionItem[]): CodeActionItem[] {
  return items
    .map((item, i) => ({ item, i }))
    .sort(
      (a, b) =>
        Number(a.item.disabled !== null) - Number(b.item.disabled !== null) ||
        kindRank(a.item.kind) - kindRank(b.item.kind) ||
        Number(b.item.preferred) - Number(a.item.preferred) ||
        a.i - b.i,
    )
    .map((s) => s.item);
}

/** The actions for the "Fix…" button (L21): the ones tied to the diagnostic, else every fix. */
export function fixesOnly(items: readonly CodeActionItem[]): CodeActionItem[] {
  const tied = items.filter((item) => item.fixes);
  return tied.length > 0 ? tied : items.filter((item) => kindRank(item.kind) === 0);
}

/** The actions whose title matches `query` unscharf (L20), best first; empty `query` keeps the order. */
export function filterActions(items: readonly CodeActionItem[], query: string): CodeActionItem[] {
  if (query === "") return [...items];
  const scored: { item: CodeActionItem; score: number; i: number }[] = [];
  items.forEach((item, i) => {
    const score = matchScore(query, item.title);
    if (score !== null) scored.push({ item, score, i });
  });
  return scored.sort((a, b) => b.score - a.score || a.i - b.i).map((s) => s.item);
}

/** The short tag beside an action (L19): what kind of change it is. */
export function kindTag(kind: string): string {
  if (kind === "") return "cmd";
  if (kind.startsWith("quickfix")) return "fix";
  if (kind.startsWith("refactor.extract")) return "extract";
  if (kind.startsWith("refactor.inline")) return "inline";
  if (kind.startsWith("refactor.rewrite")) return "rewrite";
  if (kind.startsWith("refactor")) return "refactor";
  if (kind.startsWith("source.organizeImports")) return "imports";
  if (kind.startsWith("source.fixAll")) return "fix all";
  if (kind.startsWith("source")) return "source";
  return kind.split(".")[0];
}

/** The kinds the client names in its capabilities (L19: every kind is shown). */
export const CODE_ACTION_KINDS = [
  "",
  "quickfix",
  "refactor",
  "refactor.extract",
  "refactor.inline",
  "refactor.rewrite",
  "source",
  "source.organizeImports",
  "source.fixAll",
];
