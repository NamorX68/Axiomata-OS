<!--
  md-file ("Document") — the viewer for one workspace file, and (via `isNew`)
  the compose UI for a brand new one.
  - `.md`: read mode renders through core/markdown (marked + DOMPurify), edit
    mode is a textarea, Save writes via `write_workspace_file`.
  - `.html` / `.htm` (courses): shown read-only in a sandboxed iframe whose src
    is an asset-protocol URL from `open_workspace_html` (which allows the
    page's folder in the asset scope, so same-folder links keep working) —
    built with `core/backend.assetFileUrl`, not the SDK's `convertFileSrc`
    (see its doc comment: that one breaks relative links inside the page).
    `sandbox="allow-scripts allow-same-origin"`: WebKit's custom-scheme
    protocol handler (asset://) appears to refuse to serve into a frame that
    sandboxing has forced to an opaque origin — with `allow-scripts` alone
    the whole frame rendered blank (owner-reported, 2026-09-04; matches
    reports against Tauri's asset/custom-protocol handling in sandboxed
    iframes, e.g. tauri-apps/tauri#12767). `allow-same-origin` here does NOT
    hand the page the app's own origin — it only lets it keep its *real*
    origin (`asset://localhost`), which is still a different origin from the
    app shell (served over `tauri://`/`http://tauri.localhost`), so it still
    cannot reach `parent.document` or the IPC bridge. Known limits:
    navigating inside the frame does not update the path in the bar; external
    links are blocked by the app's frame-src and show a blank frame. In the
    browser dev mock the page is loaded via `srcdoc` instead (no asset
    protocol there, and the mock frame never needs the same-origin fix since
    dev-mock content is trusted fixture data, not the sandboxed real path).
  - `isNew` + no `path` yet: a bare textarea, no title field — either the
    note starts with its own `# Heading`, or the agent proposes one, exactly
    like `axiomata_core::notes`. Save calls `create_note`, then re-points
    `config.path` at the file it wrote, which falls straight through into
    the normal read-mode viewer above.
  Config: `path` (workspace-relative), `mode` ("read" | "edit"), `isNew`,
  `stageFrom`.
-->
<script lang="ts">
  import { assetFileUrl, insideTauri, type WorkspaceFile } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { renderMarkdown } from "../core/markdown";
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

  const path = $derived(typeof $config.path === "string" ? $config.path : "");
  const isNew = $derived($config.isNew === true && !path);
  const kind = $derived(/\.html?$/i.test(path) ? "html" : "markdown");
  const mode = $derived($config.mode === "edit" && kind === "markdown" ? "edit" : "read");
  let frameSrc = $state<string | null>(null);
  let frameDoc = $state<string | null>(null);
  let reloadTick = $state(0);
  const dirty = $derived(file !== null && draft !== file.content);
  const html = $derived(file ? renderMarkdown(mode === "edit" ? draft : file.content) : "");

  async function load(rel: string) {
    if (!rel) {
      file = null;
      frameSrc = null;
      frameDoc = null;
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
    file = null;
    try {
      if (insideTauri()) {
        const abs = await ctx.invoke<string>("open_workspace_html", { rel });
        frameSrc = assetFileUrl(abs);
        frameDoc = null;
      } else {
        // Browser dev mock: no asset protocol — inline the fixture page.
        const f = await ctx.invoke<WorkspaceFile>("read_workspace_file", { rel });
        frameDoc = f.content;
        frameSrc = null;
      }
      reloadTick++;
      error = "";
    } catch (err) {
      frameSrc = null;
      frameDoc = null;
      error = String(err);
    }
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
      <p class="muted">Open a Markdown file from the workspace:</p>
      <div class="pair">
        <input type="text" placeholder="notes/inbox.md" bind:value={pathInput} />
        <button type="submit">Open</button>
      </div>
    </form>
  {:else}
    <div class="bar">
      <span class="path" title={path}>{path}</span>
      {#if file?.modified}<span class="muted">· {relativeTime(file.modified)}</span>{/if}
      <span class="spacer"></span>
      {#if kind === "html"}
        <button type="button" onclick={() => ctx.emit("open-second-brain", { focus: `file:${path}` })}>In Second Brain</button>
      {:else if mode === "read"}
        <button type="button" onclick={() => setMode("edit")} disabled={!file}>Edit</button>
      {:else}
        <button type="button" onclick={() => setMode("read")}>Read</button>
        <button type="button" class="save" onclick={save} disabled={!dirty || saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      {/if}
      <button type="button" title="Reload" aria-label="Reload" onclick={() => load(path)}>↻</button>
    </div>
    {#if error}
      <p class="error">{error}</p>
    {:else if kind === "html"}
      {#if frameSrc || frameDoc}
        {#key reloadTick}
          <iframe
            class="page"
            title={path}
            sandbox="allow-scripts allow-same-origin"
            referrerpolicy="no-referrer"
            src={frameSrc ?? undefined}
            srcdoc={frameDoc ?? undefined}
          ></iframe>
        {/key}
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    {:else if !file}
      <p class="muted">Loading…</p>
    {:else if mode === "edit"}
      <textarea bind:value={draft} spellcheck="false"></textarea>
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
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .path {
    font-family: var(--ax-font-mono);
    color: var(--ax-accent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
  }
  .rendered :global(h2) {
    font-size: var(--ax-font-size-lg);
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
    padding-left: var(--ax-space-3);
    border-left: 3px solid var(--ax-accent);
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
