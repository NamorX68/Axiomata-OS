/**
 * Parse / serialize / mutate `ToDo.md` — the backing file of the `todo`
 * module (one fixed file in the workspace root). The `.svelte` shell stays
 * thin and all list logic lives here, matching the repo's
 * pure-logic-plus-vitest convention (`core/snap.ts`, `core/htmllink.ts`).
 *
 * On-disk format — standard GitHub-flavoured Markdown task lists, so the file
 * also renders correctly in the `md-file` module and in Obsidian:
 *
 *     # ToDo
 *
 *     - [ ] Open task
 *     - [ ] Another open task
 *
 *     ## Done
 *
 *     - [x] Finished task (done: 2026-09-04)
 *
 * Everything before the first `## Done` heading is the open list; everything
 * after it is the done list — regardless of each line's checkbox state, so
 * hand-editing the file never moves an item unexpectedly. A completion date
 * is a trailing ` (done: YYYY-MM-DD)`; a missing or malformed one parses as
 * `doneOn: null` and the item is kept.
 */

/** The single backing file, relative to the workspace root. */
export const TODO_PATH = "ToDo.md";

/** One task line. `doneOn` is only ever set for items in the done list. */
export interface TodoItem {
  text: string;
  doneOn: string | null;
}

/** The whole file, split into its two sections. */
export interface TodoDoc {
  open: TodoItem[];
  done: TodoItem[];
}

/** A Markdown task-list line: `- [ ] text` / `* [x] text` (lenient spacing). */
const TASK_LINE = /^\s*[-*]\s*\[([ xX])\]\s*(.*)$/;
/** The section separator: a heading whose text starts with "done". */
const DONE_HEADING = /^\s*#{1,6}\s+done\b/i;
/** A trailing completion stamp, e.g. ` (done: 2026-09-04)`. */
const DONE_SUFFIX = /\s*\(done:\s*(\d{4}-\d{2}-\d{2})\)\s*$/;

/** Parses `ToDo.md` content. Non-task lines are dropped; the open/done split
 *  is the first `## Done` heading (any heading level). */
export function parseTodoDoc(md: string): TodoDoc {
  const open: TodoItem[] = [];
  const done: TodoItem[] = [];
  let inDone = false;
  for (const raw of md.split(/\r?\n/)) {
    if (DONE_HEADING.test(raw)) {
      inDone = true;
      continue;
    }
    const m = TASK_LINE.exec(raw);
    if (!m) continue;
    const body = m[2].trim();
    if (!body) continue;
    if (!inDone) {
      open.push({ text: body, doneOn: null });
      continue;
    }
    const stamp = DONE_SUFFIX.exec(body);
    done.push({
      text: stamp ? body.slice(0, stamp.index).trim() : body,
      doneOn: stamp ? stamp[1] : null,
    });
  }
  return { open, done };
}

/** Serializes to the canonical form. Deterministic; `## Done` is emitted only
 *  when there is at least one done item. Ends with exactly one `\n`. */
export function serializeTodoDoc(doc: TodoDoc): string {
  const lines = ["# ToDo", ""];
  for (const it of doc.open) lines.push(`- [ ] ${it.text}`);
  if (doc.done.length > 0) {
    lines.push("", "## Done", "");
    for (const it of doc.done) {
      lines.push(it.doneOn ? `- [x] ${it.text} (done: ${it.doneOn})` : `- [x] ${it.text}`);
    }
  }
  return `${lines.join("\n")}\n`;
}

/** Appends a new open task. A blank or whitespace-only `text` is a no-op. */
export function addTodo(doc: TodoDoc, text: string): TodoDoc {
  const t = text.trim();
  if (!t) return doc;
  return { open: [...doc.open, { text: t, doneOn: null }], done: doc.done };
}

/** Moves open item `index` to the front of the done list, stamped `today`
 *  (`YYYY-MM-DD`). Any stray trailing `(done: …)` in the text is dropped. */
export function completeTodo(doc: TodoDoc, index: number, today: string): TodoDoc {
  if (index < 0 || index >= doc.open.length) return doc;
  const text = doc.open[index].text.replace(DONE_SUFFIX, "").trim();
  return {
    open: doc.open.filter((_, i) => i !== index),
    done: [{ text, doneOn: today }, ...doc.done],
  };
}

/** Moves done item `index` back to the end of the open list, dropping its date. */
export function reopenTodo(doc: TodoDoc, index: number): TodoDoc {
  if (index < 0 || index >= doc.done.length) return doc;
  return {
    open: [...doc.open, { text: doc.done[index].text, doneOn: null }],
    done: doc.done.filter((_, i) => i !== index),
  };
}

/** Removes open item `index`. */
export function deleteOpen(doc: TodoDoc, index: number): TodoDoc {
  if (index < 0 || index >= doc.open.length) return doc;
  return { open: doc.open.filter((_, i) => i !== index), done: doc.done };
}

/** Removes done item `index`. */
export function deleteDone(doc: TodoDoc, index: number): TodoDoc {
  if (index < 0 || index >= doc.done.length) return doc;
  return { open: doc.open, done: doc.done.filter((_, i) => i !== index) };
}

/** Local calendar date as `YYYY-MM-DD` (not UTC — a task ticked at 23:30 is
 *  done "today" in the user's timezone). */
export function todayIso(date: Date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}
