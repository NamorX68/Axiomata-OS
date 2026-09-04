/**
 * Relative-link navigation for HTML course pages framed via `srcdoc` (see
 * `modules/md-file.svelte`). A `srcdoc` document's own URL is `about:srcdoc`,
 * but its *base URL* — what a relative `href` resolves against — is
 * inherited from the **embedding page**, per the HTML living standard. Two
 * consequences `withNavIntercept`'s injected script has to handle itself,
 * since the browser's native handling gets both wrong for framed content:
 *
 * 1. A same-folder link (`href="0003-next.html"`) has nothing of the
 *    lesson's own location to resolve against — the browser can't follow
 *    it. Intercepted and handed to the parent via `postMessage`, which
 *    resolves it against the *actual* file path (`resolveRelativeLink`) and
 *    opens it through the normal `load()`/`open()` path.
 * 2. A same-page anchor (`href="#etappe-a"`) is worse than merely
 *    unresolvable: WebKit resolves it against the embedding app's base URL
 *    (`http://localhost:1420/`, say) rather than treating it as an
 *    in-document scroll, and actually navigates the iframe there —
 *    reloading the whole dashboard shell *inside* the lesson frame, which
 *    then fails on CORS (opaque `null` origin) and renders blank
 *    (owner-reported, 2026-09-04). Intercepted and handled manually with a
 *    plain `getElementById` + `scrollIntoView`, never letting the browser's
 *    own anchor-navigation logic run at all.
 *
 * `resolveRelativeLink` turns an `href` from case 1 into a workspace-relative
 * path using the real WHATWG URL algorithm (the same approach validated for
 * `core/backend.assetFileUrl`), based on the currently-open file's own path
 * standing in for the "page URL".
 */

const NAV_SCRIPT = `<script>document.addEventListener("click",function(e){var a=e.target&&e.target.closest?e.target.closest("a[href]"):null;if(!a)return;var href=a.getAttribute("href");if(!href)return;if(href.charAt(0)==="#"){e.preventDefault();var id=href.slice(1);var el=id?document.getElementById(id):null;if(el)el.scrollIntoView({behavior:"smooth",block:"start"});return;}if(/^[a-z][a-z0-9+.-]*:/i.test(href))return;e.preventDefault();parent.postMessage({source:"ax-md-file",href:href},"*");});</script>`;

/** Appends the click-intercept script just before `</body>` (or at the end
 *  if the page has none — course pages always do, but don't assume it). */
export function withNavIntercept(html: string): string {
  return /<\/body>/i.test(html) ? html.replace(/<\/body>/i, `${NAV_SCRIPT}</body>`) : html + NAV_SCRIPT;
}

/** Resolves `href` (as clicked inside the framed page) against `currentPath`
 *  (the workspace-relative path of the page that contains the link), e.g.
 *  `resolveRelativeLink("Learning/Rust/lessons/0002-x.html", "0003-y.html")`
 *  → `"Learning/Rust/lessons/0003-y.html"`. Handles `../` the same way a
 *  real browser would, via `URL`'s own relative-resolution algorithm. */
export function resolveRelativeLink(currentPath: string, href: string): string {
  const dir = currentPath.includes("/") ? currentPath.slice(0, currentPath.lastIndexOf("/") + 1) : "";
  const resolved = new URL(href, `file:///${dir}`);
  return decodeURIComponent(resolved.pathname).replace(/^\/+/, "");
}
