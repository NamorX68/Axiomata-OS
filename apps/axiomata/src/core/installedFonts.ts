/**
 * The fonts installed on the Mac (`installed_fonts`, `docs/plans/editor.md`,
 * ED5, T10, T16): every family with its real weights and whether it is
 * monospaced — for the editor's and the terminal's font pickers.
 *
 * Asked for once per run (Rust keeps it too: CoreText takes a moment); until
 * the answer is there the list is empty and `loaded` is false, so nobody
 * mistakes "not asked yet" for "not installed".
 */

import { get, writable, type Readable } from "svelte/store";

import { invokeBackend } from "./backend";

export interface InstalledFont {
  family: string;
  /** CSS weights of its upright faces, ascending. */
  weights: number[];
  monospace: boolean;
}

interface State {
  loaded: boolean;
  fonts: InstalledFont[];
}

const state = writable<State>({ loaded: false, fonts: [] });

/** The installed families, and whether the list has arrived. */
export const installedFonts: Readable<State> = state;

let loading: Promise<void> | null = null;

/** Asks for the list once; later calls wait for the same answer. A failure leaves it empty but loaded. */
export function ensureInstalledFonts(): Promise<void> {
  loading ??= invokeBackend<InstalledFont[]>("installed_fonts")
    .then((fonts) => state.set({ loaded: true, fonts }))
    .catch(() => state.set({ loaded: true, fonts: [] }));
  return loading;
}

/** The installed family named `family` (case-insensitively), if the list has it. */
export function installedFont(family: string): InstalledFont | undefined {
  const wanted = family.toLowerCase();
  return get(state).fonts.find((f) => f.family.toLowerCase() === wanted);
}

/**
 * Whether `name` may be drawn with: the same rule as Rust's `usable_name` —
 * no private system font, nothing that could end a quoted CSS string or a
 * declaration, not absurdly long. Settings files are checked with it too.
 */
export function usableFamilyName(name: string): boolean {
  // Bytes of UTF-8, as Rust counts `len()`; control characters C0, DEL and C1, as Rust's `is_control`.
  return (
    name.length > 0 &&
    new TextEncoder().encode(name).length <= 128 &&
    !name.startsWith(".") &&
    !/[\u0000-\u001f\u007f-\u009f"'\\;{}<>]/.test(name)
  );
}
