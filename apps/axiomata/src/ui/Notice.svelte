<!--
  A notice as one line with its action (`docs/plans/ui-ux-round.md` UX2), instead of a framed block of prose: a status
  mark, what is the matter in a few words, the button that deals with it. The reasons behind it fold out on demand
  (`details`); whatever the action opens (a review) goes into `children`, under the line.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import IconButton from "./IconButton.svelte";
  import StatusMark from "./StatusMark.svelte";
  import type { StatusTone } from "./status";

  interface Props {
    /** What is the matter, in one line. */
    title: string;
    tone?: StatusTone;
    /** The reasons, folded away until asked for. */
    details?: Snippet;
    /** The button(s) that deal with it, at the end of the line. */
    action?: Snippet;
    /** What the action opened, under the line (a review, a form). */
    children?: Snippet;
    /** Read out at once by assistive technology: something needs the owner before work can go on. */
    alert?: boolean;
  }

  let { title, tone = "warn", details, action, children, alert = false }: Props = $props();

  let open = $state(false);
</script>

<div class="notice {tone}">
  <!-- Only the line is announced: a review opened under it must not be read out again as an alert. -->
  <div class="line" role={alert ? "alert" : "status"}>
    <StatusMark {tone} />
    <span class="title">{title}</span>
    {#if details}
      <IconButton
        icon={open ? "chevron-down" : "chevron-right"}
        label={open ? "Fewer details" : "More details"}
        size="sm"
        onclick={() => (open = !open)}
      />
    {/if}
    {#if action}<span class="action">{@render action()}</span>{/if}
  </div>
  {#if details && open}
    <div class="details">{@render details()}</div>
  {/if}
  {#if children}<div class="body">{@render children()}</div>{/if}
</div>

<style>
  .notice {
    --tone: var(--ax-text-muted);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-2) var(--ax-space-3);
    border: 1px solid color-mix(in srgb, var(--tone) 28%, transparent);
    border-radius: var(--ax-radius-md);
    background: color-mix(in srgb, var(--tone) 8%, var(--ax-elev-2));
    font: var(--ax-type-body);
  }

  .warn {
    --tone: var(--ax-warning);
  }

  .bad {
    --tone: var(--ax-danger);
  }

  .ok {
    --tone: var(--ax-success);
  }

  .live {
    --tone: var(--ax-glow);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-height: var(--ax-hit-min);
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  .action {
    display: inline-flex;
    gap: var(--ax-space-1);
    flex: none;
  }

  .details {
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
  }

  .details :global(p) {
    margin: 0 0 var(--ax-space-1);
  }
</style>
