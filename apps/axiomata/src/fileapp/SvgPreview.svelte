<!--
  The rendered view of an SVG file (`docs/plans/editor.md`, ED4, W3): the source
  as a picture, through an `<img>` from a `data:` URL — an SVG shown that way
  runs no script and loads nothing, whatever it contains. Re-rendered a moment
  after the source stops changing.
-->
<script lang="ts">
  interface Props {
    text: string;
    rel: string;
  }

  let { text, rel }: Props = $props();

  const RENDER_DELAY_MS = 200;

  let src = $state("");
  let broken = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const source = text;
    if (timer) clearTimeout(timer);
    timer = setTimeout(
      () => {
        broken = false;
        src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(source)}`;
      },
      src ? RENDER_DELAY_MS : 0,
    );
    return () => clearTimeout(timer);
  });
</script>

<div class="svg-view">
  {#if broken}
    <p class="note">This SVG does not draw — check its source.</p>
  {:else if src}
    <img {src} alt={rel} onerror={() => (broken = true)} />
  {/if}
</div>

<style>
  .svg-view {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
    padding: var(--ax-space-5);
    overflow: auto;
    background: var(--ax-surface-1);
  }

  img {
    max-width: 100%;
    max-height: 100%;
  }

  .note {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
