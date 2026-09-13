<!--
  The chat transcript: slides up from bottom-centre above the assistant bar.
  Assistant / instruction turns render as Markdown (core/markdown); user
  turns as text. Auto-scrolls to the newest turn. Chrome is the shared
  `Window.svelte` (`modal={false}` — no scrim, this panel never blocks
  interaction with the rest of the app); `.chat-wrap` below owns the fixed
  bottom-centre positioning and the slide-in transition, exactly the split
  `Window.svelte`'s own doc comment describes.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import type { TransitionConfig } from "svelte/transition";

  const SLIDE_MS = 560;

  /** Same technique as `StagingLayer.svelte`'s `slide()` — a plain `fly`
   *  can't be used because `.chat-wrap` is horizontally centred with
   *  `transform: translateX(-50%)`, and Svelte's `fly` *replaces* the
   *  element transform for the duration, which would knock the panel
   *  off-centre mid-animation (this panel used to do exactly that, via a
   *  bare `fly`, before it shared `Window.svelte`'s chrome). */
  function slide(_node: Element): TransitionConfig {
    return {
      duration: SLIDE_MS,
      easing: cubicOut,
      css: (t, u) => `opacity: ${t}; transform: translate(-50%, ${u * 420}px)`,
    };
  }

  import { busy, newSession, panelOpen, sessionId, turns } from "../core/chat";
  import { renderMarkdown } from "../core/markdown";
  import Window from "./Window.svelte";

  let list = $state<HTMLDivElement | null>(null);

  $effect(() => {
    void $turns.length;
    void $busy;
    if (list) list.scrollTop = list.scrollHeight;
  });
</script>

{#if $panelOpen}
  <div class="chat-wrap" transition:slide>
    <Window title="Agent" modal={false} onClose={() => panelOpen.set(false)} style="width: 100%; height: 100%;">
      {#snippet headerExtra()}
        <span class="session">{$sessionId ? `session ${$sessionId.slice(0, 8)}…` : "new session"}</span>
        <button type="button" class="new-session" onclick={newSession} disabled={$busy || $turns.length === 0}>
          New session
        </button>
      {/snippet}
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
    </Window>
  </div>
{/if}

<style>
  .chat-wrap {
    position: fixed;
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    width: min(var(--ax-chat-width), calc(100vw - 2 * var(--ax-space-5)));
    /* Always a little taller than half the viewport — no fixed px cap, so a
       short-but-wide 21:9 display still gets a usable-height panel. */
    height: 56vh;
    z-index: var(--ax-z-assistant);
  }

  .session {
    flex: 1 1 auto;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .new-session {
    font-size: var(--ax-font-size-sm);
    padding: 1px var(--ax-space-2);
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
  .md :global(hr) {
    margin: var(--ax-space-2) 0;
    border: none;
    border-top: 1px solid var(--ax-border);
  }
  .md :global(li::marker) {
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
