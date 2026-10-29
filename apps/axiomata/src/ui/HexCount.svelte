<!--
  Something waits for the owner that is not an agent — today the tag proposals (`docs/plans/tags-links.md` TL4): the
  same hexagon in the `warn` tone as `WaitingBadge.svelte`, with the number, nudging under motion Full and still under
  Calm and Off. A click does what the caller says (open the place where it is answered). Nothing is drawn at zero.
-->
<script lang="ts">
  interface Props {
    count: number;
    label: string;
    size?: "sm" | "md";
    onclick?: () => void;
  }

  let { count, label, size = "sm", onclick }: Props = $props();
</script>

{#if count > 0}
  {#if onclick}
    <button
      type="button"
      class="badge {size}"
      title={label}
      aria-label={label}
      onclick={(event) => {
        event.stopPropagation();
        onclick();
      }}
    >
      <span class="hex">{count}</span>
    </button>
  {:else}
    <span class="badge {size}" role="img" title={label} aria-label={label}><span class="hex">{count}</span></span>
  {/if}
{/if}

<style>
  .badge {
    display: inline-grid;
    place-items: center;
    flex: none;
    padding: 0;
    border: none;
    border-radius: var(--ax-radius-md);
    background: transparent;
    cursor: pointer;
  }

  span.badge {
    cursor: default;
  }

  .badge:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 1px;
  }

  .hex {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    clip-path: var(--ax-hex-clip);
    background: var(--ax-warning);
    color: var(--ax-text-invert);
    font: 600 var(--ax-font-size-xs) / 1 var(--ax-font-sans);
    animation: var(--ax-anim-nudge);
  }

  .sm {
    width: calc(16px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
  }

  .sm .hex {
    font-size: calc(10px * var(--ax-ui-scale));
  }

  .md {
    width: calc(22px * var(--ax-ui-scale));
    height: calc(25px * var(--ax-ui-scale));
  }
</style>
