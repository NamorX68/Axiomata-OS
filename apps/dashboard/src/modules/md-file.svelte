<!--
  md-file ("Document") — the viewer/editor for one workspace file, and (via
  `isNew`) the compose UI for a brand new one. Every text-ish file can be
  edited; only images are view-only.
  - `.md` / `.markdown` / …: read mode renders through core/markdown (marked +
    DOMPurify), edit mode is a textarea, Save writes via `write_workspace_file`.
  - Any other UTF-8 text file (`.txt`, `.json`, `.csv`, source code, no
    extension, …): read mode is a monospace `<pre>`, edit mode a textarea,
    Save the same. A non-UTF-8 (binary) file fails `read_workspace_file` and
    its error is shown — that's the "not a text file" refusal.
  - `.html` / `.htm` (courses): read mode is a `sandbox="allow-scripts"`
    iframe via `srcdoc` — **not** the asset protocol (`asset://` + `src=`),
    which was the original design (see git history) but proved unreliable:
    every lesson rendered a blank white frame with "Failed to load resource:
    403 (Forbidden)" / "Not allowed to download due to sandboxing" in the
    WebKit console, reproducibly, across a fresh app relaunch, a dynamic
    `asset_protocol_scope().allow_directory()` grant confirmed `is_allowed ==
    true` on the Rust side, *and* a static `tauri.conf.json` scope entry —
    none of it made a difference (owner-reported and jointly diagnosed via
    Safari's Web Inspector, 2026-09-04; matches reports against Tauri's
    asset/custom-protocol handling inside sandboxed iframes, e.g.
    tauri-apps/tauri#12767). `read_workspace_file` (already proven reliable —
    it powers the Second Brain's content preview) reads the raw HTML;
    `core/htmllink.withNavIntercept` appends a small script that intercepts
    clicks on same-page-relative links (a `srcdoc` iframe has no URL of its
    own for the browser to resolve `href="0003-next.html"` against) and
    posts `{source: "ax-md-file", href}` to the parent via `postMessage`;
    `onMessage` below checks `event.source` against this instance's own
    iframe (`iframeEl.contentWindow`) before resolving the link with
    `core/htmllink.resolveRelativeLink` and calling `open()` on it — reusing
    the exact same `load()`/`open()` plumbing as opening any other file.
    Known limits: navigating inside the frame updates the path in the bar
    now (an improvement over the asset-protocol version), but external
    links and anything with a URL scheme (`http:`, `mailto:`, …) are left
    alone and simply do nothing inside a frame with no `allow-top-navigation`
    and no network access of its own beyond what `srcdoc` inlines. Edit mode
    swaps the iframe for a raw-HTML textarea; Save rebuilds the preview.
  - `.png` / `.jpg` / `.jpeg` / `.gif` / `.webp` / `.bmp` / `.tif` / `.tiff` /
    `.heic` / `.heif` / `.avif`: shown read-only as a plain `<img>` fed by
    `read_workspace_image`'s base64 `data:` URI — the same backend call
    `core/markdownImages.ts` already uses to inline an image *referenced from
    inside* a note, now also used when the opened file *is* the image itself.
    Before this branch existed, opening one of these directly fell through to
    the text path below, which reads the file as UTF-8 and fails with "not
    valid UTF-8" for any binary file — exactly the bug this fixes. Extension
    list must stay in lockstep with the Rust `image_mime` (`workspace.rs`)
    and `core/markdown.ts`'s `DATA_IMAGE_RE`; anything outside it (SVG
    included, deliberately — see `image_mime`'s doc comment) is treated as a
    text document and shows its source.
    Whether HEIC/TIFF/BMP/AVIF actually *render* is up to WKWebView's own
    image decoder, not this app — see `image_mime`'s doc comment for the
    caveat (HEIC in particular is a known WebKit web-content gap even though
    the OS decodes it fine elsewhere); verify each in `cargo tauri dev`.
  - `isNew` + no `path` yet: a bare textarea, no title field — either the
    note starts with its own `# Heading`, or the agent proposes one, exactly
    like `axiomata_core::notes`. Save calls `create_note`, then re-points
    `config.path` at the file it wrote, which falls straight through into
    the normal read-mode viewer above.
  Config: `path` (workspace-relative), `mode` ("read" | "edit"), `isNew`,
  `stageFrom`.
-->
<script lang="ts">
  import { type WorkspaceFile, type WorkspaceImage } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { resolveRelativeLink, withNavIntercept } from "../core/htmllink";
  import { renderMarkdown } from "../core/markdown";
  import { resolveMarkdownImages } from "../core/markdownImages";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let file = $state<WorkspaceFile | null>(null);
  let draft = $state("");
  let error = $state("");
  let saving = $state(false);
  let pathInput = $state("");
  let iframeEl = $state<HTMLIFrameElement | null>(null);
  let imageSrc = $state<string | null>(null);

  const path = $derived(typeof $config.path === "string" ? $config.path : "");
  const isNew = $derived($config.isNew === true && !path);
  // Image extension list mirrors the Rust `image_mime` (`workspace.rs`) and
  // `core/markdown.ts`'s `DATA_IMAGE_RE` — keep all three in lockstep.
  // Everything that isn't an image, an HTML page, or a Markdown file is a
  // plain "text" document: read as monospace, edited in a textarea. A
  // non-UTF-8 (binary) file fails `read_workspace_file` and shows its error.
  const kind = $derived(
    /\.(?:png|jpe?g|gif|webp|bmp|tiff?|heic|heif|avif)$/i.test(path)
      ? "image"
      : /\.html?$/i.test(path)
        ? "html"
        : /\.(?:md|markdown|mdown|mkd|mkdn)$/i.test(path)
          ? "markdown"
          : "text",
  );
  /** Every text-ish kind can be edited; only images are view-only. */
  const editable = $derived(kind === "markdown" || kind === "text" || kind === "html");
  const mode = $derived($config.mode === "edit" && editable ? "edit" : "read");
  let frameDoc = $state<string | null>(null);
  let reloadTick = $state(0);
  const dirty = $derived(file !== null && draft !== file.content);

  /** Word count / reading time for the bar — Markdown read mode only. ~200
   *  wpm, rounded, floor of 1. */
  const wordCount = $derived(
    kind === "markdown" && mode === "read" && file ? file.content.trim().split(/\s+/).filter(Boolean).length : 0,
  );
  const readingMins = $derived(Math.max(1, Math.round(wordCount / 200)));
  /** Line count for the bar — plain-text kind, either mode. */
  const lineCount = $derived(kind === "text" && file ? file.content.split("\n").length : 0);

  // Rendering is async (relative image references are resolved to inline
  // `data:` URIs first — see `resolveMarkdownImages`), so `html` is state
  // recomputed by an effect rather than a plain `$derived`. `cancelled`
  // guards against a slower, superseded resolution overwriting a newer one
  // (switching files quickly, or a reload mid-flight).
  let html = $state("");
  $effect(() => {
    const source = kind === "markdown" && mode === "read" ? file?.content : undefined;
    if (!source) {
      html = "";
      return;
    }
    let cancelled = false;
    void resolveMarkdownImages(source, path, ctx.invoke).then((resolved) => {
      if (!cancelled) html = renderMarkdown(resolved);
    });
    return () => {
      cancelled = true;
    };
  });

  async function load(rel: string) {
    if (!rel) {
      file = null;
      frameDoc = null;
      imageSrc = null;
      return;
    }
    if (/\.(?:png|jpe?g|gif|webp|bmp|tiff?|heic|heif|avif)$/i.test(rel)) {
      await loadImage(rel);
      return;
    }
    if (/\.html?$/i.test(rel)) {
      await loadHtml(rel);
      return;
    }
    try {
      file = await ctx.invoke<WorkspaceFile>("read_workspace_file", { rel });
      draft = file.content;
      error = "";
    } catch (err) {
      file = null;
      error = String(err);
    }
  }

  async function loadHtml(rel: string) {
    try {
      const f = await ctx.invoke<WorkspaceFile>("read_workspace_file", { rel });
      // `file`/`draft` back the edit-mode textarea + Save; `frameDoc` is the
      // read-mode sandboxed preview.
      file = f;
      draft = f.content;
      frameDoc = withNavIntercept(f.content);
      reloadTick++;
      error = "";
    } catch (err) {
      file = null;
      frameDoc = null;
      error = String(err);
    }
  }

  async function loadImage(rel: string) {
    file = null;
    imageSrc = null;
    try {
      const img = await ctx.invoke<WorkspaceImage>("read_workspace_image", { rel });
      imageSrc = `data:${img.mime};base64,${img.base64}`;
      error = "";
    } catch (err) {
      imageSrc = null;
      error = String(err);
    }
  }

  // A click on a same-page-relative link inside the srcdoc'd page (see the
  // header comment) — only from *this* instance's own iframe, since
  // `postMessage` targets the whole window and several Document instances
  // could be mounted at once.
  function onFrameMessage(e: MessageEvent) {
    if (!iframeEl || e.source !== iframeEl.contentWindow) return;
    const data = e.data as { source?: string; href?: string } | null;
    if (!data || data.source !== "ax-md-file" || typeof data.href !== "string") return;
    open(resolveRelativeLink(path, data.href));
  }

  function setMode(next: "read" | "edit") {
    config.update((c) => ({ ...c, mode: next }));
  }

  function open(rel: string) {
    const clean = rel.trim();
    if (!clean) return;
    config.update((c) => ({ ...c, path: clean, mode: "read" }));
  }

  async function save() {
    if (!file || saving) return;
    saving = true;
    try {
      await ctx.invoke("write_workspace_file", { rel: file.path, content: draft });
      file = { ...file, content: draft, modified: new Date().toISOString() };
      // Refresh the read-mode preview to what was just written.
      if (kind === "html") {
        frameDoc = withNavIntercept(draft);
        reloadTick++;
      }
      error = "";
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }

  // Compose mode (`isNew`): the agent decides where the note lands
  // (`axiomata_core::notes`) — re-pointing `config.path` at the result hands
  // this same module straight over to the normal read-mode viewer above.
  async function saveNew() {
    if (saving || !draft.trim()) return;
    saving = true;
    try {
      const rel = await ctx.invoke<string>("create_note", { content: draft });
      config.update((c) => ({ ...c, path: rel, mode: "read", isNew: false }));
      draft = "";
      error = "";
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }

  // Reload whenever the configured path changes (flip side, action, open()).
  $effect(() => {
    void load(path);
  });
</script>

<svelte:window onmessage={kind === "html" ? onFrameMessage : undefined} />

<div class="md">
  {#if isNew}
    <div class="bar">
      <span class="path muted">New note</span>
      <span class="spacer"></span>
      <button type="button" class="save" onclick={saveNew} disabled={!draft.trim() || saving}>
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
    {#if error}<p class="error">{error}</p>{/if}
    <textarea
      bind:value={draft}
      spellcheck="false"
      placeholder={"# Title (optional)\n\nWrite your note in Markdown. Give it a heading yourself, or leave it out and the agent will name it. Either way, it decides where in the vault this lands when you save."}
    ></textarea>
  {:else if !path}
    <form class="ask" onsubmit={(e) => (e.preventDefault(), open(pathInput))}>
      <p class="muted">Open a file from the workspace:</p>
      <div class="pair">
        <input type="text" placeholder="notes/inbox.md" bind:value={pathInput} />
        <button type="submit">Open</button>
      </div>
    </form>
  {:else}
    <div class="bar">
      <span class="path" title={path}>{path}</span>
      {#if file?.modified}<span class="muted">· {relativeTime(file.modified)}</span>{/if}
      {#if wordCount > 0}<span class="muted">· {wordCount} words · {readingMins} min</span>{/if}
      {#if lineCount > 0}<span class="muted">· {lineCount} {lineCount === 1 ? "line" : "lines"}</span>{/if}
      <span class="spacer"></span>
      <button type="button" onclick={() => ctx.emit("open-second-brain", { focus: `file:${path}` })} title="Open in Second Brain">In Second Brain</button>
      {#if kind === "image"}
        <!-- image: view-only -->
      {:else if mode === "read"}
        <button type="button" onclick={() => setMode("edit")} disabled={!file} title="Edit">Edit</button>
      {:else}
        <button type="button" onclick={() => setMode("read")} title="Back to read view">Read</button>
        <button type="button" class="save" onclick={save} disabled={!dirty || saving} title="Save">
          {saving ? "Saving…" : "Save"}
        </button>
      {/if}
      <button type="button" title="Reload" aria-label="Reload" onclick={() => load(path)}>↻</button>
    </div>
    {#if error}
      <p class="error">{error}</p>
    {:else if mode === "edit"}
      {#if file}
        <textarea bind:value={draft} spellcheck="false"></textarea>
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    {:else if kind === "html"}
      {#if frameDoc}
        {#key reloadTick}
          <iframe
            class="page"
            title={path}
            sandbox="allow-scripts"
            referrerpolicy="no-referrer"
            srcdoc={frameDoc}
            bind:this={iframeEl}
          ></iframe>
        {/key}
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    {:else if kind === "image"}
      {#if imageSrc}
        <div class="image-view"><img src={imageSrc} alt={path} /></div>
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    {:else if !file}
      <p class="muted">Loading…</p>
    {:else if kind === "text"}
      <pre class="code-view">{file.content}</pre>
    {:else}
      <div class="rendered">{@html html}</div>
    {/if}
  {/if}
</div>

<style>
  .md {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .ask {
    padding: var(--ax-space-3);
  }
  .pair {
    display: flex;
    gap: var(--ax-space-2);
  }
  .pair input {
    flex: 1 1 auto;
    min-width: 0;
    font-family: var(--ax-font-mono);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
    flex: 0 0 auto;
  }
  .bar button {
    padding: 2px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    /* The plain global button style (--ax-surface-3 on --ax-border) reads
       too close to the panel's own --ax-surface-1 background to notice at
       a glance — a stronger border gives it real edges without going as
       loud as the accent-coloured Save button. */
    border-color: var(--ax-border-strong);
  }
  .path {
    font-family: var(--ax-font-mono);
    color: var(--ax-accent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    /* Without this, a flex item's minimum width defaults to its full
       unwrapped content size — a long path would refuse to shrink at all,
       pushing the buttons after it out of the bar instead of ellipsizing. */
    min-width: 0;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .save:not(:disabled) {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  textarea {
    flex: 1 1 auto;
    min-height: 0;
    resize: none;
    border: none;
    border-radius: 0;
    background: var(--ax-surface-1);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    line-height: 1.55;
    padding: var(--ax-space-3);
  }

  .page {
    flex: 1 1 auto;
    min-height: 0;
    width: 100%;
    border: 0;
    background: var(--ax-surface-1);
  }

  .image-view {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: var(--ax-surface-1);
  }
  .image-view img {
    max-width: 100%;
    height: auto;
  }

  .code-view {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    line-height: 1.55;
    white-space: pre;
    tab-size: 2;
    user-select: text;
  }

  .rendered {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    user-select: text;
  }
  .rendered :global(h1),
  .rendered :global(h2),
  .rendered :global(h3) {
    margin: var(--ax-space-3) 0 var(--ax-space-2);
  }
  .rendered :global(h1) {
    font-size: var(--ax-font-size-xl);
    padding-bottom: var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
  }
  .rendered :global(h2) {
    font-size: var(--ax-font-size-lg);
    color: var(--ax-accent);
  }
  .rendered :global(hr) {
    margin: var(--ax-space-3) 0;
    border: none;
    border-top: 1px solid var(--ax-border);
  }
  .rendered :global(kbd) {
    font-family: var(--ax-font-mono);
    font-size: 0.85em;
    padding: 1px 5px;
    border: 1px solid var(--ax-border-strong);
    border-bottom-width: 2px;
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
  }
  .rendered :global(li::marker) {
    color: var(--ax-accent);
  }
  .rendered :global(p),
  .rendered :global(ul),
  .rendered :global(ol),
  .rendered :global(pre),
  .rendered :global(table),
  .rendered :global(blockquote) {
    margin: 0 0 var(--ax-space-3);
  }
  .rendered :global(code) {
    font-family: var(--ax-font-mono);
    font-size: 0.92em;
    background: var(--ax-surface-3);
    padding: 1px 4px;
    border-radius: var(--ax-radius-sm);
  }
  .rendered :global(pre) {
    padding: var(--ax-space-3);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-md);
    overflow: auto;
  }
  .rendered :global(pre code) {
    background: none;
    padding: 0;
  }
  .rendered :global(blockquote) {
    padding: var(--ax-space-2) var(--ax-space-3);
    border-left: 3px solid var(--ax-accent);
    border-radius: 0 var(--ax-radius-sm) var(--ax-radius-sm) 0;
    background: var(--ax-accent-muted);
    color: var(--ax-text-muted);
  }
  .rendered :global(table) {
    border-collapse: collapse;
  }
  .rendered :global(th),
  .rendered :global(td) {
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
  }
  .rendered :global(th) {
    background: var(--ax-surface-3);
    color: var(--ax-accent);
    text-align: left;
  }
  .rendered :global(a) {
    color: var(--ax-accent);
  }
  .rendered :global(input[type="checkbox"]) {
    accent-color: var(--ax-accent);
    margin-right: var(--ax-space-1);
  }
  .rendered :global(img) {
    max-width: 100%;
  }

  p {
    margin: 0;
    padding: var(--ax-space-3);
  }
  .ask p {
    padding: 0 0 var(--ax-space-2);
  }
  .bar .muted {
    padding: 0;
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
  }
</style>
