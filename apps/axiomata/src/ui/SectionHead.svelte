<!--
  The head of a section inside a panel or a side bar (`docs/plans/ui-ux-round.md` UX2): a label in the label role
  (small, in sentence case — docs/plans/glass.md G9), an optional status mark and count, the section's own quiet
  actions at the end. Not a button and not a pill — a heading, so it cannot be mistaken for something to press.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import StatusMark from "./StatusMark.svelte";
  import type { StatusTone } from "./status";

  interface Props {
    label: string;
    /** A count after the label ("Open 4"). */
    count?: number | string;
    /** A status mark before the label, for a column or a group that has a state. */
    tone?: StatusTone;
    /** Actions at the end of the line (refresh, add, …), as `IconButton`s. */
    actions?: Snippet;
    /** A hairline under the head, for a head that ends a block above it. */
    rule?: boolean;
    level?: 2 | 3 | 4;
  }

  let { label, count, tone, actions, rule = false, level = 3 }: Props = $props();
</script>

<div class="section-head" class:rule>
  {#if tone}<StatusMark {tone} />{/if}
  <svelte:element this={`h${level}`} class="label">{label}</svelte:element>
  {#if count !== undefined}<span class="count">{count}</span>{/if}
  <span class="spacer"></span>
  {#if actions}{@render actions()}{/if}
</div>

<style>
  .section-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-height: var(--ax-hit-min);
    padding: 0 var(--ax-space-2) 0 var(--ax-space-4);
  }

  .rule {
    border-bottom: 1px solid var(--ax-border);
  }

  .label {
    margin: 0;
    font: var(--ax-type-label);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }

  .count {
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }
</style>
