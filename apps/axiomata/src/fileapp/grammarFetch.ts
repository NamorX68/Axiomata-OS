/**
 * Telling a real query file from the "page" a server answers a missing one with.
 *
 * The dev server (and the packaged app, for a path it does not know) answers an absent file with the app's
 * `index.html` and status 200, which must not be mistaken for a query (an absent `*.local.scm`, say). The
 * `Content-Type` cannot tell them apart: the packaged app derives it from the file extension and serves every
 * unknown one — `.scm` among them — as `text/html` (tauri-utils `MimeType`), so every real query looked absent and
 * nothing was coloured. The body decides: tree-sitter queries are S-expressions and comments, never markup.
 */

/** The query text, or `null` when `body` is an HTML page rather than a query. */
export function queryTextOrNull(body: string): string | null {
  return /^\s*<(?:!doctype|html)\b/i.test(body) ? null : body;
}
