<!--
  One permission request of an agent and the owner's answer to it (`docs/plans/permission-asks.md`, card #88): what is
  asked — the tool and its argument, plain text, written by the agent — and Allow / Deny, plus "In terminal" for Claude
  Code, which hands the question back to the pane's own prompt. Used in the waiting badge's list and on top of the
  agent's pane in the Studio. The answer goes through `answer_permission`, the app's only door for it.
-->
<script lang="ts">
  import { askDecisions, waitingAgents, withoutAsk, type AskDecision, type WaitingAsk } from "../core/agentWaiting";
  import { invokeBackend } from "../core/backend";
  import { toast } from "../core/toast";

  interface Props {
    agentId: number;
    ask: WaitingAsk;
    /** A single line, for the strip over a pane. */
    compact?: boolean;
  }

  let { agentId, ask, compact = false }: Props = $props();

  const LABELS: Record<AskDecision, string> = { allow: "Allow", deny: "Deny", terminal: "In terminal" };
  const TITLES: Record<AskDecision, string> = {
    allow: "Allow this once",
    deny: "Refuse it; the agent is told you declined",
    terminal: "Answer it in the agent's terminal instead",
  };

  let busy = $state(false);

  async function answer(event: MouseEvent, decision: AskDecision): Promise<void> {
    // The request may sit inside something clickable (a list row): the click is the answer's alone.
    event.stopPropagation();
    if (busy) return;
    busy = true;
    try {
      await invokeBackend("answer_permission", { agentId, requestId: ask.id, decision });
      waitingAgents.update((list) => withoutAsk(list, agentId));
    } catch (err) {
      toast(`Could not answer: ${String(err)}`, "warning");
    } finally {
      busy = false;
    }
  }
</script>

<div class="ask" class:compact>
  <p class="what" title={`${ask.tool}: ${ask.detail}`}>
    <span class="tool">{ask.tool}</span>
    <span class="detail">{ask.detail}</span>
  </p>
  <div class="answers">
    {#each askDecisions(ask) as decision (decision)}
      <button
        type="button"
        class="answer {decision}"
        title={TITLES[decision]}
        disabled={busy}
        onclick={(event) => void answer(event, decision)}
      >
        {LABELS[decision]}
      </button>
    {/each}
  </div>
</div>

<style>
  .ask {
    display: grid;
    gap: var(--ax-space-1);
    min-width: 0;
  }

  .ask.compact {
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .what {
    display: flex;
    gap: var(--ax-space-2);
    min-width: 0;
    margin: 0;
    font: var(--ax-type-caption);
  }

  .tool {
    flex: none;
    color: var(--ax-text);
    font-weight: 600;
  }

  .detail {
    overflow: hidden;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .answers {
    display: flex;
    gap: var(--ax-space-1);
    justify-content: flex-end;
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

  /* The one primary action: an agent stands still until the owner lets it go on. */
  .answer.allow {
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }

  .answer:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
