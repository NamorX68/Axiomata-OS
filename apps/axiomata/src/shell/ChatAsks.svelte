<!--
  The chat's questions to the owner (`docs/plans/chat-knows-app.md`, CH5): what the chat wants to start — a skill, a
  routine, a card — and Allow / Deny, in the shape of an agent's permission request (`ui/AskAnswer.svelte`). Only the
  owner's click here runs the action (`core/chatAsks.ts`), and only once the question has stood a moment, while it is
  fresh and was shown in full. The question is built by the app from the action's parameters and shown as plain text,
  never markup, with who asks and the whole detail. Floats over the bottom of every view, the Studio included, since
  the chat may ask while the owner is elsewhere.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { ARM_MS, answerChatAsk, askerLabel, chatAsks, dropStale } from "../core/chatAsks";
  import { tShell } from "../i18n/shell";
  import Icon from "../ui/Icon.svelte";

  /** The clock the arming and the staleness are read against; ticks while questions wait. */
  let now = $state(Date.now());

  onMount(() => {
    // Ticks only while a question waits: the arming and the staleness are all it reads the clock for.
    let tick: ReturnType<typeof setInterval> | null = null;
    const off = chatAsks.subscribe((list) => {
      if (list.length > 0 && tick === null) {
        tick = setInterval(() => {
          now = Date.now();
          dropStale(now);
        }, ARM_MS);
      } else if (list.length === 0 && tick !== null) {
        clearInterval(tick);
        tick = null;
      }
    });
    return () => {
      off();
      if (tick !== null) clearInterval(tick);
    };
  });

  /** The question leaves the list at once (`answerChatAsk`); the action itself runs on, without holding the others. */
  function answer(id: number, allow: boolean): void {
    void answerChatAsk(id, allow);
  }
</script>

{#if $chatAsks.length > 0}
  <section class="asks" aria-label={tShell("questionsForYou")}>
    {#each $chatAsks as ask (ask.id)}
      <div class="ask" role="group" aria-label="{askerLabel(ask.origin)}: {ask.what}">
        <span class="mark" aria-hidden="true"><Icon name="circle-question-mark" size="sm" /></span>
        <p class="what">
          <span class="lead">{askerLabel(ask.origin)}:</span>
          <span class="text">{ask.what}</span>
        </p>
        <div class="answers">
          <button type="button" class="answer" onclick={() => answer(ask.id, false)}>
            {tShell("deny")}
          </button>
          <button
            type="button"
            class="answer allow"
            disabled={!ask.complete || now - ask.at < ARM_MS}
            title={ask.complete ? tShell("runNow") : tShell("tooLongTitle")}
            onclick={() => answer(ask.id, true)}
          >
            {tShell("allow")}
          </button>
        </div>
        {#if ask.detail}
          <!-- Everything that decides the action, in full: a routine's prompt, a card's text. -->
          <p class="detail">{ask.detail}</p>
        {/if}
        {#if !ask.complete}
          <p class="note">{tShell("tooLongNote")}</p>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
  /* Above the assistant bar and an open dialog (a question must stay answerable), below the toasts. Solid, not glass:
     it holds text (docs/plans/glass.md). */
  .asks {
    position: fixed;
    left: 50%;
    bottom: calc(76px * var(--ax-ui-scale));
    z-index: calc(var(--ax-z-dialog) + 1);
    transform: translateX(-50%);
    display: grid;
    gap: var(--ax-space-2);
    width: min(calc(640px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-4)));
  }

  .ask {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: start;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-panel);
  }

  /* Something waits for the owner: the accent belongs here (the accent rule). */
  .mark {
    display: inline-flex;
    color: var(--ax-glow);
  }

  .what {
    display: flex;
    gap: var(--ax-space-2);
    min-width: 0;
    margin: 0;
    font: var(--ax-type-body);
  }

  .lead {
    flex: none;
    color: var(--ax-text-muted);
  }

  /* Wrapped, never cut with an ellipsis: what is allowed is what is read. */
  .text {
    min-width: 0;
    color: var(--ax-text);
    overflow-wrap: anywhere;
  }

  .detail,
  .note {
    grid-column: 2 / -1;
    margin: 0;
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  .detail {
    max-height: calc(160px * var(--ax-ui-scale));
    overflow: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--ax-font-mono);
  }

  .answers {
    display: flex;
    gap: var(--ax-space-1);
  }

  .answer {
    padding: calc(2px * var(--ax-ui-scale)) var(--ax-space-2);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: var(--ax-type-caption);
    cursor: pointer;
  }

  .answer:hover:not(:disabled),
  .answer:focus-visible {
    background: var(--ax-selected-bg);
  }

  /* The one primary action. */
  .answer.allow {
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }

  .answer:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
