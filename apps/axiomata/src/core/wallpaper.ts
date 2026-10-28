/**
 * The owner's background image behind the glass (`docs/plans/glass.md`, G15). Rust keeps the one copy
 * (`src-tauri/src/wallpaper.rs`); here its bytes become a `blob:` URL, written to `--ax-wallpaper` on `<html>` so the
 * ground (`IdeView.svelte`'s `.ground`) shows it instead of the theme's own. No image, no property: the theme's ground.
 */

import { writable, type Writable } from "svelte/store";

import { invokeBackend } from "./backend";

/** The image on show as a `blob:` URL, or `null` for the theme's own ground. */
export const wallpaper: Writable<string | null> = writable(null);

/** The picture formats Rust stores, by the bytes they start with — the `Blob` needs its type to show as an image. */
export function imageType(bytes: Uint8Array): string | null {
  const starts = (...head: number[]): boolean => head.every((b, i) => bytes[i] === b);
  if (starts(0x89, 0x50, 0x4e, 0x47)) return "image/png";
  if (starts(0xff, 0xd8, 0xff)) return "image/jpeg";
  if (starts(0x52, 0x49, 0x46, 0x46) && [0x57, 0x45, 0x42, 0x50].every((b, i) => bytes[8 + i] === b)) {
    return "image/webp";
  }
  return null;
}

/** Shows `url`, or the theme's ground for `null`, and lets go of the image shown before. */
function show(url: string | null): void {
  wallpaper.update((previous) => {
    if (previous) URL.revokeObjectURL(previous);
    return url;
  });
}

/** Reads the stored image and shows it; none stored, or none that is a picture, is the theme's ground. A read that
 *  fails keeps what is on show — at the start that is the theme's ground. */
export async function loadWallpaper(): Promise<void> {
  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await invokeBackend<ArrayBuffer>("wallpaper_read"));
  } catch {
    return;
  }
  const type = bytes.length > 0 ? imageType(bytes) : null;
  show(type ? URL.createObjectURL(new Blob([bytes], { type })) : null);
}

/** Lets the owner pick an image in the native dialog and shows it. `false` when the dialog was cancelled. */
export async function pickWallpaper(): Promise<boolean> {
  const picked = await invokeBackend<boolean>("wallpaper_pick");
  if (picked) await loadWallpaper();
  return picked;
}

/** Goes back to the theme's ground. */
export async function clearWallpaper(): Promise<void> {
  await invokeBackend<void>("wallpaper_clear");
  show(null);
}

/** Loads the stored image and keeps `--ax-wallpaper` on `<html>` in step with it; returns a function that stops. */
export function startWallpaper(): () => void {
  const root = document.documentElement;
  const unsubscribe = wallpaper.subscribe((url) => {
    if (url) root.style.setProperty("--ax-wallpaper", `url("${url}")`);
    else root.style.removeProperty("--ax-wallpaper");
  });
  void loadWallpaper();
  return unsubscribe;
}
