/**
 * Transient shell notifications ("dashboard.json was corrupt, moved to .bak").
 * Rendered by `shell/Toasts.svelte`; auto-dismissed after `TOAST_MS`.
 */

import { writable } from "svelte/store";

export type ToastKind = "info" | "warning" | "danger";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

export const TOAST_MS = 7000;

export const toasts = writable<Toast[]>([]);

let nextId = 1;

export function toast(message: string, kind: ToastKind = "info"): void {
  const id = nextId++;
  toasts.update((list) => [...list, { id, kind, message }]);
  setTimeout(() => dismissToast(id), TOAST_MS);
}

export function dismissToast(id: number): void {
  toasts.update((list) => list.filter((t) => t.id !== id));
}
