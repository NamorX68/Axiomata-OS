/**
 * The editor's fonts and their real weights (`docs/plans/editor.md`, D7, F13).
 *
 * Every family lists the weights its package actually ships, and each weight
 * loads only when it is first chosen or previewed — `main.ts` imports three
 * weights per family for the terminal; the other forty-odd faces would be dead
 * weight on every start. The loaders are literal `import()` calls, so Vite
 * splits every face into its own chunk.
 *
 * A weight a family does not have is never faked by the browser: the editor
 * keeps the owner's choice in its settings and draws the nearest real face,
 * saying so next to the slider (F13).
 *
 * Besides the bundled families, every font installed on the Mac (ED5, T10,
 * `core/installedFonts.ts`) with the weights CoreText reports; those need no
 * loading — the webview has them. A family that is neither (uninstalled
 * since it was chosen) is drawn with the default instead (`drawnFamily`).
 */

import { installedFont } from "../core/installedFonts";

type Loader = () => Promise<unknown>;

export interface FontFamily {
  family: string;
  /** Weight → loader for its face; ascending. */
  faces: Record<number, Loader>;
}

/** Already loaded globally (`main.ts` / `terminal-nerd-fonts.css`). */
const PRELOADED: Loader = async () => undefined;

export const EDITOR_FONTS: readonly FontFamily[] = [
  {
    family: "JetBrains Mono",
    faces: {
      100: PRELOADED,
      200: () => import("@fontsource/jetbrains-mono/200.css"),
      300: () => import("@fontsource/jetbrains-mono/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/jetbrains-mono/500.css"),
      600: () => import("@fontsource/jetbrains-mono/600.css"),
      700: PRELOADED,
      800: () => import("@fontsource/jetbrains-mono/800.css"),
    },
  },
  { family: "JetBrainsMono Nerd Font Mono", faces: { 100: PRELOADED, 400: PRELOADED, 700: PRELOADED } },
  {
    family: "IBM Plex Mono",
    faces: {
      100: PRELOADED,
      200: () => import("@fontsource/ibm-plex-mono/200.css"),
      300: () => import("@fontsource/ibm-plex-mono/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/ibm-plex-mono/500.css"),
      600: () => import("@fontsource/ibm-plex-mono/600.css"),
      700: PRELOADED,
    },
  },
  {
    family: "Fira Code",
    faces: {
      300: PRELOADED,
      400: PRELOADED,
      500: () => import("@fontsource/fira-code/500.css"),
      600: () => import("@fontsource/fira-code/600.css"),
      700: PRELOADED,
    },
  },
  {
    family: "Source Code Pro",
    faces: {
      200: PRELOADED,
      300: () => import("@fontsource/source-code-pro/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/source-code-pro/500.css"),
      600: () => import("@fontsource/source-code-pro/600.css"),
      700: PRELOADED,
      800: () => import("@fontsource/source-code-pro/800.css"),
      900: () => import("@fontsource/source-code-pro/900.css"),
    },
  },
  {
    family: "Roboto Mono",
    faces: {
      100: PRELOADED,
      200: () => import("@fontsource/roboto-mono/200.css"),
      300: () => import("@fontsource/roboto-mono/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/roboto-mono/500.css"),
      600: () => import("@fontsource/roboto-mono/600.css"),
      700: PRELOADED,
    },
  },
  { family: "Space Mono", faces: { 400: PRELOADED, 700: PRELOADED } },
  { family: "Ubuntu Mono", faces: { 400: PRELOADED, 700: PRELOADED } },
  {
    family: "Inconsolata",
    faces: {
      200: PRELOADED,
      300: () => import("@fontsource/inconsolata/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/inconsolata/500.css"),
      600: () => import("@fontsource/inconsolata/600.css"),
      700: PRELOADED,
      800: () => import("@fontsource/inconsolata/800.css"),
      900: () => import("@fontsource/inconsolata/900.css"),
    },
  },
  {
    family: "Victor Mono",
    faces: {
      100: PRELOADED,
      200: () => import("@fontsource/victor-mono/200.css"),
      300: () => import("@fontsource/victor-mono/300.css"),
      400: PRELOADED,
      500: () => import("@fontsource/victor-mono/500.css"),
      600: () => import("@fontsource/victor-mono/600.css"),
      700: PRELOADED,
    },
  },
  { family: "Anonymous Pro", faces: { 400: PRELOADED, 700: PRELOADED } },
];

const WEIGHT_NAMES: Record<number, string> = {
  100: "Thin",
  200: "ExtraLight",
  300: "Light",
  400: "Regular",
  500: "Medium",
  600: "SemiBold",
  700: "Bold",
  800: "ExtraBold",
  900: "Black",
};

export function weightName(weight: number): string {
  return `${WEIGHT_NAMES[weight] ?? "Weight"} ${weight}`;
}

export function fontFamily(family: string): FontFamily | undefined {
  return EDITOR_FONTS.find((f) => f.family === family);
}

/** The default family, drawn when the chosen one is not there. */
export const DEFAULT_FONT_FAMILY = "JetBrains Mono";

/** The weights `family` really has, ascending — bundled or installed; `[400]` for an unknown family. */
export function realWeights(family: string): number[] {
  const known = fontFamily(family);
  if (known) return Object.keys(known.faces).map(Number).sort((a, b) => a - b);
  const installed = installedFont(family);
  return installed && installed.weights.length > 0 ? installed.weights : [400];
}

/**
 * The family to draw with for the chosen `family`: itself if it is bundled or
 * installed (or the installed list has not arrived yet), else the default.
 */
export function drawnFamily(family: string, installedLoaded: boolean): string {
  if (fontFamily(family) || !installedLoaded || installedFont(family)) return family;
  return DEFAULT_FONT_FAMILY;
}

/** The real weight nearest to `wanted`; on a tie the lighter one. */
export function nearestWeight(family: string, wanted: number): number {
  return realWeights(family).reduce((best, w) => (Math.abs(w - wanted) < Math.abs(best - wanted) ? w : best));
}

/** Loads `family`'s face at `weight` (a real one), once; unknown faces resolve at once. */
export async function loadFace(family: string, weight: number): Promise<void> {
  await fontFamily(family)?.faces[weight]?.();
}
