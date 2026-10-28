/**
 * Relative-link navigation for HTML course pages framed via `srcdoc` (see
 * `fileapp/HtmlPreview.svelte`). A `srcdoc` document's own URL is `about:srcdoc`,
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
 *    reloading the whole app shell *inside* the lesson frame, which
 *    then fails on CORS (opaque `null` origin) and renders blank
 *    (owner-reported, 2026-09-04). Intercepted and handled manually with a
 *    plain `getElementById` + `scrollIntoView`, never letting the browser's
 *    own anchor-navigation logic run at all.
 *
 * 3. A link with a scheme (`https:`, `mailto:`) would navigate the frame, which the CSP's `frame-src` refuses: it is
 *    handed to the parent as an external link, which opens it in the browser (`HtmlPreview.svelte`).
 *
 * **The CSP.** A `srcdoc` document inherits the app's Content-Security-Policy, whose `script-src` allows no inline
 * script — the page's own scripts (a lesson's quiz) and this module's intercept were blocked in the installed app,
 * and a link click navigated the frame to a blank page (owner-reported, 2026-10-28). Tauri fills a fresh nonce into
 * `index.html` on every load (the placeholder `vite.config.ts` puts on its tags) and allows it in the policy;
 * [`pageScriptNonce`] reads it, and [`withNavIntercept`] gives it to every script of the page and to its own. Only the
 * page being shown runs scripts, in its sandbox.
 *
 * `resolveRelativeLink` turns an `href` from case 1 into a workspace-relative
 * path using the real WHATWG URL algorithm (the same approach validated for
 * `core/backend.assetFileUrl`), based on the currently-open file's own path
 * standing in for the "page URL".
 */

/** The `source` of the message the framed page posts for a clicked link. */
export const PAGE_MESSAGE_SOURCE = "ax-page";

/** The placeholder Tauri replaces with the load's nonce (put on `index.html`'s tags by `vite.config.ts`,
 *  `html.cspNonce`); left as it is where no Tauri served the page. */
const NONCE_PLACEHOLDER = "__TAURI_SCRIPT_NONCE__";

/** The nonce the app's CSP allows scripts by on this load, or `null` where there is none (the vite mock, a test). */
export function pageScriptNonce(doc: Document = document): string | null {
  const nonce =
    doc.querySelector<HTMLScriptElement>("script[nonce]")?.nonce ?? "";
  // Tauri's nonce is a decimal number; anything else is no nonce of ours, and goes nowhere near an attribute.
  return nonce !== NONCE_PLACEHOLDER && /^\d{1,20}$/.test(nonce) ? nonce : null;
}

/** The `source` of the message the file window posts *into* the framed page to scale it (`{ source, zoom }`). */
export const ZOOM_MESSAGE_SOURCE = "ax-zoom";

// The zoom listener takes messages from the parent only, and sets the page's CSS `zoom` — a resize of the window's
// text then scales the page in place, without reloading it (which would lose its scroll position and script state).
const NAV_SCRIPT = `<script>document.addEventListener("click",function(e){var a=e.target&&e.target.closest?e.target.closest("a[href]"):null;if(!a)return;var href=a.getAttribute("href");if(!href)return;if(href.charAt(0)==="#"){e.preventDefault();var id=href.slice(1);var el=id?document.getElementById(id):null;if(el)el.scrollIntoView({behavior:"smooth",block:"start"});return;}if(/^[a-z][a-z0-9+.-]*:/i.test(href)){if(/^(https?|mailto):/i.test(href)){e.preventDefault();parent.postMessage({source:"ax-page",href:href,external:true},"*");}return;}e.preventDefault();parent.postMessage({source:"ax-page",href:href},"*");});window.addEventListener("message",function(e){var d=e.data;if(e.source!==parent||!d||d.source!=="ax-zoom"||typeof d.zoom!=="number"||!(d.zoom>0))return;document.documentElement.style.zoom=String(d.zoom);});</script>`;

/** Appends the click-intercept script just before `</body>` (or at the end
 *  if the page has none — course pages always do, but don't assume it). With a `nonce` (the app's, see
 *  [`pageScriptNonce`]), every inline script of the page that has none of its own gets it, and so does the intercept,
 *  so the CSP lets them run. A script loaded by `src` does not: a nonced element ignores the policy's host list, and a
 *  page would load code from anywhere on the net, which `'self'` refuses. */
export function withNavIntercept(
  html: string,
  nonce: string | null = null,
): string {
  const attr = nonce ? ` nonce="${nonce}"` : "";
  const page = nonce
    ? html.replace(
        /<script\b(?![^>]*\s(?:nonce|src)\s*=)/gi,
        (tag) => tag + attr,
      )
    : html;
  const script = NAV_SCRIPT.replace("<script>", () => `<script${attr}>`);
  return /<\/body>/i.test(page)
    ? page.replace(/<\/body>/i, () => `${script}</body>`)
    : page + script;
}

/** Whether `href`, posted by a framed page as an external link, may be opened in the browser: web and mail links
 *  only — never `javascript:`, `file:` or an app scheme, and never a service on this Mac (`localhost`, `127.…`,
 *  `[::1]`), which a page script could otherwise call through the browser. */
export function isOpenableExternal(href: string): boolean {
  if (/^mailto:/i.test(href)) return true;
  if (!/^https?:\/\//i.test(href)) return false;
  let host: string;
  try {
    host = new URL(href).hostname.toLowerCase();
  } catch {
    return false;
  }
  return !(
    host === "localhost" ||
    host.endsWith(".localhost") ||
    host.startsWith("127.") ||
    host === "[::1]" ||
    host === "0.0.0.0"
  );
}

/** The least time between two external links a framed page may open, in ms: a page script posting them in a loop
 *  opens one, not a flood of browser tabs. */
export const EXTERNAL_LINK_INTERVAL_MS = 2000;

/** Resolves `href` (as clicked inside the framed page) against `currentPath`
 *  (the workspace-relative path of the page that contains the link), e.g.
 *  `resolveRelativeLink("Learning/Rust/lessons/0002-x.html", "0003-y.html")`
 *  → `"Learning/Rust/lessons/0003-y.html"`. Handles `../` the same way a
 *  real browser would, via `URL`'s own relative-resolution algorithm.
 *
 *  `null` for a link that still climbs out after decoding: `URL` collapses a
 *  literal `../`, but not one spelled `..%2f`, which decodes into `../`. The
 *  file service refuses such a path too; this refuses it where it is made. */
export function resolveRelativeLink(
  currentPath: string,
  href: string,
): string | null {
  const dir = currentPath.includes("/")
    ? currentPath.slice(0, currentPath.lastIndexOf("/") + 1)
    : "";
  const resolved = new URL(href, `file:///${dir}`);
  let decoded: string;
  try {
    decoded = decodeURIComponent(resolved.pathname).replace(/^\/+/, "");
  } catch {
    return null; // A malformed escape (`%E0%A4%A`) names no file.
  }
  return decoded.split("/").some((segment) => segment === "..")
    ? null
    : decoded;
}
