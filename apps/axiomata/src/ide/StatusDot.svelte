<!--
  The coloured dot an agent's status shows as — in its pane's status line, on
  its dock tab and in the agent picker (M7.2 CP6). One component, so the three
  places cannot drift into three colour schemes.

  `waiting` pulses: it is the one state that needs a human, and the whole point
  of showing it on a tab is that it is noticed in a pane nobody is looking at.
-->
<script lang="ts">
  import type { StatusView } from "./agentStatus";

  let { view }: { view: StatusView } = $props();
</script>

<span class="dot {view.tone}" role="img" aria-label={view.label} title={view.title}></span>

<style>
  .dot {
    display: inline-block;
    flex: 0 0 auto;
    width: var(--ax-space-2);
    height: var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-text-muted);
  }

  .idle {
    background: var(--ax-success);
  }

  .working {
    background: var(--ax-accent);
  }

  .waiting {
    background: var(--ax-warning);
    animation: pulse var(--ax-dur-slow) var(--ax-ease) infinite alternate;
  }

  .ended {
    background: var(--ax-border-strong);
  }

  /* No channel: an outline, so "we cannot know" never looks like a state. */
  .none {
    background: none;
    border: 1px solid var(--ax-border-strong);
  }

  @keyframes pulse {
    to {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .waiting {
      animation: none;
    }
  }
</style>
