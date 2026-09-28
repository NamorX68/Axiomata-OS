/**
 * Which icon a file or folder gets in the file tree (`docs/plans/editor-look.md`,
 * LK4, K5), in one of four styles: Catppuccin, "Git" (GitHub's Octicons),
 * JetBrains and monochrome.
 *
 * * **The kind of a file** is decided once, independent of the style: first by
 *   its whole name (`Cargo.toml`, `package.json`, a lock file, `.gitignore`,
 *   `Dockerfile`, `LICENSE`, `README`, `.env`), then by extension (pictures,
 *   JSX/TSX), then by its language — the same table the tab colours use
 *   (`languageColors.ts`).
 * * **The styles** are the vendored sets (`ui/icons/fileIcons.ts`,
 *   `scripts/vendor-file-icons.sh`) and, for monochrome, the Lucide icons
 *   already in the app, drawn in the text colour.
 * * **JetBrains draws dark themes differently**; which variant shows follows the
 *   theme's `--ax-color-scheme` ({@link lightTheme}).
 */

import { readable, type Readable } from "svelte/store";

import { VENDORED_FILE_ICONS, type FileIconGlyph, type FileIconKey } from "../ui/icons/fileIcons";
import type { IconName } from "../ui/icons/lucide";
import { languageColorId } from "./languageColors";

export type { FileIconKey };

/** The styles offered in the settings; `none` shows no icons. */
export const FILE_ICON_STYLES = ["catppuccin", "octicons", "jetbrains", "mono", "none"] as const;
export type FileIconStyle = (typeof FILE_ICON_STYLES)[number];

/** What each style is called in the settings. */
export const FILE_ICON_STYLE_NAMES: Record<FileIconStyle, string> = {
  catppuccin: "Catppuccin",
  octicons: "Git (Octicons)",
  jetbrains: "JetBrains",
  mono: "Monochrome",
  none: "None",
};

/** Whole names that say what a file is, whatever its extension. */
const BY_NAME: Record<string, FileIconKey> = {
  "cargo.toml": "cargo",
  "package.json": "npm",
  "cargo.lock": "lock",
  "package-lock.json": "lock",
  "yarn.lock": "lock",
  "pnpm-lock.yaml": "lock",
  "bun.lockb": "lock",
  "uv.lock": "lock",
  "poetry.lock": "lock",
  ".gitignore": "git",
  ".gitattributes": "git",
  ".gitmodules": "git",
  dockerfile: "docker",
  "docker-compose.yml": "docker",
  "docker-compose.yaml": "docker",
  "compose.yml": "docker",
  "compose.yaml": "docker",
};

const IMAGES = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "tiff", "avif", "heic"]);

/** The languages that are a kind of their own (`languageColors.ts` ids). */
const LANGUAGE_KINDS = new Set<string>([
  "rust",
  "typescript",
  "javascript",
  "json",
  "markdown",
  "python",
  "css",
  "html",
  "svelte",
  "toml",
  "yaml",
  "lua",
  "swift",
  "sql",
  "bash",
]);

/** The kind of a file, by its name (the last path segment is enough). */
export function fileIconKey(fileName: string): FileIconKey {
  const base = (fileName.split("/").pop() ?? fileName).toLowerCase();
  const named = BY_NAME[base];
  if (named) return named;
  if (/^(license|licence|copying)(\..+)?$/.test(base)) return "license";
  if (/^readme(\..+)?$/.test(base)) return "readme";
  if (base === ".env" || base.startsWith(".env.")) return "env";
  if (base.endsWith(".lock")) return "lock";
  const ext = base.includes(".") ? base.slice(base.lastIndexOf(".") + 1) : "";
  if (IMAGES.has(ext)) return "image";
  if (ext === "tsx" || ext === "jsx") return "react";
  if (ext === "txt" || ext === "log") return "text";
  const language = languageColorId(base);
  return LANGUAGE_KINDS.has(language) ? (language as FileIconKey) : "file";
}

/** Monochrome: a Lucide icon per kind, drawn in the text colour. */
const MONO: Record<FileIconKey, IconName> = {
  rust: "file-code",
  typescript: "file-code",
  javascript: "file-code",
  react: "file-code",
  json: "file-braces",
  markdown: "file-text",
  python: "file-code",
  css: "file-code",
  html: "file-code",
  svelte: "file-code",
  toml: "file-cog",
  yaml: "file-cog",
  lua: "file-code",
  swift: "file-code",
  sql: "database",
  bash: "file-code",
  image: "file-image",
  git: "git-branch",
  docker: "container",
  lock: "lock",
  text: "file-text",
  env: "file-cog",
  license: "file-text",
  readme: "file-text",
  cargo: "file-cog",
  npm: "file-cog",
  file: "file",
  folder: "folder",
  "folder-open": "folder-open",
};

/** What to draw: a vendored glyph (its own colours), a Lucide icon (the text colour), or nothing. */
export type FileIconDrawing =
  | { kind: "glyph"; viewBox: string; body: string }
  | { kind: "lucide"; name: IconName }
  | null;

/** The drawing for a kind in a style; `light` picks JetBrains' light variant. */
export function fileIconDrawing(style: FileIconStyle, key: FileIconKey, light: boolean): FileIconDrawing {
  if (style === "none") return null;
  if (style === "mono") return { kind: "lucide", name: MONO[key] };
  const glyph: FileIconGlyph = VENDORED_FILE_ICONS[style][key];
  const variant = !light && glyph.dark ? glyph.dark : glyph;
  return { kind: "glyph", viewBox: variant.viewBox, body: variant.body };
}

/**
 * Whether the theme in effect is a light one (its `--ax-color-scheme`), for
 * JetBrains' variants. Watched on the page itself — `<html data-theme>` and the
 * styles in `<head>` — because a custom `theme.css` can change the scheme
 * without the active theme changing.
 */
export const lightTheme: Readable<boolean> = readable(false, (set) => {
  if (typeof document === "undefined") return;
  const read = () =>
    set(getComputedStyle(document.documentElement).getPropertyValue("--ax-color-scheme").trim() === "light");
  read();
  const observer = new MutationObserver(read);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "style"] });
  observer.observe(document.head, { childList: true, subtree: true, characterData: true });
  return () => observer.disconnect();
});
