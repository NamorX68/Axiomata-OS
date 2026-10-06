<!--
  The Flow's graph of a plan (A2A CP-A10, A6): every card of the plan as a node, a line for each "needs first", the
  state as colour, columns by how many cards come before. Laid out by `ide/flowGraph.ts`, drawn here as SVG; no
  library. A click on a node opens what is known of the card — its history, its sessions — and, for a card the studio
  gave up integrating, the two ways out. Nothing here edits the graph: dependencies are made by the planner and the
  board, not drawn with a mouse.
-->
<script lang="ts">
  import {
    invokeBackend as invoke,
    type Board,
    type BoardCard,
    type BoardPlan,
    type CardEvent,
    type CardIntegrationResult,
    type IdeProject,
  } from "../../core/backend";
  import { boardStore, refreshBoard, type BoardData } from "../../core/boardStore";
  import { emit } from "../../core/bus";
  import { messageOf } from "../../core/errors";
  import { relativeTime } from "../../core/format";
  import { STATE_LABEL } from "../../core/kanban";
  import { toast } from "../../core/toast";
  import { NODE_H, NODE_W, clip, layoutGraph, toneOf } from "../flowGraph";
  import { allCardsOfPlan, defaultPlanId, newestFirst, planStatusLabel } from "../planning";
  import { refreshAgents, session } from "../projectSession";

  let { project, visible }: { project: IdeProject; tabId: string; visible: boolean } = $props();

  // The cards and plans of every board; the subscriptions end with the effect, also when it ends before the boards came.
  let boardData = $state<Record<number, BoardData>>({});
  $effect(() => {
    let ended = false;
    let stops: (() => void)[] = [];
    invoke<Board[]>("list_boards")
      .then((boards) => {
        if (ended) return;
        stops = boards.map((board) =>
          boardStore(board.id).subscribe((value) => (boardData = { ...boardData, [board.id]: value })),
        );
      })
      .catch(() => {
        // Without the boards there is nothing to draw.
      });
    return () => {
      ended = true;
      for (const stop of stops) stop();
    };
  });

  const allCards = $derived<BoardCard[]>(Object.values(boardData).flatMap((data) => data.cards));
  const plans = $derived<BoardPlan[]>(newestFirst(Object.values(boardData).flatMap((data) => data.plans)));

  let chosen = $state<number | null>(null);
  // The plan shown: the one picked while it exists, else a plan that is running, else the newest.
  const planId = $derived(
    chosen !== null && plans.some((p) => p.id === chosen)
      ? chosen
      : (plans.find((p) => p.status === "approved")?.id ?? defaultPlanId(plans)),
  );
  const plan = $derived(plans.find((p) => p.id === planId) ?? null);
  const cards = $derived(plan ? allCardsOfPlan(allCards, plan.id) : []);
  const graph = $derived(layoutGraph(cards));

  let selectedId = $state<number | null>(null);
  const selected = $derived(cards.find((card) => card.id === selectedId) ?? null);
  const sessions = $derived(selected ? $session.agents.filter((agent) => agent.card_id === selected.id) : []);
  /** Cards with a live session: a small dot on the node. */
  const livingCards = $derived(new Set($session.agents.map((agent) => agent.card_id).filter((id) => id != null)));

  // The board is read again while the pane is shown: the cards move from other processes (agents, the CLI).
  $effect(() => {
    if (!visible) return;
    const ids = Object.keys(boardData).map(Number);
    const timer = setInterval(() => ids.forEach((id) => void refreshBoard(id)), 5000);
    return () => clearInterval(timer);
  });

  // The history of the selected card, read again whenever the card changes.
  let events = $state<CardEvent[]>([]);
  $effect(() => {
    const card = selected;
    void card?.updated_at;
    void card?.state;
    if (!card) {
      events = [];
      return;
    }
    let stale = false;
    invoke<CardEvent[]>("list_card_events", { cardId: card.id, limit: 8 })
      .then((list) => {
        if (!stale) events = list;
      })
      .catch(() => {
        if (!stale) events = [];
      });
    return () => {
      stale = true;
    };
  });

  // The cards the studio gave up integrating, which wait for the owner.
  let leftForOwner = $state<number[]>([]);
  $effect(() => {
    const current = plan;
    if (!visible || !current) {
      leftForOwner = [];
      return;
    }
    let stale = false;
    const read = (): void => {
      invoke<number[]>("plan_cards_left_for_owner", { id: current.id })
        .then((ids) => {
          if (!stale) leftForOwner = ids;
        })
        .catch(() => {
          // The buttons are a courtesy; the CLI does the same.
        });
    };
    read();
    const timer = setInterval(read, 5000);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  });

  let busy = $state(false);
  let error = $state("");
  async function act(step: () => Promise<void>): Promise<void> {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await step();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }

  function closePanesOf(ids: number[]): void {
    if (ids.length > 0) emit("studio:close-agent-panes", { agentIds: ids });
  }

  async function integrateAgain(card: BoardCard): Promise<void> {
    await act(async () => {
      const result = await invoke<CardIntegrationResult>("integrate_card", { cardId: card.id });
      if (result.outcome === "done") {
        closePanesOf(result.agent_ids);
        toast(`Karte #${card.id} ist im Plan integriert.`, "info");
      } else if (result.outcome === "conflict") {
        toast(`Karte #${card.id} passt immer noch nicht in den Plan (${result.files.join(", ")}).`, "warning");
      }
      await Promise.all([...Object.keys(boardData).map((id) => refreshBoard(Number(id))), refreshAgents()]);
    });
  }

  async function redo(card: BoardCard): Promise<void> {
    await act(async () => {
      closePanesOf(await invoke<number[]>("redo_card", { cardId: card.id }));
      await Promise.all([...Object.keys(boardData).map((id) => refreshBoard(Number(id))), refreshAgents()]);
      toast(`Karte #${card.id} wird auf dem neuen Stand des Plans noch einmal gemacht.`, "info");
    });
  }

  function showSession(agentId: number): void {
    emit("shell:agent", { projectId: project.id, agentId });
  }

  function onKey(event: KeyboardEvent, id: number): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectedId = selectedId === id ? null : id;
    }
  }
</script>

<div class="graph-pane">
  <header>
    <label>
      <span class="muted">Plan</span>
      <select value={planId ?? ""} onchange={(e) => (chosen = Number((e.currentTarget as HTMLSelectElement).value))}>
        {#each plans as p (p.id)}
          <option value={p.id}>#{p.id} · {p.name} ({planStatusLabel(p.status)})</option>
        {/each}
      </select>
    </label>
    <ul class="legend" aria-label="Farben">
      <li class="idle">bereit / wartet</li>
      <li class="active">in Arbeit</li>
      <li class="attention">Rückfrage</li>
      <li class="review">im Review</li>
      <li class="done">geprüft / im Plan</li>
      <li class="failed">gescheitert</li>
    </ul>
  </header>

  {#if !plan}
    <p class="empty">Es gibt noch keinen Plan. Lege im Reiter „Planung“ einen an; seine Karten erscheinen hier als Graph.</p>
  {:else if cards.length === 0}
    <p class="empty">Der Plan hat noch keine Karten. Der Planer legt sie als Vorschläge an.</p>
  {:else}
    <div class="canvas">
      <svg width={graph.width} height={graph.height} role="img" aria-label="Graph des Plans {plan.name}">
        <defs>
          <marker id="arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto">
            <path d="M 0 0 L 8 4 L 0 8 z" class="arrow-head" />
          </marker>
        </defs>
        {#each graph.edges as edge (`${edge.from}-${edge.to}`)}
          <path
            d={edge.path}
            class="edge"
            class:lit={selectedId === edge.from || selectedId === edge.to}
            marker-end="url(#arrow)"
          />
        {/each}
        {#each graph.nodes as node (node.card.id)}
          <g
            class="node {toneOf(node.card.state)}"
            class:proposed={node.card.state === "proposed"}
            class:selected={selectedId === node.card.id}
            transform="translate({node.x} {node.y})"
            role="button"
            tabindex="0"
            aria-label="Karte {node.card.id}, {STATE_LABEL[node.card.state]}"
            aria-pressed={selectedId === node.card.id}
            onclick={() => (selectedId = selectedId === node.card.id ? null : node.card.id)}
            onkeydown={(e) => onKey(e, node.card.id)}
          >
            <rect width={NODE_W} height={NODE_H} rx="8" />
            <text x="12" y="22" class="title">#{node.card.id} {clip(node.card.title)}</text>
            <text x="12" y="42" class="sub">
              {node.card.agent ?? "—"} · {STATE_LABEL[node.card.state]}{node.card.returned_count > 0
                ? ` · ${node.card.returned_count}× zurück`
                : ""}
            </text>
            {#if livingCards.has(node.card.id)}<circle cx={NODE_W - 14} cy="14" r="4" class="live" />{/if}
          </g>
        {/each}
      </svg>
    </div>

    <section class="detail" aria-label="Karte">
      {#if selected}
        <h3>#{selected.id} {selected.title}</h3>
        <p class="meta">
          <span class="chip {toneOf(selected.state)}">{STATE_LABEL[selected.state]}</span>
          {#if selected.agent}<span class="muted">Rolle {selected.agent}{selected.tier ? ` · ${selected.tier}` : ""}</span>{/if}
          {#if selected.depends_on.length > 0}
            <span class="muted">braucht zuerst: {selected.depends_on.map((id) => `#${id}`).join(", ")}</span>
          {/if}
        </p>
        {#if selected.input_required}<p class="question" role="status">Fragt: {selected.input_required}</p>{/if}
        <div class="actions">
          {#each sessions as agent (agent.id)}
            <button class="ax-btn" type="button" onclick={() => showSession(agent.id)}>
              Sitzung öffnen: {agent.name}
            </button>
          {/each}
          {#if leftForOwner.includes(selected.id)}
            <span class="error" role="status">Passt zweimal nicht in den Plan; das Studio hat aufgegeben.</span>
            <button class="ax-btn" type="button" disabled={busy} onclick={() => void integrateAgain(selected)}>Erneut integrieren</button>
            <button class="ax-btn" type="button" disabled={busy} onclick={() => void redo(selected)}>Neu machen</button>
          {/if}
        </div>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        {#if events.length > 0}
          <ol class="events" aria-label="Verlauf">
            {#each [...events].reverse() as event (event.id)}
              <li>
                <span class="muted">{relativeTime(event.at)}</span>
                <strong>{event.kind}</strong>
                <span class="muted">{event.actor}</span>
                {#if event.text}<span class="text">{event.text}</span>{/if}
              </li>
            {/each}
          </ol>
        {/if}
      {:else}
        <p class="muted">Klicke eine Karte an, um ihren Verlauf und ihre Sitzungen zu sehen.</p>
      {/if}
    </section>
  {/if}
</div>

<style>
  .graph-pane {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-4);
    flex-wrap: wrap;
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
  }
  header label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  select {
    background: var(--ax-surface-2);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    padding: var(--ax-space-1) var(--ax-space-2);
    font: inherit;
  }
  .legend {
    display: flex;
    gap: var(--ax-space-3);
    margin: 0;
    padding: 0;
    list-style: none;
    flex-wrap: wrap;
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .legend li {
    white-space: nowrap;
  }
  .legend li::before {
    content: "";
    display: inline-block;
    width: var(--ax-space-2);
    height: var(--ax-space-2);
    margin-right: var(--ax-space-1);
    border-radius: var(--ax-radius-pill);
    background: var(--tone);
  }
  .empty {
    margin: var(--ax-space-4);
    color: var(--ax-text-muted);
  }
  .canvas {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  svg {
    display: block;
  }

  /* One colour per family of states, from the theme's tokens. */
  .idle {
    --tone: var(--ax-text-muted);
  }
  .active {
    --tone: var(--ax-accent);
  }
  .attention {
    --tone: var(--ax-warning);
  }
  .review {
    --tone: var(--ax-accent-hover);
  }
  .done {
    --tone: var(--ax-success);
  }
  .failed {
    --tone: var(--ax-danger);
  }

  .edge {
    fill: none;
    stroke: var(--ax-border);
    stroke-width: 1.5;
  }
  .edge.lit {
    stroke: var(--ax-accent);
  }
  .arrow-head {
    fill: var(--ax-text-muted);
  }
  .node {
    cursor: pointer;
    outline: none;
  }
  .node rect {
    fill: var(--ax-surface-2);
    stroke: var(--tone);
    stroke-width: 1.5;
  }
  .node.proposed rect {
    stroke-dasharray: 5 4;
  }
  .node.selected rect,
  .node:focus-visible rect {
    fill: var(--ax-accent-muted);
    stroke-width: 2.5;
  }
  .node text {
    fill: var(--ax-text);
    font-size: 12px;
  }
  .node .sub {
    fill: var(--ax-text-muted);
    font-size: 11px;
  }
  .node .live {
    fill: var(--ax-success);
  }
  .detail {
    flex: none;
    max-height: 40%;
    overflow-y: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    border-top: 1px solid var(--ax-border);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }
  .meta {
    margin: 0;
    display: flex;
    gap: var(--ax-space-3);
    flex-wrap: wrap;
    align-items: center;
  }
  .chip {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--tone);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-xs);
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .question,
  .error {
    margin: 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
    flex-wrap: wrap;
    align-items: center;
  }
  .events {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .events li {
    display: flex;
    gap: var(--ax-space-2);
    flex-wrap: wrap;
    align-items: baseline;
  }
  .events .text {
    word-break: break-word;
  }
</style>
