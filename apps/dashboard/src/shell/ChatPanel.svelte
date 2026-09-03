<!--
  The chat transcript: slides up from bottom-centre above the assistant bar.
  Assistant / instruction turns render as Markdown (core/markdown); user
  turns as text. Auto-scrolls to the newest turn.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fly } from "svelte/transition";

  const SLIDE_MS = 560;

  import { busy, newSession, panelOpen, sessionId, turns } from "../core/chat";
  import { renderMarkdown } from "../core/markdown";

  let list = $state<HTMLDivElement | null>(null);

  // Escape closes the chat — unless a staged panel / the Second Brain
  // already consumed it (they mark the event).
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented && $panelOpen) {
      e.preventDefault();
      panelOpen.set(false);
    }
  }

  $effect(() => {
    void $turns.length;
    void $busy;
    if (list) list.scrollTop = list.scrollHeight;
  });
</script>

<svelte:window onkeydown={onKeydown} />

{#if $panelOpen}
  <section class="chat" transition:fly={{ y: 420, duration: SLIDE_MS, easing: cubicOut }} aria-label="Chat">
    <header>
      <h2>Agent</h2>
      <span class="session">{$sessionId ? `session ${$sessionId.slice(0, 8)}…` : "new session"}</span>
      <button type="button" onclick={newSession} disabled={$busy || $turns.length === 0}>New session</button>
      <button type="button" class="close" aria-label="Close chat" onclick={() => panelOpen.set(false)}>×</button>
    </header>
    <div class="turns" bind:this={list}>
      {#each $turns as t (t.id)}
        <article class="turn {t.role}">
          {#if t.role === "user"}
            <p>{t.text}</p>
          {:else}
            <div class="md">{@html renderMarkdown(t.text)}</div>
            {#if t.costUsd != null}<span class="cost">${t.costUsd.toFixed(4)}</span>{/if}
          {/if}
        </article>
      {/each}
      {#if $busy}
        <article class="turn assistant thinking"><span class="dots"><i></i><i></i><i></i></span></article>
      {/if}
    </div>
  </section>
{/if}

<style>
  .chat {
    position: fixed;
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    width: min(var(--ax-chat-width), calc(100vw - 2 * var(--ax-space-5)));
    height: min(60vh, 560px);
    z-index: var(--ax-z-assistant);
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }
  h2 {
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  .session {
    flex: 1 1 auto;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  header button {
    font-size: var(--ax-font-size-sm);
    padding: 1px var(--ax-space-2);
  }
  .close {
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }

  .turns {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }

  .turn {
    position: relative;
    max-width: 88%;
    padding: var(--ax-space-2) var(--ax-space-3);
    border-radius: var(--ax-radius-md);
    user-select: text;
  }
  .turn.user {
    align-self: flex-end;
    background: var(--ax-accent-muted);
    border: 1px solid color-mix(in srgb, var(--ax-accent) 40%, transparent);
  }
  .turn.assistant,
  .turn.instruction,
  .turn.error {
    align-self: flex-start;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
  }
  .turn.instruction {
    border-left: 3px solid var(--ax-accent);
  }
  .turn.error {
    border-left: 3px solid var(--ax-danger);
  }
  .turn p {
    margin: 0;
    white-space: pre-wrap;
  }
  .cost {
    display: block;
    margin-top: var(--ax-space-1);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }

  .md :global(p),
  .md :global(ul),
  .md :global(ol),
  .md :global(pre) {
    margin: 0 0 var(--ax-space-2);
  }
  .md :global(:last-child) {
    margin-bottom: 0;
  }
  .md :global(code) {
    font-family: var(--ax-font-mono);
    font-size: 0.92em;
    background: var(--ax-surface-3);
    padding: 1px 4px;
    border-radius: var(--ax-radius-sm);
  }
  .md :global(pre) {
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-md);
    overflow: auto;
  }
  .md :global(pre code) {
    background: none;
    padding: 0;
  }
  .md :global(blockquote) {
    margin: 0 0 var(--ax-space-2);
    padding-left: var(--ax-space-3);
    border-left: 3px solid var(--ax-border-strong);
    color: var(--ax-text-muted);
  }
  .md :global(a) {
    color: var(--ax-accent);
  }

  .thinking {
    padding: var(--ax-space-3);
  }
  .dots {
    display: inline-flex;
    gap: 4px;
  }
  .dots i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ax-text-muted);
    animation: blink 1.2s infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes blink {
    0%,
    80%,
    100% {
      opacity: 0.25;
    }
    40% {
      opacity: 1;
    }
  }
</style>
