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
