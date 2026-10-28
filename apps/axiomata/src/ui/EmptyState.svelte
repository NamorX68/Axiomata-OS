<!--
  An empty place (`docs/plans/ui-ux-round.md` UX2): the hexagon of the system with an ember in it, what is missing in
  the title role, a sentence on how it fills, and the one action that fills it. The ember glows only while there is
  such an action — it is the "needs you" of the accent rule; without one the place is simply quiet.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    /** How the place fills. */
    text?: string;
    /** The one action that fills it (a primary `ax-btn`). */
    action?: Snippet;
    /** A smaller motif, for a side bar or a column. */
    compact?: boolean;
  }

  let { title, text, action, compact = false }: Props = $props();
</script>

<div class="empty" class:compact>
  <span class="art" aria-hidden="true">
    <span class="shell"></span>
    <span class="hollow"></span>
    <span class="ember" class:lit={action !== undefined}></span>
  </span>
  <p class="title">{title}</p>
  {#if text}<p class="text">{text}</p>{/if}
  {#if action}<div class="action">{@render action()}</div>{/if}
</div>

<style>
  .empty {
    display: grid;
    justify-items: center;
    align-content: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-6) var(--ax-space-4);
    text-align: center;
  }

  .compact {
    padding: var(--ax-space-4) var(--ax-space-3);
  }

  .art {
    position: relative;
    width: calc(64px * var(--ax-ui-scale));
    height: calc(72px * var(--ax-ui-scale));
    margin-bottom: var(--ax-space-2);
  }

  .compact .art {
    width: calc(40px * var(--ax-ui-scale));
    height: calc(45px * var(--ax-ui-scale));
    margin-bottom: 0;
  }

  .shell,
  .hollow,
  .ember {
    position: absolute;
    clip-path: var(--ax-hex-clip);
  }

  .shell {
    inset: 0;
    background: var(--ax-border-strong);
  }

  .hollow {
    inset: calc(1.5px * var(--ax-ui-scale));
    /* Solid even on glass: the hollow covers the shell, which would otherwise fill the whole hexagon. */
    background: var(--ax-solid-surface-1);
  }

  .ember {
    inset: 32% 32%;
    background: var(--ax-border-strong);
  }

  .ember.lit {
    background: var(--ax-glow);
    filter: var(--ax-glow-drop);
    animation: var(--ax-anim-breathe);
  }

  .title {
    margin: 0;
    font: var(--ax-type-title);
    color: var(--ax-text);
  }

  .compact .title {
    font: var(--ax-type-body);
    font-weight: 600;
  }

  .text {
    margin: 0;
    max-width: calc(380px * var(--ax-ui-scale));
    font: var(--ax-type-body);
    color: var(--ax-text-muted);
  }

  .action {
    display: flex;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
  }
</style>
