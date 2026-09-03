/**
 * Markdown → sanitised HTML, shared by the md-file module and the chat panel.
 * `marked` (GFM) renders; DOMPurify scrubs to an allowlist — no raw HTML
 * passthrough, no inline event handlers, no `javascript:` URLs, no styles.
 * Everything is bundled, so the `script-src 'self'` CSP holds.
 */

import DOMPurify from "dompurify";
import { marked } from "marked";

marked.setOptions({ gfm: true, breaks: false, async: false });

const ALLOWED_TAGS = [
  "a", "p", "br", "hr", "em", "strong", "del", "s", "code", "pre", "kbd",
  "blockquote", "ul", "ol", "li", "h1", "h2", "h3", "h4", "h5", "h6",
  "table", "thead", "tbody", "tr", "th", "td", "img", "input", "span", "div", "sup", "sub",
];
const ALLOWED_ATTR = ["href", "title", "alt", "src", "align", "start", "type", "checked", "disabled", "class"];

/** Inline raster images only — never SVG (it can carry script) and never
 *  as a link target (a top-level `data:` navigation would run it). */
const DATA_IMAGE_RE = /^data:image\/(?:png|jpe?g|gif|webp);base64,[a-z0-9+/=]+$/i;

const purify = DOMPurify();
purify.setConfig({
  ALLOWED_TAGS,
  ALLOWED_ATTR,
  ALLOW_DATA_ATTR: false,
  FORBID_ATTR: ["style"],
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|\/|\.|data:image\/(?:png|jpe?g|gif|webp);base64,)/i,
});
purify.addHook("uponSanitizeAttribute", (node, data) => {
  const value = data.attrValue.trim();
  if (data.attrName === "href" && /^data:/i.test(value)) {
    data.keepAttr = false; // no data: link targets at all
  }
  if (data.attrName === "src" && /^data:/i.test(value) && !(node.tagName === "IMG" && DATA_IMAGE_RE.test(value))) {
    data.keepAttr = false;
  }
});
// External links open via the OS browser later (tauri-plugin-opener); for now
// make every anchor safe if the webview ever navigates.
purify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    node.setAttribute("rel", "noopener noreferrer");
    node.setAttribute("target", "_blank");
  }
  if (node.tagName === "INPUT") {
    // Task-list checkboxes render read-only.
    node.setAttribute("disabled", "");
  }
});

export function renderMarkdown(source: string): string {
  const html = marked.parse(source) as string;
  return purify.sanitize(html);
}

/** Plain-text preview of a Markdown note: frontmatter and the first `#`
 *  heading removed, Markdown syntax reduced to text, cut at a word boundary. */
export function excerpt(source: string, max = 600): string {
  let text = source.replace(/^\ufeff/, "");
  text = text.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, "");
  text = text.replace(/^\s*#\s+[^\n]*\n/, "");
  text = text
    .replace(/```[\s\S]*?```/g, (m) => m.replace(/```\w*\n?/g, "").trim())
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*[-*+]\s+\[[ xX]\]\s*/gm, "• ")
    .replace(/^\s*[-*+]\s+/gm, "• ")
    .replace(/^\s*>\s?/gm, "")
    .replace(/(\*\*|__|\*|_|`)/g, "")
    .replace(/\|/g, " ")
    .replace(/^[-:| ]+$/gm, "")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return cut(text, max);
}

/** Plain-text preview of an HTML page: style/script blocks and tags removed,
 *  entities decoded, whitespace collapsed. */
export function excerptHtml(source: string, max = 600): string {
  const text = source
    .replace(/<(script|style)[^>]*>[\s\S]*?<\/\1>/gi, " ")
    .replace(/<\/(title|p|div|h[1-6]|li|br|tr|section|article)>/gi, "\n")
    .replace(/<[^>]+>/g, " ")
    .replace(/&(amp|lt|gt|quot|#39|apos|nbsp);/g, (_, e: string) =>
      ({ amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'", apos: "'", nbsp: " " })[e] ?? "",
    )
    .replace(/&#(\d+);/g, (_, n: string) => String.fromCodePoint(Number(n)))
    .replace(/[ \t]+/g, " ")
    .replace(/\s*\n\s*/g, "\n")
    .replace(/\n{2,}/g, "\n")
    .trim();
  // <title> and the page's h1 are usually the same line — keep one.
  const lines = text.split("\n").filter((line, i, all) => i === 0 || line !== all[i - 1]);
  return cut(lines.join("\n"), max);
}

function cut(text: string, max: number): string {
  if (text.length <= max) return text;
  const slice = text.slice(0, max);
  const at = Math.max(slice.lastIndexOf(" "), slice.lastIndexOf("\n"));
  return `${slice.slice(0, at > max * 0.6 ? at : max).trimEnd()}…`;
}
