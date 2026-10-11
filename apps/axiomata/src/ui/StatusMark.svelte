<!--
  A status as a small hexagon (`docs/plans/ui-ux-round.md` U1/U5): the system's shape for "how is this thing doing" —
  an agent, a column, a notice. One component, so every status in the app speaks the same colours.

  `live` glows and breathes: something works there. `warn` breathes without the glow: it waits for the owner (a
  question, a review). Everything else holds still. `unknown` is only a ghost of the mark, so "we cannot know" never
  looks like a state.
-->
<script lang="ts">
  import type { StatusTone } from "./status";

  interface Props {
    tone?: StatusTone;
    /** Read out by assistive technology; without it the mark is decorative (the text beside it says it). */
    label?: string;
    title?: string;
    size?: "sm" | "md";
  }

  let { tone = "neutral", label, title, size = "sm" }: Props = $props();
</script>

<span
  class="mark {tone} {size}"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  {title}
></span>

<style>
  .mark {
    display: inline-block;
    flex: none;
    clip-path: var(--ax-hex-clip);
    background: var(--ax-text-muted);
  }

  .sm {
    width: calc(9px * var(--ax-ui-scale));
    height: calc(10px * var(--ax-ui-scale));
  }

  .md {
    width: calc(12px * var(--ax-ui-scale));
    height: calc(13px * var(--ax-ui-scale));
  }

  .live {
    background: var(--ax-glow);
    filter: var(--ax-glow-drop);
    animation: var(--ax-anim-breathe);
  }

  .ok {
    background: var(--ax-success);
  }

  .warn {
    background: var(--ax-warning);
    animation: var(--ax-anim-breathe);
  }

  .bad {
    background: var(--ax-danger);
  }

  .off {
    background: var(--ax-border-strong);
  }

  /* A ghost of the mark: a border would not follow the clip, and a ghost reads as "no state" just as well. */
  .unknown {
    background: color-mix(in srgb, var(--ax-border-strong) 45%, transparent);
  }
</style>
