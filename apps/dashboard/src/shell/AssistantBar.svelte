<!--
  The permanent bottom input line. Enter routes the text (core/commands):
  a registered `/command` runs locally, other `/text` is a one-shot agent
  instruction, plain text is a chat turn. ↑/↓ walk the input history.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { on } from "../core/bus";
  import { busy, panelOpen, pushInstruction, send, turns } from "../core/chat";
  import { route, runCommand } from "../core/commands";
  import { toast } from "../core/toast";

  let input = $state("");
  let field = $state<HTMLInputElement | null>(null);
  let history = $state<string[]>([]);
  let cursor = $state(-1);

  const placeholder = $derived(
    $busy ? "Agent is working…" : "Ask the agent, or /help for commands",
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const routed = route(input);
    if (!routed) return;
    history = [input, ...history.filter((h) => h !== input)].slice(0, 50);
    cursor = -1;
    input = "";

    if (routed.kind === "command") {
      const result = await runCommand(routed.name, routed.args);
      toast(result.message, result.ok ? "info" : "warning");
      if (result.detail) pushInstruction(result.detail);
      return;
    }
    await send(routed.message, routed.kind);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowUp" && history.length > 0) {
      cursor = Math.min(cursor + 1, history.length - 1);
      input = history[cursor];
      e.preventDefault();
    } else if (e.key === "ArrowDown") {
      cursor = Math.max(cursor - 1, -1);
      input = cursor === -1 ? "" : history[cursor];
      e.preventDefault();
    }
  }

  onMount(() => on("shell:search", () => field?.focus()));
</script>

<div class="dock">
<form class="bar" onsubmit={submit}>
  <span class="prompt" aria-hidden="true">›</span>
  <input
    bind:this={field}
    bind:value={input}
    type="text"
    {placeholder}
    aria-label="Assistant input"
    autocomplete="off"
    spellcheck="false"
    onkeydown={onKeydown}
  />
  {#if $busy}
    <span class="chip"><span class="spinner"></span>working…</span>
  {:else if $turns.length > 0}
    <button type="button" class="toggle" onclick={() => panelOpen.update((v) => !v)}>
      {$panelOpen ? "hide chat" : `chat (${$turns.length})`}
    </button>
  {/if}
</form>
</div>

<style>
  /* Centred pill — never the full width of an ultra-wide monitor. */
  .dock {
    flex: 0 0 auto;
    display: flex;
    justify-content: center;
    padding: var(--ax-space-2) var(--ax-space-4) var(--ax-space-3);
  }
  .bar {
    width: min(var(--ax-assistant-width), 100%);
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3) var(--ax-space-1) var(--ax-space-4);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-tile);
    z-index: var(--ax-z-assistant);
  }
  .bar:focus-within {
    border-color: var(--ax-accent);
  }
  .prompt {
    color: var(--ax-accent);
    font-weight: 700;
  }
  input {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border-color: transparent;
    padding: var(--ax-space-1) var(--ax-space-2);
  }
  input:focus-visible {
    outline: none;
    border-color: transparent;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .spinner {
    width: 10px;
    height: 10px;
    border: 2px solid var(--ax-accent-muted);
    border-top-color: var(--ax-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .toggle {
    font-size: var(--ax-font-size-sm);
    padding: 1px var(--ax-space-3);
    border-radius: var(--ax-radius-pill);
  }
</style>
