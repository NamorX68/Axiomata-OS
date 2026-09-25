<!--
  A raster image in the file app (`docs/plans/editor.md`, ED4, W3): read through
  the file service (`file_read_image`, so any root works, not just the
  workspace), shown fitted to the view or at its own size, with its pixel size
  and file size underneath. View only — there is no text to edit.
-->
<script lang="ts">
  import { invokeBackend } from "../core/backend";
  import { messageOf } from "../core/errors";

  interface Props {
    root: string;
    rel: string;
  }

  let { root, rel }: Props = $props();

  let src = $state<string | null>(null);
  let bytes = $state(0);
  let error = $state("");
  let fit = $state(true);
  let size = $state<{ w: number; h: number } | null>(null);

  $effect(() => {
    const at = { root, rel };
    let cancelled = false;
    src = null;
    size = null;
    error = "";
    invokeBackend<{ mime: string; base64: string }>("file_read_image", at)
      .then((image) => {
        if (cancelled) return;
        src = `data:${image.mime};base64,${image.base64}`;
        // Base64 carries three bytes in every four characters, less its padding.
        bytes = Math.floor((image.base64.length * 3) / 4) - (image.base64.match(/=*$/)?.[0].length ?? 0);
      })
      .catch((err) => {
        if (!cancelled) error = messageOf(err);
      });
    return () => {
      cancelled = true;
    };
  });

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

<div class="image-view">
  <div class="canvas" class:fit>
    {#if error}
      <p class="note">{error}</p>
    {:else if src}
      <img
        {src}
        alt={rel}
        onload={(e) => {
          const img = e.currentTarget as HTMLImageElement;
          size = { w: img.naturalWidth, h: img.naturalHeight };
        }}
        onerror={() => (error = "This image cannot be shown here (the web view does not decode its format).")}
      />
    {:else}
      <p class="note">Loading…</p>
    {/if}
  </div>
  <footer>
    <button type="button" onclick={() => (fit = !fit)}>{fit ? "Actual size" : "Fit"}</button>
    {#if size}<span>{size.w} × {size.h} px</span>{/if}
    {#if bytes}<span>{formatBytes(bytes)}</span>{/if}
  </footer>
</div>

<style>
  .image-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--ax-surface-1);
  }

  .canvas {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--ax-space-4);
  }

  /* Actual size: large images scroll from their top-left corner. */
  .canvas:not(.fit) {
    align-items: flex-start;
    justify-content: flex-start;
  }

  .canvas.fit img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .note {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  footer {
    display: flex;
    gap: var(--ax-space-5);
    align-items: center;
    padding: var(--ax-space-1) var(--ax-space-5);
    border-top: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  footer button {
    padding: 0 var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }
</style>
