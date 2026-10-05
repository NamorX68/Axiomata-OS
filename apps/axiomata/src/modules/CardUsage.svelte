<!--
  "Verbrauch": what the sessions started for a card have used, against what their role allows (A2A CP-A6c, A9). Read
  fresh when the card is opened and whenever it changes — a stop shows up as a line in the history, and this says by how
  much. A session stopped at a limit keeps the card; to go on, the role's limit is raised (Studio, Engines & roles).
-->
<script lang="ts">
  import { invokeBackend as invoke, type BoardCard, type SessionUsage } from "../core/backend";
  import { unmeasuredNote, usageLines } from "../ide/cardUsage";

  let { card }: { card: Pick<BoardCard, "id" | "updated_at" | "state"> } = $props();

  let sessions = $state<SessionUsage[]>([]);
  let shownFor: number | null = null;

  $effect(() => {
    const id = card.id;
    // Another card's figures must not stay on screen while this card's are being read; a refresh of the same card
    // keeps what it shows until the new answer is in.
    if (shownFor !== id) {
      sessions = [];
      shownFor = id;
    }
    void card.updated_at;
    void card.state;
    // A slower answer for the card that was open before must not overwrite the one that is open now.
    let stale = false;
    invoke<SessionUsage[]>("card_usage", { cardId: id })
      .then((list) => {
        if (!stale) sessions = list;
      })
      .catch(() => {
        if (!stale) sessions = [];
      });
    return () => {
      stale = true;
    };
  });
</script>

{#if sessions.length > 0}
  <section class="usage" aria-label="Verbrauch">
    <h4 class="usage-head">Verbrauch</h4>
    {#each sessions as session (session.agent_id)}
      <div class="session">
        <div class="who">
          <strong>{session.name}</strong>
          <span class="muted">{session.review ? "Reviewer" : "Arbeiter"} · {session.role}</span>
        </div>
        {#if !session.measured}
          <p class="warn">Verbrauch unbekannt. {unmeasuredNote(session)}</p>
        {/if}
        {#each usageLines(session) as row (row.label)}
          <div class="row" class:reached={row.reached}>
            <span class="label">{row.label}</span>
            <span class="bar" aria-hidden="true"><span class="fill" style:width="{Math.round(row.share * 100)}%"></span></span>
            <span class="figures">{row.used} / {row.allowed}</span>
          </div>
        {/each}
        {#if session.stopped}
          <p class="stopped">Gestoppt: {session.stopped}</p>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
  .usage {
    margin-top: var(--ax-space-3);
  }
  .usage-head {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }
  .session {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .muted {
    margin-left: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .row {
    display: grid;
    grid-template-columns: calc(64px * var(--ax-ui-scale)) 1fr auto;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .label {
    color: var(--ax-text-muted);
  }
  .bar {
    height: calc(6px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--ax-accent);
  }
  .reached .fill {
    background: var(--ax-warning);
  }
  .figures {
    font-variant-numeric: tabular-nums;
  }
  .warn,
  .stopped {
    margin: 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
</style>
