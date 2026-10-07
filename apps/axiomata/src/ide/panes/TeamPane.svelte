<!--
  The Flow's team pane (A2A CP-A9): what the studio's sessions are doing right now. A tile per session — a worker, a
  reviewer, a planner — grouped by the plan its card belongs to: who it is, the card it works on, and a live line with
  its newest step ("Edit …/src/a.rs"). A click on a tile opens the trail of its last steps and what it has used against
  its limits; "Terminal" puts its pane in front, as the Canvas shows it.

  The steps come from the harness's own record (`session_activity`: the Claude Code transcript, the Opencode messages),
  read every few seconds while the pane is shown. Nothing here starts, stops or changes a session.
-->
<script lang="ts">
    import {
    invokeBackend as invoke,
    type IdeProject,
    type SessionActivity,
  } from "../../core/backend";
  import { boardStore, type BoardData } from "../../core/boardStore";
  import { emit } from "../../core/bus";
  import { relativeTime } from "../../core/format";
  import { STATE_LABEL } from "../../core/kanban";
  import CardUsage from "../../modules/CardUsage.svelte";
  import Icon from "../../ui/Icon.svelte";
  import { agentStatus, describeStatus } from "../agentStatus";
  import { session } from "../projectSession";
  import SessionMail from "../SessionMail.svelte";
  import StatusDot from "../StatusDot.svelte";
  import { flowSelection, resolvePlan } from "../flowSelection";
  import { dutyLabel, groupsOf, nowAt, nowLine, stepLabel } from "../team";

  let { project, visible }: { project: IdeProject; tabId: string; visible: boolean } = $props();

  const statuses = agentStatus.statuses;
  /** How often the sessions' records are read while the pane is shown: a step is seconds long, a poll costs a file tail. */
  const POLL_MS = 3000;

  // The board and plan are the Flow's, chosen in the bar above it (`ide/flowSelection.ts`): the tiles are those of the
  // plan shown, and of sessions that belong to no plan.
  let data = $state<BoardData | null>(null);
  $effect(() => {
    const id = $flowSelection.boardId;
    if (id === null) {
      data = null;
      return;
    }
    return boardStore(id).subscribe((value) => (data = value));
  });

  const shown = $derived(resolvePlan(data?.plans ?? [], $flowSelection.planId));
  const groups = $derived(
    groupsOf($session.agents, data?.cards ?? [], data?.plans ?? []).filter(
      (group) => group.planId === null || shown === null || group.planId === shown.id,
    ),
  );
  const idKey = $derived(groups.flatMap((group) => group.tiles.map((tile) => tile.agent.id)).join(","));

  let activity = $state<Record<number, SessionActivity>>({});
  $effect(() => {
    const ids = idKey === "" ? [] : idKey.split(",").map(Number);
    if (!visible || ids.length === 0) return;
    let stale = false;
    // One look at a time: a slow service must not pile calls up behind each other.
    let looking = false;
    const read = (): void => {
      if (looking) return;
      looking = true;
      invoke<SessionActivity[]>("session_activity", { agentIds: ids })
        .then((list) => {
          if (!stale) activity = Object.fromEntries(list.map((entry) => [entry.agent_id, entry]));
        })
        .catch(() => {
          // A failed look leaves what the tiles show.
        })
        .finally(() => {
          looking = false;
        });
    };
    read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  });

  // How many messages wait unread in each session's inbox: a badge on the tile, so mail is seen without opening it.
  let unread = $state<Record<number, number>>({});
  $effect(() => {
    const ids = idKey === "" ? [] : idKey.split(",").map(Number);
    if (!visible || ids.length === 0) return;
    let stale = false;
    const read = (): void => {
      invoke<Record<number, number>>("ide_mailbox_unread", { ids })
        .then((counts) => {
          if (!stale) unread = counts;
        })
        .catch(() => {
          // A badge is a courtesy.
        });
    };
    read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  });

  let open = $state<Record<number, boolean>>({});
  function toggle(agentId: number): void {
    open = { ...open, [agentId]: !open[agentId] };
  }

  function showTerminal(agentId: number): void {
    emit("shell:agent", { projectId: project.id, agentId });
  }

  const now = $derived($statuses.checkedAt || Date.now());
</script>

<div class="team">
  {#if groups.length === 0}
    <p class="empty">
      Noch läuft hier nichts. Sobald ein Plan freigegeben ist und seine Karten starten (oder ein Planer arbeitet), erscheint
      jede Sitzung hier als Kachel — mit dem, was sie gerade tut.
    </p>
  {/if}
  {#each groups as group (group.planId ?? "none")}
    <section class="group" aria-label={group.title}>
      <h3>{group.title}</h3>
      <div class="tiles">
        {#each group.tiles as tile (tile.agent.id)}
          {@const status = describeStatus($statuses.byAgent.get(tile.agent.id), tile.agent, now)}
          {@const live = activity[tile.agent.id]}
          <article class="tile" class:needs={tile.card?.state === "input_required" || status.tone === "waiting"}>
            <header>
              <StatusDot view={status} />
              <span class="name">{tile.agent.name}</span>
              <span class="chip">{tile.agent.agent_role}</span>
              {#if tile.agent.engine_id}<span class="chip muted">{tile.agent.engine_id}</span>{/if}
              {#if (unread[tile.agent.id] ?? 0) > 0}
                <span class="chip mail" title="Ungelesene Nachrichten in der Inbox der Sitzung">✉ {unread[tile.agent.id]}</span>
              {/if}
              <span class="spacer"></span>
              <button
                class="ax-btn terminal"
                class:primary={status.tone === "waiting"}
                type="button"
                title="Das Terminal dieser Sitzung nach vorn holen; es startet keine neue"
                onclick={() => showTerminal(tile.agent.id)}
              >
                <Icon name="terminal" size="sm" />
                Terminal
              </button>
            </header>

            {#if status.tone === "waiting"}
              <p class="waiting" role="status">Wartet auf dich: eine Rückfrage oder eine Freigabe im Terminal.</p>
            {/if}

            {#if tile.card}
              <p class="card-line">
                <span class="muted">{dutyLabel(tile.duty)}</span>
                <strong>#{tile.card.id}</strong>
                <span class="title">{tile.card.title}</span>
                <span class="state">{STATE_LABEL[tile.card.state]}</span>
              </p>
              {#if tile.card.input_required}
                <p class="question" role="status">Fragt: {tile.card.input_required}</p>
              {/if}
            {:else if tile.duty === "planner"}
              <p class="card-line"><span class="muted">{dutyLabel("planner")}</span> den Plan</p>
            {/if}

            <button class="now" type="button" aria-expanded={open[tile.agent.id] ?? false} onclick={() => toggle(tile.agent.id)}>
              <span class="now-label">{status.label}</span>
              <span class="now-text">{nowLine(live) || "…"}</span>
              {#if nowAt(live)}
                <span class="muted">{relativeTime(nowAt(live), now)}</span>
              {/if}
            </button>

            {#if open[tile.agent.id]}
              {#if live && live.steps.length > 1}
                <ol class="trail" aria-label="Letzte Schritte">
                  {#each live.steps.slice(0, -1).reverse() as step, index (index)}
                    <li class:said={step.kind === "say"}>{stepLabel(step)}</li>
                  {/each}
                </ol>
              {/if}
              {#if tile.card}<CardUsage card={tile.card} />{/if}
              <SessionMail agentId={tile.agent.id} {visible} />
            {/if}
          </article>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .team {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-4);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }
  .empty {
    margin: 0;
    color: var(--ax-text-muted);
  }
  h3 {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    font-weight: 600;
  }
  .tiles {
    display: grid;
    /* One tile fills the row until a second fits beside it (auto-fit), never more than two side by side — a third
     * would leave each too narrow to read; in a narrow pane the tiles wrap under each other. */
    grid-template-columns: repeat(
      auto-fit,
      minmax(min(100%, max(calc(420px * var(--ax-ui-scale)), calc(50% - var(--ax-space-3) / 2))), 1fr)
    );
    gap: var(--ax-space-3);
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    min-width: 0;
  }
  .tile.needs {
    border-color: var(--ax-warning);
    box-shadow: 0 0 0 1px var(--ax-warning);
  }
  .waiting {
    margin: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    background: color-mix(in srgb, var(--ax-warning) 18%, var(--ax-surface-2));
    color: var(--ax-warning);
    font-weight: 600;
  }
  header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
  }
  .ax-btn.terminal {
    flex: none;
    white-space: nowrap;
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .chip {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
  }
  .muted {
    color: var(--ax-text-muted);
  }
  .chip.mail {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }
  .card-line,
  .question {
    margin: 0;
    display: flex;
    gap: var(--ax-space-2);
    align-items: baseline;
    min-width: 0;
  }
  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
  }
  .question {
    color: var(--ax-warning);
    word-break: break-word;
  }
  .now {
    display: flex;
    gap: var(--ax-space-2);
    align-items: baseline;
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .now:hover {
    border-color: var(--ax-accent);
  }
  .now-label {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
  }
  /* The newest step is what the tile is for: it wraps (three lines, the trail below has the rest) instead of being
   * cut off. */
  .now-text {
    flex: 1;
    min-width: 0;
    font-family: var(--ax-font-mono);
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow-wrap: anywhere;
  }
  .trail {
    margin: 0;
    padding: 0 0 0 var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }
  .trail li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trail li.said {
    font-family: inherit;
    font-style: italic;
  }
</style>
