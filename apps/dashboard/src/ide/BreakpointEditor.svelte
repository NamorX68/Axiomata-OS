<!--
  The extras of one breakpoint (#51): a **condition** (stop only when it is true), a **hit count** (stop on the
  Nth time, or `>3`, `% 10`) and a **log message** (print instead of stopping; `{x}` is replaced by the value
  of `x`). Used in a small popover next to the gutter and inline in the Debug view's breakpoint list.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { BpInfo } from "./breakpoints";

  let {
    info,
    label,
    onSave,
    onClose,
  }: {
    info: BpInfo | undefined;
    /** Where the breakpoint is, for the title (`chat.py:11`). */
    label: string;
    /** `null` = a plain breakpoint again. */
    onSave: (info: BpInfo | null) => void;
    onClose: () => void;
  } = $props();

  // The form starts from what the breakpoint has now and edits its own copy.
  // svelte-ignore state_referenced_locally
  let condition = $state(info?.condition ?? "");
  // svelte-ignore state_referenced_locally
  let hit = $state(info?.hit ?? "");
  // svelte-ignore state_referenced_locally
  let log = $state(info?.log ?? "");
  let first = $state<HTMLInputElement | null>(null);

  onMount(() => first?.focus());

  function save(): void {
    onSave(condition.trim() || hit.trim() || log.trim() ? { condition, hit, log } : null);
    onClose();
  }
</script>

<!-- Escape closes the form; the key goes to the form because the focus is in one of its inputs. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="editor"
  onsubmit={(event) => {
    event.preventDefault();
    save();
  }}
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  }}
>
  <p class="title">Breakpoint {label}</p>
  <label>
    Condition
    <input bind:this={first} type="text" bind:value={condition} placeholder="e.g. i == 3" spellcheck="false" autocomplete="off" />
  </label>
  <label>
    Hit count
    <input type="text" bind:value={hit} placeholder="e.g. 5, >3, % 10" spellcheck="false" autocomplete="off" />
  </label>
  <label>
    Log message <small>(does not stop)</small>
    <input type="text" bind:value={log} placeholder="e.g. total = &#123;total&#125;" spellcheck="false" autocomplete="off" />
  </label>
  <div class="actions">
    <button type="submit" class="ax-btn primary">Save</button>
    <button type="button" class="ax-btn" onclick={() => { onSave(null); onClose(); }}>Plain breakpoint</button>
    <button type="button" class="ax-btn" onclick={onClose}>Cancel</button>
  </div>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .title {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  input {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  input:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
  }
</style>
