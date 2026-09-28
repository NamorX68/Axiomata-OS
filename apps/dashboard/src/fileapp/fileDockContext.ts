/**
 * What the file app's groups and dividers ask of `FileAppView`
 * (`docs/plans/editor-look.md`, LK2) — handed down through Svelte's context so
 * the recursive tree does not thread a dozen props through every level. The
 * IDE does the same with `ide/dockContext.ts`.
 */

import { getContext, setContext } from "svelte";

import type { DockTarget } from "../ide/layout";
import type { Tab } from "./fileDock";

export interface FileDockView {
  /** The label on a tab. */
  title(tab: Tab): string;
  /** The tooltip on a tab: where its file lives. */
  where(tab: Tab): string;
  dirty(tabId: string): boolean;
  /** The group whose visible tab the header and the keys are about. */
  focusedGroup(): string | null;
  focusTab(tabId: string): void;
  focusGroup(groupId: string): void;
  pin(tabId: string): void;
  requestClose(tabId: string): void;
  startTabDrag(tabId: string, event: PointerEvent): void;
  startDividerDrag(splitId: string, boundary: number, event: PointerEvent): void;
  draggingTab(): string | null;
  hint(): DockTarget | null;
}

const KEY = Symbol("file-dock");

export function setFileDock(view: FileDockView): void {
  setContext(KEY, view);
}

export function getFileDock(): FileDockView {
  return getContext<FileDockView>(KEY);
}
