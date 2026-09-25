<!--
  Vi's part of a status line (`docs/plans/editor.md`, ED3, V8): the mode pill,
  a recording, keys typed so far — and, while it is open, the command line
  (`:`, `/`, `?`) with its cursor, or the last message (`E486: …`) in its place.
  The file app's footer holds it, and so does the bar under a diff.
-->
<script lang="ts">
  import type { ViStatus } from "./viSurface";

  let { status }: { status: ViStatus } = $props();

  const LABELS: Record<ViStatus["mode"], string> = {
    normal: "NORMAL",
    insert: "INSERT",
    replace: "REPLACE",
    visual: "VISUAL",
    visualLine: "V-LINE",
    visualBlock: "V-BLOCK",
  };
</script>

<span class="vi-status">
  <span class="vi-pill {status.mode}">{LABELS[status.mode]}</span>
  {#if status.recording}<span class="vi-rec">● REC {status.recording}</span>{/if}
  {#if status.cmdline}
    {@const line = status.cmdline}
    <span class="vi-cmdline" aria-live="polite"
      >{line.kind}{line.text.slice(0, line.cursor)}<span class="vi-caret" class:register={line.register}
        >{line.register ? '"' : ""}</span
      >{line.text.slice(line.cursor)}</span
    >
  {:else if status.message}
    <span class="vi-message" class:error={status.message.error} role="status">{status.message.text}</span>
  {:else if status.pending}
    <span class="vi-pending">{status.pending}</span>
  {/if}
</span>

<style>
  .vi-status {
    display: flex;
    gap: var(--ax-space-3);
    min-width: 0;
    align-items: baseline;
  }

  .vi-pill {
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-bg);
    font-family: var(--ax-font-sans);
    letter-spacing: var(--ax-tracking-wide);
  }

  .vi-pill.normal {
    background: var(--ax-vi-normal);
  }

  .vi-pill.insert {
    background: var(--ax-vi-insert);
  }

  .vi-pill.visual,
  .vi-pill.visualLine,
  .vi-pill.visualBlock {
    background: var(--ax-vi-visual);
  }

  .vi-pill.replace {
    background: var(--ax-vi-replace);
  }

  .vi-rec {
    color: var(--ax-danger);
  }

  .vi-pending,
  .vi-cmdline {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }

  .vi-cmdline {
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .vi-caret {
    border-left: 2px solid var(--ax-vi-normal);
    margin-right: -2px;
  }

  .vi-caret.register {
    border-left: none;
    margin-right: 0;
    color: var(--ax-vi-normal);
  }

  .vi-message {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .vi-message.error {
    color: var(--ax-danger);
  }
</style>
