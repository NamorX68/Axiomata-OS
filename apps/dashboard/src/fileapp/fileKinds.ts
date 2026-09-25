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

/** Source only, the rendered view only, or both side by side (G8, W3). */
export type ViewMode = "source" | "preview" | "split";

/** ⌘⇧V: source → rendered → side by side → source. */
export function nextViewMode(mode: ViewMode): ViewMode {
  return mode === "source" ? "preview" : mode === "preview" ? "split" : "source";
}

/** What a file of `kind` opened with `intent` shows first (W1). */
export function initialViewMode(kind: PreviewKind | null, intent: OpenIntent): ViewMode {
  return startsInPreview(kind, intent) ? "preview" : "source";
}

/** The footer's word for `mode`. */
export function viewModeLabel(mode: ViewMode): string {
  return mode === "source" ? "Source" : mode === "preview" ? "Preview" : "Side by side";
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

/** The first `maxLines` lines of `text`, and whether anything was left off (W15: a preview shows the start). */
export function headOf(text: string, maxLines: number): { text: string; cut: boolean } {
  if (maxLines <= 0) return { text: "", cut: text.length > 0 };
  let at = -1;
  for (let line = 0; line < maxLines; line++) {
    at = text.indexOf("\n", at + 1);
    if (at < 0) return { text, cut: false };
  }
  return { text: text.slice(0, at), cut: at < text.length - 1 };
}
