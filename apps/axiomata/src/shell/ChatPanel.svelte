<!--
  The chat transcript: slides up from bottom-centre above the assistant bar.
  Assistant / instruction turns render as Markdown (core/markdown); user
  turns as text. Auto-scrolls to the newest turn. Chrome is the shared
  `Window.svelte` (`modal={false}` — no scrim, this panel never blocks
  interaction with the rest of the app); `.chat-wrap` below owns the fixed
  bottom-centre positioning and the slide-in transition, exactly the split
  `Window.svelte`'s own doc comment describes.

  Glut + Hex (UX7c): the agent's turns carry its hexagon ("AX") like a sender, lit while it thinks — three breathing
  hexes in place of grey dots; the owner's own turns are neutral, since the accent means "alive", not "mine".
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

  import { busy, newSession, panelOpen, sessionId, toolCalls, turns, type ToolCall } from "../core/chat";
  import HexAvatar from "../ui/HexAvatar.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import { renderMarkdown } from "../core/markdown";
  import Window from "./Window.svelte";

  let list = $state<HTMLDivElement | null>(null);

  $effect(() => {
    void $turns.length;
    void $busy;
    if (list) list.scrollTop = list.scrollHeight;
  });
</script>

<!-- The app actions the agent called for a reply (UX3): small hex chips instead of a wall of text. -->
{#snippet chips(list: ToolCall[])}
  <ul class="tools" aria-label="Actions the agent used">
    {#each list as call, i (i)}
      <li class="tool"><StatusMark tone={call.ok ? "ok" : "bad"} />{call.label}</li>
    {/each}
  </ul>
{/snippet}

{#if $panelOpen}
  <div class="chat-wrap" transition:slide>
    <Window
      title="Agent"
      subtitle={$sessionId ? `session ${$sessionId.slice(0, 8)}…` : "new session"}
      modal={false}
      onClose={() => panelOpen.set(false)}
      style="width: 100%; height: 100%;"
    >
      {#snippet headerExtra()}
        <button type="button" class="ax-btn new-session" onclick={newSession} disabled={$busy || $turns.length === 0}>
          New session
        </button>
      {/snippet}
      <div class="turns" bind:this={list}>
        {#each $turns as t (t.id)}
          {#if t.role === "user"}
            <article class="turn user"><p>{t.text}</p></article>
          {:else}
            <div class="from-agent">
              <HexAvatar text="AX" ring={t.role === "error" ? "var(--ax-danger)" : undefined} size="sm" />
              <article class="turn {t.role}">
                {#if t.tools}{@render chips(t.tools)}{/if}
                <div class="md">{@html renderMarkdown(t.text)}</div>
                {#if t.costUsd != null}<span class="cost">${t.costUsd.toFixed(4)}</span>{/if}
              </article>
            </div>
          {/if}
        {/each}
        {#if $busy}
          <div class="from-agent">
            <HexAvatar text="AX" live size="sm" label="Axiomata is thinking" />
            <article class="turn assistant thinking">
              {#if $toolCalls.length > 0}{@render chips($toolCalls)}{/if}
              <span class="hexes" aria-hidden="true"><i class="ax-hex"></i><i class="ax-hex"></i><i class="ax-hex"></i></span>
            </article>
          </div>
        {/if}
      </div>
    </Window>
  </div>
{/if}

<style>
  .chat-wrap {
    position: fixed;
    left: 50%;
    bottom: calc(64px * var(--ax-ui-scale));
    transform: translateX(-50%);
    width: min(var(--ax-chat-width), calc(100vw - 2 * var(--ax-space-5)));
    /* Always a little taller than half the viewport — no fixed px cap, so a
       short-but-wide 21:9 display still gets a usable-height panel. */
    height: 56vh;
    z-index: var(--ax-z-assistant);
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
  /* The owner's own words: a raised neutral plate on the right. */
  .turn.user {
    align-self: flex-end;
    background: var(--ax-elev-3);
    border: 1px solid var(--ax-border-strong);
    box-shadow: var(--ax-shadow-raised);
  }
  /* The agent's turns: its hexagon beside them, like a sender. */
  .from-agent {
    align-self: flex-start;
    display: flex;
    align-items: flex-start;
    gap: var(--ax-space-2);
    max-width: 92%;
  }
  .from-agent :global(.avatar) {
    margin-top: var(--ax-space-1);
  }
  .from-agent .turn {
    max-width: none;
    min-width: 0;
  }
  .turn.assistant,
  .turn.instruction,
  .turn.error {
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
  }
  .turn.instruction {
    border-left: calc(3px * var(--ax-ui-scale)) solid var(--ax-border-strong);
  }
  .turn.error {
    border-left: calc(3px * var(--ax-ui-scale)) solid var(--ax-danger);
  }
  .turn p {
    margin: 0;
    white-space: pre-wrap;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-elev-1);
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }
  .cost {
    display: block;
    margin-top: var(--ax-space-1);
    text-align: right;
    font: var(--ax-type-caption);
    font-variant-numeric: tabular-nums;
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
    padding: 1px calc(4px * var(--ax-ui-scale));
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
    color: var(--ax-text-muted);
  }

  .thinking {
    padding: var(--ax-space-3);
  }
  /* Thinking: three small hexes breathing one after another in the glow (still at Calm and Off). */
  .hexes {
    display: inline-flex;
    gap: calc(4px * var(--ax-ui-scale));
  }
  .hexes i {
    width: calc(8px * var(--ax-ui-scale));
    height: calc(9px * var(--ax-ui-scale));
    background: var(--ax-glow);
    animation: var(--ax-anim-breathe);
  }
  .hexes i:nth-child(2) {
    animation-delay: 0.3s;
  }
  .hexes i:nth-child(3) {
    animation-delay: 0.6s;
  }
</style>
