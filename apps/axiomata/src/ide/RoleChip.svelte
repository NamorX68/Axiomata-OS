<!--
  A role's name as a small chip, tinted by its kind of work (`docs/plans/studio-roster-ui.md`, R-4): the one place
  that says what kind of agent something is, on an agent's card, in the role list and on a "Start" chip.
-->
<script lang="ts">
  import { roleTone } from "./rosterLook";

  let { label, kind }: { label: string; kind?: string | null } = $props();

  const tone = $derived(roleTone(kind ?? label));
</script>

<span class="chip {tone}">{label}</span>

<style>
  .chip {
    display: inline-block;
    flex: 0 0 auto;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--chip) 18%, transparent);
    /* Mixed with the text colour: a theme-independent hue alone would not read on a light ground. */
    color: color-mix(in srgb, var(--chip) 65%, var(--ax-text));
    font-size: var(--ax-font-size-xs);
    line-height: 1.6;
  }

  .plan {
    --chip: var(--ax-accent);
  }

  .review {
    --chip: var(--ax-success);
  }

  .implement {
    --chip: var(--ax-harness-opencode);
  }

  .other {
    --chip: var(--ax-text-muted);
  }
</style>
