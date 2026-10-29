/**
 * Markdown → sanitised HTML, shared by the editor's preview and the chat panel.
 * `marked` (GFM) renders — fenced code blocks go through `highlight.js` for
 * syntax colouring first; DOMPurify scrubs the result to an allowlist (the
 * `hljs-*` `<span>`s highlighting produces are plain elements + `class`, both
 * already allowed, so no sanitiser change was needed for this). No raw HTML
 * passthrough, no inline event handlers, no `javascript:` URLs, no styles.
 * Everything is bundled, so the `script-src 'self'` CSP holds.
 */

import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/core";
import type { LanguageFn } from "highlight.js";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdownLang from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";
import { Marked, marked, type TokenizerAndRendererExtension, type Tokens } from "marked";

/** Maps every registered name — canonical or alias — to its canonical form,
 *  so a code fence's `language-*` class is stable regardless of which one
 *  the author typed (` ```js ` and ` ```javascript ` render identically). */
const CANONICAL_LANGUAGE: Record<string, string> = {};

/** Registers one language under `canonical` plus any `aliases`, and records
 *  the canonical form for {@link CANONICAL_LANGUAGE}. `highlight.js/lib/core`
 *  ships with no languages registered — only what's listed at the bottom of
 *  this file is bundled, rather than the ~190-language full build. */
function registerLanguage(canonical: string, definition: LanguageFn, aliases: string[] = []): void {
  hljs.registerLanguage(canonical, definition);
  CANONICAL_LANGUAGE[canonical] = canonical;
  if (aliases.length > 0) {
    hljs.registerAliases(aliases, { languageName: canonical });
    for (const alias of aliases) CANONICAL_LANGUAGE[alias] = canonical;
  }
}

// A curated set of languages likely to show up in personal notes / dev docs.
registerLanguage("javascript", javascript, ["js", "jsx"]);
registerLanguage("typescript", typescript, ["ts", "tsx"]);
registerLanguage("python", python, ["py"]);
registerLanguage("rust", rust, ["rs"]);
registerLanguage("bash", bash, ["sh", "shell", "zsh"]);
registerLanguage("json", json);
registerLanguage("yaml", yaml, ["yml"]);
registerLanguage("css", css);
registerLanguage("xml", xml, ["html"]);
registerLanguage("markdown", markdownLang, ["md"]);
registerLanguage("sql", sql);

/** Escapes text for safe placement inside HTML — used only for a code block
 *  whose fence language isn't one `hljs` knows, since `hljs.highlight`
 *  already returns escaped markup for the ones it does. */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** A fenced code block, coloured by `hljs` where it knows the language. */
function renderCode({ text, lang }: Tokens.Code): string {
  const requested = lang?.trim().split(/\s+/, 1)[0]?.toLowerCase();
  const canonical = requested ? CANONICAL_LANGUAGE[requested] : undefined;
  const body = canonical ? hljs.highlight(text, { language: canonical }).value : escapeHtml(text);
  const langClass = canonical ? ` language-${canonical}` : "";
  return `<pre><code class="hljs${langClass}">${body}</code></pre>`;
}

marked.use({ renderer: { code: renderCode } });

marked.setOptions({ gfm: true, breaks: false, async: false });

/**
 * How the reading view of a vault note shows its wikilinks and tags (`docs/plans/tags-links.md`, TL2): a wikilink
 * becomes `<a class="ax-wikilink" href="#ax-note:…">` (with `ax-wikilink-missing` when its note does not exist), a tag
 * `<a class="ax-tag" href="#ax-tag:…">` — fragment links the sanitiser keeps, which the view catches on click; nothing
 * in them navigates by itself.
 */
export interface NoteLinkRendering {
  /** Whether the note a link names exists: `false` dims it, `null` while the vault is not known. */
  exists(target: string): boolean | null;
}

/** Where a wikilink's and a tag's fragment start: what the view looks for in a clicked `href`. */
export const NOTE_HREF = "#ax-note:";
export const TAG_HREF = "#ax-tag:";

/** The rendering of the note being rendered right now — `marked` renders synchronously, one note at a time. */
let rendering: NoteLinkRendering | null = null;

/** Escapes text for an attribute value or element text. */
function escapeText(text: string): string {
  return escapeHtml(text).replace(/"/g, "&quot;");
}

const WIKILINK_RE = /^\[\[([^[\]\n]+?)\]\]/;
const TAG_RE = /^#([\p{L}\p{M}\p{N}_\-/]+)/u;

/** `[[Target#Heading|alias]]` (Obsidian's form), as `fileapp/noteLinks.ts` reads it. */
const wikilinkExtension: TokenizerAndRendererExtension = {
  name: "wikilink",
  level: "inline",
  start: (src) => {
    const at = src.indexOf("[[");
    return at < 0 ? undefined : at;
  },
  tokenizer(src) {
    if (this.lexer.state.inLink) return undefined;
    const m = WIKILINK_RE.exec(src);
    if (!m) return undefined;
    const inner = m[1];
    const bar = inner.indexOf("|");
    const head = bar < 0 ? inner : inner.slice(0, bar);
    const alias = bar < 0 ? "" : inner.slice(bar + 1).trim();
    const hash = head.indexOf("#");
    const target = (hash < 0 ? head : head.slice(0, hash)).trim();
    const heading = hash < 0 ? "" : head.slice(hash + 1).trim();
    if (!target && !heading) return undefined;
    return { type: "wikilink", raw: m[0], inner, target, heading, alias };
  },
  renderer(token) {
    const { inner, target, heading, alias } = token as unknown as Record<string, string>;
    const shown = alias || (target && heading ? `${target} › ${heading}` : target || heading);
    const missing = target && rendering?.exists(target) === false ? " ax-wikilink-missing" : "";
    const href = `${NOTE_HREF}${encodeURIComponent(inner)}`;
    return `<a class="ax-wikilink${missing}" href="${href}" title="${escapeText(target || heading)}">${escapeText(shown)}</a>`;
  },
};

/** `#tag` at the start or after whitespace, not digits alone (Q1). */
const tagExtension: TokenizerAndRendererExtension = {
  name: "hashtag",
  level: "inline",
  start: (src) => {
    for (let i = src.indexOf("#"); i >= 0; i = src.indexOf("#", i + 1)) {
      if ((i === 0 || /\s/u.test(src[i - 1])) && TAG_RE.test(src.slice(i))) return i;
    }
    return undefined;
  },
  tokenizer(src) {
    if (this.lexer.state.inLink) return undefined;
    const m = TAG_RE.exec(src);
    if (!m || /^\p{N}+$/u.test(m[1])) return undefined;
    return { type: "hashtag", raw: m[0], tag: m[1] };
  },
  renderer(token) {
    const tag = String((token as unknown as { tag: string }).tag);
    return `<a class="ax-tag" href="${TAG_HREF}${encodeURIComponent(tag.toLowerCase())}">#${escapeText(tag)}</a>`;
  },
};

/** A second `marked` for vault notes, so the chat and other Markdown never turn `#` or `[[` into links. */
const noteMarked = new Marked(
  { gfm: true, breaks: false, async: false },
  { renderer: { code: renderCode }, extensions: [wikilinkExtension, tagExtension] },
);

const ALLOWED_TAGS = [
  "a", "p", "br", "hr", "em", "strong", "del", "s", "code", "pre", "kbd",
  "blockquote", "ul", "ol", "li", "h1", "h2", "h3", "h4", "h5", "h6",
  "table", "thead", "tbody", "tr", "th", "td", "img", "input", "span", "div", "sup", "sub",
];
const ALLOWED_ATTR = ["href", "title", "alt", "src", "align", "start", "type", "checked", "disabled", "class"];

/** Inline raster images only — never SVG (it can carry script) and never
 *  as a link target (a top-level `data:` navigation would run it). Extension
 *  list mirrors the Rust `image_mime` (`workspace.rs`) — keep them in
 *  lockstep, same as `fileapp/fileKinds.ts`'s own copy of this list. */
const DATA_IMAGE_RE = /^data:image\/(?:png|jpe?g|gif|webp|bmp|tiff|heic|avif);base64,[a-z0-9+/=]+$/i;

/** The one configured DOMPurify instance. */
function sanitizer() {
  const instance = DOMPurify();
  instance.setConfig({
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOW_DATA_ATTR: false,
    FORBID_ATTR: ["style"],
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|\/|\.|data:image\/(?:png|jpe?g|gif|webp|bmp|tiff|heic|avif);base64,)/i,
  });
  addHooks(instance);
  return instance;
}

function addHooks(purify: ReturnType<typeof DOMPurify>): void {
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
    // Task-list checkboxes render read-only. DOMPurify drops `type` on an
    // input even when allowed, which left a text field in the checkbox's
    // place; Markdown makes no other input, so every one is a checkbox.
    node.setAttribute("type", "checkbox");
    node.setAttribute("disabled", "");
  }
});
}

const purify = sanitizer();

export function renderMarkdown(source: string): string {
  const html = marked.parse(source) as string;
  return purify.sanitize(html);
}

/**
 * Like `renderMarkdown`, but every top-level block is wrapped in
 * `<div data-line="n">` with its zero-based source line — what the editor's
 * preview (`docs/plans/editor.md`, G8) scrolls in step with the source by.
 * Each block is sanitised on its own with the ordinary rules and wrapped only
 * afterwards, so `data-line` is never something a note's own HTML can produce:
 * every `[data-line]` in the result is one of these wrappers (ED2 audit).
 * With `notes` (a vault note), its wikilinks and tags become links ({@link NoteLinkRendering}).
 */
export function renderMarkdownBlocks(source: string, notes: NoteLinkRendering | null = null): string {
  const engine = notes ? noteMarked : marked;
  rendering = notes;
  try {
    const tokens = engine.lexer(source);
    let line = 0;
    const parts: string[] = [];
    for (const token of tokens) {
      if (token.type !== "space") {
        const one = Object.assign([token], { links: tokens.links });
        parts.push(`<div data-line="${line}">${purify.sanitize(engine.parser(one))}</div>`);
      }
      line += (token.raw.match(/\n/g) ?? []).length;
    }
    return parts.join("");
  } finally {
    rendering = null;
  }
}

/** Word/line-boundary truncation for a plain-text preview — `core/mail.ts`'s
 *  `summaryPreview` (the Second Brain's own excerpts gave way to the editor's
 *  file peek in ED4.2). */
export function cut(text: string, max: number): string {
  if (text.length <= max) return text;
  const slice = text.slice(0, max);
  const at = Math.max(slice.lastIndexOf(" "), slice.lastIndexOf("\n"));
  return `${slice.slice(0, at > max * 0.6 ? at : max).trimEnd()}…`;
}
