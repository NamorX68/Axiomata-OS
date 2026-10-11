<!--
  The permanent bottom input line. Enter routes the text (core/commands):
  a registered `/command` runs locally, other `/text` is a one-shot agent
  instruction, plain text is a chat turn. ↑/↓ walk the input history.
-->
<script lang="ts">
  import { busy, panelOpen, pushInstruction, send, turns } from "../core/chat";
  import { route, runCommand } from "../core/commands";
  import { toast } from "../core/toast";
  import StatusMark from "../ui/StatusMark.svelte";

  let input = $state("");
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

  // The top-bar search icon opens the spotlight (`shell:spotlight`, `App.svelte`), not this chat field.
</script>

<div class="dock">
<form class="bar" class:thinking={$busy} onsubmit={submit}>
  <span class="prompt" aria-hidden="true">›</span>
  <input
    bind:value={input}
    type="text"
    {placeholder}
    aria-label="Assistant input"
    autocomplete="off"
    spellcheck="false"
    onkeydown={onKeydown}
  />
  {#if $busy}
    <span class="chip"><StatusMark tone="live" />working…</span>
  {:else if $turns.length > 0}
    <button
      type="button"
      class="toggle"
      title="{$turns.length} messages in this conversation"
      onclick={() => panelOpen.update((v) => !v)}
    >
      <span class="toggle-label">{$panelOpen ? "hide chat" : "chat"}</span>
      <span class="toggle-count" aria-hidden="true">{$turns.length}</span>
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
  /* While the agent works an ember runs round the bar ("Glut + Hex", UX3): a conic gradient in the border box under
     the bar's own fill, turned by `--ax-anim-orbit` (still at Calm and Off — the ember then simply stands). */
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
    border-color: var(--ax-text-muted);
  }
  .bar.thinking {
    border-color: transparent;
    background:
      linear-gradient(var(--ax-surface-1), var(--ax-surface-1)) padding-box,
      conic-gradient(
          from var(--ax-orbit-angle),
          transparent 0deg,
          var(--ax-glow) 60deg,
          transparent 140deg,
          transparent 360deg
        )
        border-box;
    box-shadow:
      var(--ax-shadow-tile),
      var(--ax-glow-shadow);
    animation: var(--ax-anim-orbit);
  }
  .prompt {
    color: var(--ax-text-muted);
    font-weight: 700;
  }
  .thinking .prompt {
    color: var(--ax-glow);
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
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    padding: 1px var(--ax-space-3);
    border-radius: var(--ax-radius-pill);
  }
  .toggle-label {
    color: var(--ax-text);
  }
  /* Turn count as a distinct badge, not a "chat #N" label — the panel has a
     single session, so a bare number reads like the conversation's size. */
  .toggle-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.2em;
    height: 1.2em;
    padding: 0 0.3em;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-selected-bg);
    color: var(--ax-text);
    font-size: 0.85em;
    font-weight: 600;
  }
</style>
