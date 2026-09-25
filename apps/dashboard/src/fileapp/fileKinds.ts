/**
 * What kind of view a file gets in the file app (`docs/plans/editor.md`, ED4,
 * W1, W3): text in the editor, with a rendered preview for Markdown, HTML and
 * SVG, or a picture for a raster image.
 *
 * The image list mirrors the file service's `image_mime`
 * (`crates/axiomata-files/src/file.rs`) — keep them in step. SVG is not in it
 * on purpose: it is text, opened as source, and previewed as a picture.
 */

/** Files the editor can show rendered beside (or instead of) their source. */
export type PreviewKind = "markdown" | "html" | "svg";

/** Where a file is being opened from — which decides whether it starts rendered (W1). */
export type OpenIntent = "read" | "edit";

const IMAGE = /\.(?:png|jpe?g|gif|webp|bmp|tiff?|heic|heif|avif)$/i;
const MARKDOWN = /\.(?:md|markdown|mdown|mkd|mkdn)$/i;
const HTML = /\.html?$/i;
const SVG = /\.svg$/i;

/** A raster image: shown as a picture, never as text. */
export function isImagePath(rel: string): boolean {
  return IMAGE.test(rel);
}

/** The rendered view `fileName` has beside its source, or `null` for plain text and code. */
export function previewKindFor(fileName: string): PreviewKind | null {
  if (MARKDOWN.test(fileName)) return "markdown";
  if (HTML.test(fileName)) return "html";
  if (SVG.test(fileName)) return "svg";
  return null;
}

/**
 * Whether a file opened with `intent` starts on its rendered view: reading
 * (from the Second Brain, the chat, an agent) shows Markdown and HTML rendered;
 * editing, code, and SVG (whose source is what one opens it for) start in the
 * editor. ⌘⇧V switches either way.
 */
export function startsInPreview(kind: PreviewKind | null, intent: OpenIntent): boolean {
  return intent === "read" && (kind === "markdown" || kind === "html");
}
