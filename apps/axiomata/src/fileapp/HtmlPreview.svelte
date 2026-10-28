<!--
  The rendered view of an HTML file (`docs/plans/editor.md`, ED4, W3): the page
  in a sandboxed `srcdoc` iframe — scripts run, but in an opaque origin with no
  access to the app — the way the Document viewer showed course pages before.
  `srcdoc`, not `asset://`: see the postmortem in `docs/architecture.md` §5.

  A click on a relative link inside the page (the next lesson) arrives as a
  message (`core/htmllink.ts`) and is handed to the owner as a file to open,
  resolved against this page's own path; a web or mail link opens in the browser. The page's scripts run under the
  app's CSP by the load's nonce (`pageScriptNonce`). The page is re-rendered a moment
  after the source stops changing, not on every keystroke.
-->
<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";

  import {
    PAGE_MESSAGE_SOURCE,
    EXTERNAL_LINK_INTERVAL_MS,
    isOpenableExternal,
    pageScriptNonce,
    resolveRelativeLink,
    withNavIntercept,
    ZOOM_MESSAGE_SOURCE,
  } from "../core/htmllink";

  interface Props {
    /** The page's source. */
    text: string;
    /** Its path, for resolving the links inside it. */
    rel: string;
    /** A relative link was clicked: the file it points to, relative to the same root. */
    onOpenLink?: (rel: string) => void;
    /** Scale of the page (the file window's text size against its base); the page is left as it is without it. */
    zoom?: number;
  }

  let { text, rel, onOpenLink, zoom }: Props = $props();

  /** Pause in typing before the page is rendered again. */
  const RENDER_DELAY_MS = 300;

  let frame = $state<HTMLIFrameElement | null>(null);
  let doc = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const source = text;
    if (timer) clearTimeout(timer);
    // The app's nonce for the CSP, so the page's own scripts and the intercept run (`core/htmllink.ts`).
    timer = setTimeout(() => (doc = withNavIntercept(source, pageScriptNonce())), doc ? RENDER_DELAY_MS : 0);
    return () => clearTimeout(timer);
  });

  /** Hands the zoom to the page's own script; a freshly loaded page starts at 1 and is told again on `load`. */
  function sendZoom(): void {
    if (zoom === undefined) return;
    frame?.contentWindow?.postMessage({ source: ZOOM_MESSAGE_SOURCE, zoom }, "*");
  }

  $effect(() => {
    void zoom;
    sendZoom();
  });

  /** When this page last opened an external link (`EXTERNAL_LINK_INTERVAL_MS`). */
  let lastExternal = 0;

  /** Only this frame's messages: several pages may be open at once. */
  function onMessage(e: MessageEvent): void {
    if (!frame || e.source !== frame.contentWindow) return;
    const data = e.data as { source?: string; href?: string; external?: boolean } | null;
    if (!data || data.source !== PAGE_MESSAGE_SOURCE || typeof data.href !== "string") return;
    if (data.external === true) {
      // Only while the page has the focus (a click went into it), and one at a time: a page's own script could post
      // these by itself — the parent cannot tell its message from a click's.
      const now = Date.now();
      if (document.activeElement !== frame || now - lastExternal < EXTERNAL_LINK_INTERVAL_MS) return;
      if (!isOpenableExternal(data.href)) return;
      lastExternal = now;
      void openUrl(data.href).catch(() => {});
      return;
    }
    const target = resolveRelativeLink(rel, data.href);
    if (target !== null) onOpenLink?.(target);
  }
</script>

<svelte:window onmessage={onMessage} />

<iframe
  class="page"
  title={rel}
  sandbox="allow-scripts"
  referrerpolicy="no-referrer"
  srcdoc={doc}
  bind:this={frame}
  onload={sendZoom}
></iframe>

<style>
  .page {
    flex: 1;
    width: 100%;
    height: 100%;
    border: none;
    /* Behind a page that brings no background of its own. */
    background: var(--ax-surface-1);
  }
</style>
