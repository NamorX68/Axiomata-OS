<!--
  What a session was given to reach the other sessions and the board (A2A CP-A5), shown read-only in its Inbox tab:
  whether the MCP server is wired in, from which program, and which tools this session's role gets. Nothing here can
  be edited — it is what Axiomata wrote at the session's start, and a restart writes it again. The session's secret is
  never part of it.
-->
<script lang="ts">
  import type { AgentEntry } from "../core/backend";
  import { describeEntry } from "./agentEntry";

  let { entry }: { entry: AgentEntry | null } = $props();

  const view = $derived(describeEntry(entry));
</script>

<section class="team" aria-label="Team tools">
  <header>
    <h3>Team tools</h3>
    <span class="state {view.tone}">{view.label}</span>
  </header>
  {#if view.detail}
    <p class="note">{view.detail}</p>
  {/if}
  {#if entry && entry.tools.length > 0}
    <ul class="tools">
      {#each entry.tools as tool (tool)}
        <li>{tool}</li>
      {/each}
    </ul>
  {/if}
  {#if entry?.command}
    <p class="command" title="The program the harness starts as the {entry.server} MCP server">{entry.command}</p>
  {/if}
  <p class="note later">Messages and the board's cards arrive here with the Flow view.</p>
</section>

<style>
  .team {
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-left: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: var(--ax-space-2);
  }

  h3 {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text);
  }

  .state {
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }

  .state.ok {
    color: var(--ax-success);
  }

  .state.warn {
    color: var(--ax-warning);
  }

  .note {
    margin: 0 0 var(--ax-space-2);
    color: var(--ax-text-muted);
  }

  .later {
    margin: var(--ax-space-3) 0 0;
    font-size: var(--ax-font-size-xs);
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
  }

  .tools li {
    padding: 0 var(--ax-space-2);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-pill);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text);
  }

  .command {
    margin: 0;
    overflow-wrap: anywhere;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
</style>
