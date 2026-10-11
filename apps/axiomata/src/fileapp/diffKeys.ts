/**
 * The diff view's keys as plain logic (`ide/DiffView.svelte`, `DiffPanes.svelte` act on the result), in the terms of
 * `core/keys.ts`. Off the Mac the commands whose Ctrl letter Vi owns (`<C-r>`, `<C-d>`) and the destructive one
 * take Ctrl+Shift (`docs/plans/keys-platforms.md`, K-6).
 */

import { chord, type KeyLike, type Platform } from "../core/keys";

export type DiffViewAction = "next-file" | "previous-file" | "refresh" | "layout";
export type DiffHunkAction = "next-hunk" | "previous-hunk" | "discard-hunk" | "open";

/**
 * ⌥⌘↓/⌥⌘↑ step the file, ⌘R reloads, ⇧⌘D flips the layout — wherever the focus is in the view. Off the Mac:
 * Ctrl+Alt+↓/↑, Ctrl+Shift+R and Ctrl+Shift+D.
 */
export function diffViewAction(e: KeyLike, on: Platform): DiffViewAction | null {
  const c = chord(e, on);
  if (!c.primary || c.typing) return null;
  if (c.alt && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
    return e.key === "ArrowDown" ? "next-file" : "previous-file";
  }
  if (c.key === "d" && c.shift) return "layout";
  if (c.alt) return null;
  if (c.key === "r") return (on === "mac" ? !c.shift : c.shift) ? "refresh" : null;
  return null;
}

/**
 * In the diff's text: ⌥↓/⌥↑ go to the next/previous hunk, ⌘⌫ discards the hunk under the cursor (Ctrl+Shift+⌫ off
 * the Mac, where Ctrl+⌫ is "delete a word"), ⏎ opens the file at the cursor.
 */
export function diffHunkAction(e: KeyLike, on: Platform): DiffHunkAction | null {
  if (e.altKey && !e.metaKey && !e.ctrlKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
    return e.key === "ArrowDown" ? "next-hunk" : "previous-hunk";
  }
  const c = chord(e, on);
  if (e.key === "Backspace" && c.primary && !c.alt && !c.typing && (on === "mac" ? !c.shift : c.shift)) {
    return "discard-hunk";
  }
  if (e.key === "Enter" && !e.metaKey && !e.altKey && !e.ctrlKey && !e.shiftKey) return "open";
  return null;
}
