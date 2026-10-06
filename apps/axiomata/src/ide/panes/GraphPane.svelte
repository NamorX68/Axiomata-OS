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
    type BoardCard,
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
  import { NODE_H, NODE_W, clip, endOf, layoutGraph, reviewLabel, reviewToneOf, toneOf } from "../flowGraph";
  import { allCardsOfPlan } from "../planning";
  import { flowSelection, resolvePlan } from "../flowSelection";
  import { agentStatus } from "../agentStatus";
  import { refreshAgents, session } from "../projectSession";

  let { project, visible }: { project: IdeProject; tabId: string; visible: boolean } = $props();

  // The board and plan are the Flow's, chosen in the bar above it (`ide/flowSelection.ts`): this pane has no selector.
  let data = $state<BoardData | null>(null);
  $effect(() => {
    const id = $flowSelection.boardId;
    if (id === null) {
      data = null;
      return;
    }
    return boardStore(id).subscribe((value) => (data = value));
  });

  const plan = $derived(resolvePlan(data?.plans ?? [], $flowSelection.planId));
  const cards = $derived(plan && data ? allCardsOfPlan(data.cards, plan.id) : []);
  const graph = $derived(layoutGraph(cards));
  const goal = $derived(plan ? endOf(cards, plan.status) : null);

  let selectedId = $state<number | null>(null);
  const selected = $derived(cards.find((card) => card.id === selectedId) ?? null);
  const sessions = $derived(selected ? $session.agents.filter((agent) => agent.card_id === selected.id) : []);
  // A session started to review the card is the reviewer; the card's other sessions are the worker.
  const isReviewer = (agent: { card_review?: boolean }): boolean => agent.card_review === true;
  /** Cards with a live worker session, and cards with a live reviewer: a small dot on the node. */
  const livingCards = $derived(
    new Set($session.agents.filter((a) => !isReviewer(a)).map((a) => a.card_id).filter((id) => id != null)),
  );
  const reviewedNow = $derived(
    new Set($session.agents.filter(isReviewer).map((a) => a.card_id).filter((id) => id != null)),
  );

  // A session that waits for the owner (a question, a permission) turns its node into a "Rückfrage": nobody else will
  // answer it.
  const statuses = agentStatus.statuses;
  const waitingOn = (review: boolean): Set<number | null | undefined> =>
    new Set(
      $session.agents
        .filter((a) => isReviewer(a) === review && $statuses.byAgent.get(a.id)?.state === "waiting")
        .map((a) => a.card_id),
    );
  const waitingWorkers = $derived(waitingOn(false));
  const waitingReviewers = $derived(waitingOn(true));

  // The board is read again while the pane is shown: the cards move from other processes (agents, the CLI).
  $effect(() => {
    const id = $flowSelection.boardId;
    if (!visible || id === null) return;
    const timer = setInterval(() => void refreshBoard(id), 5000);
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
      await Promise.all([reloadBoard(), refreshAgents()]);
    });
  }

  async function redo(card: BoardCard): Promise<void> {
    await act(async () => {
      closePanesOf(await invoke<number[]>("redo_card", { cardId: card.id }));
      await Promise.all([reloadBoard(), refreshAgents()]);
      toast(`Karte #${card.id} wird auf dem neuen Stand des Plans noch einmal gemacht.`, "info");
    });
  }

  async function reloadBoard(): Promise<void> {
    const id = $flowSelection.boardId;
    if (id !== null) await refreshBoard(id);
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
            class:lit={selectedId !== null && (selectedId === Math.abs(edge.from) || selectedId === Math.abs(edge.to))}
            marker-end="url(#arrow)"
          />
        {/each}
        {#if graph.start}
          <g class="start" transform="translate({graph.start.x} {graph.start.y})" aria-label="Start des Plans">
            <rect width={graph.start.w} height={graph.start.h} rx={graph.start.h / 2} />
            <text x={graph.start.w / 2} y={graph.start.h / 2 + 4} text-anchor="middle">Start</text>
          </g>
        {/if}
        {#if graph.end && goal}
          <g
            class="goal {goal.tone}"
            transform="translate({graph.end.x} {graph.end.y})"
            aria-label="Ziel des Plans: {goal.label}"
          >
            <rect width={graph.end.w} height={graph.end.h} rx={graph.end.h / 2} />
            <text x={graph.end.w / 2} y="22" text-anchor="middle" class="goal-title">Ziel</text>
            <text x={graph.end.w / 2} y="40" text-anchor="middle" class="goal-sub">{goal.label}</text>
          </g>
        {/if}
        {#each graph.nodes as node (node.key)}
          {@const review = node.kind === "review"}
          <g
            class="node {(review ? waitingReviewers : waitingWorkers).has(node.card.id)
              ? 'attention'
              : review
                ? reviewToneOf(node.card.state)
                : toneOf(node.card.state)}"
            class:review-node={review}
            class:proposed={node.card.state === "proposed"}
            class:selected={selectedId === node.card.id}
            transform="translate({node.x} {node.y})"
            role="button"
            tabindex="0"
            aria-label={review
              ? `Review von Karte ${node.card.id}, ${reviewLabel(node.card)}`
              : `Karte ${node.card.id}, ${STATE_LABEL[node.card.state]}`}
            aria-pressed={selectedId === node.card.id}
            onclick={() => (selectedId = selectedId === node.card.id ? null : node.card.id)}
            onkeydown={(e) => onKey(e, node.card.id)}
          >
            <rect width={NODE_W} height={NODE_H} rx="10" />
            {#if review}
              <text x="14" y="30" class="title">Review #{node.card.id}</text>
              <text x="14" y="56" class="sub">{reviewLabel(node.card)} · {clip(node.card.title, 26)}</text>
              {#if reviewedNow.has(node.card.id)}<circle cx={NODE_W - 16} cy="16" r="5" class="live" />{/if}
            {:else}
              <text x="14" y="30" class="title">#{node.card.id} {clip(node.card.title)}</text>
              <text x="14" y="56" class="sub">
                {node.card.agent ?? "—"} · {STATE_LABEL[node.card.state]}{node.card.returned_count > 0
                  ? ` · ${node.card.returned_count}× zurück`
                  : ""}
              </text>
              {#if livingCards.has(node.card.id)}<circle cx={NODE_W - 16} cy="16" r="5" class="live" />{/if}
            {/if}
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
  /* The graph sits in the middle of the pane, both ways; `margin: auto` (not `align-items: center`) so that one larger than
     the pane scrolls from its top left instead of being cut off at the top. */
  .canvas {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
  }
  svg {
    display: block;
    flex: none;
    margin: auto;
  }
  .start rect {
    fill: var(--ax-accent-muted);
    stroke: var(--ax-accent);
    stroke-width: 1.5;
  }
  .start text {
    fill: var(--ax-text);
    font-size: 12px;
    font-weight: 600;
  }
  .goal rect {
    fill: var(--ax-surface-2);
    stroke: var(--tone);
    stroke-width: 2;
  }
  .goal.done rect {
    fill: color-mix(in srgb, var(--tone) 18%, var(--ax-surface-2));
  }
  .goal-title {
    fill: var(--ax-text);
    font-size: 14px;
    font-weight: 600;
  }
  .goal-sub {
    fill: var(--ax-text-muted);
    font-size: 12px;
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
  /* A review is a stage of its card, not a card: a little lighter, its lines dashed like a proposal's but finer. */
  .node.review-node rect {
    fill: var(--ax-surface-1);
    stroke-dasharray: 2 3;
  }
  .node.selected rect,
  .node:focus-visible rect {
    fill: var(--ax-accent-muted);
    stroke-width: 2.5;
  }
  .node text {
    fill: var(--ax-text);
    font-size: 15px;
    font-weight: 600;
  }
  .node .sub {
    fill: var(--ax-text-muted);
    font-size: 13px;
    font-weight: 400;
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
