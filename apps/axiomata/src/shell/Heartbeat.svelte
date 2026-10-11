<!--
  The dock's heartbeat and tank (`docs/plans/ui-ux-round.md` UX3, G14/G15): today as a strip from midnight to
  midnight with a spark for every skill or routine run and every agent session on a card (`core/heartbeat.ts`), and
  beside it today's metered spend as hex segments against the daily limit — red from 80 % on.

  Read every half minute and whenever the window comes back into focus; nothing here is pushed. A click on a card's
  spark opens the card in the Kanban, one on a run's spark shows the run (there is no run view of its own yet).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import {
    invokeBackend as invoke,
    type Board,
    type BoardCard,
    type RunRecord,
    type RunSummary,
    type SpendSummary,
  } from "../core/backend";
  import {
    beatsFromCards,
    beatsFromRuns,
    dayShare,
    fuel,
    FUEL_SEGMENTS,
    placeOnDay,
    type Beat,
    type Fuel,
  } from "../core/heartbeat";
  import { openKanban } from "../modules/kanbanApp";

  /** How often the strip is read again; a run or a session is minutes long, the strip need not be exact. */
  const REFRESH_MS = 30_000;
  /** How many recent runs are asked for: more than a day holds. */
  const RUN_LIMIT = 200;
  /** How much of a run's output the popover shows. */
  const OUTPUT_CHARS = 600;

  let beats = $state<Beat[]>([]);
  let tank = $state<Fuel | null>(null);
  let now = $state(Date.now());
  let shown = $state<RunRecord | null>(null);
  let root = $state<HTMLElement | null>(null);

  async function cardsOfAllBoards(): Promise<BoardCard[]> {
    const boards = await invoke<Board[]>("list_boards");
    const lists = await Promise.all(
      boards.map((b) => invoke<BoardCard[]>("list_board_cards", { boardId: b.id, includeArchived: true })),
    );
    return lists.flat();
  }

  /** A read in flight: the timer and a focus must not start a second one that could answer out of order. */
  let pending = false;

  async function refresh(): Promise<void> {
    if (pending || document.hidden) return;
    pending = true;
    try {
      await read();
    } finally {
      pending = false;
    }
  }

  async function read(): Promise<void> {
    const at = Date.now();
    // Each source on its own: a failing one leaves the others standing (and its last answer, not an empty strip).
    const [runs, cards, spend] = await Promise.allSettled([
      invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT }),
      cardsOfAllBoards(),
      invoke<SpendSummary[]>("get_spend_summary"),
    ]);
    now = at;
    const next: Beat[] = [];
    if (runs.status === "fulfilled") next.push(...beatsFromRuns(runs.value, at));
    else next.push(...beats.filter((b) => b.kind === "run"));
    if (cards.status === "fulfilled") next.push(...beatsFromCards(cards.value, at));
    else next.push(...beats.filter((b) => b.kind === "card"));
    beats = next.sort((a, b) => a.start - b.start);
    if (spend.status === "fulfilled") tank = fuel(spend.value);
  }

  onMount(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), REFRESH_MS);
    const onFocus = (): void => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  });

  const clock = (ms: number): string => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  function describe(beat: Beat): string {
    const until = beat.end === null ? "now" : clock(beat.end);
    return `${beat.label} · ${clock(beat.start)}–${until}`;
  }

  async function open(beat: Beat): Promise<void> {
    if (beat.kind === "card") {
      await openKanban({ ...(beat.boardId === undefined ? {} : { boardId: beat.boardId }), cardId: beat.id });
      return;
    }
    try {
      shown = await invoke<RunRecord | null>("get_run", { id: beat.id });
    } catch {
      shown = null;
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (shown && e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      shown = null;
    }
  }

  function onPointerDown(e: PointerEvent): void {
    if (shown && e.target instanceof Node && !root?.contains(e.target)) shown = null;
  }

  const nowLeft = $derived(dayShare(now, now));
  const money = (usd: number): string => `$${usd.toFixed(2)}`;
  const runMeta = (r: RunRecord): string =>
    `${clock(Date.parse(r.started_at))} · ${Math.round(r.duration_ms / 1000)} s · ${r.source} · ${r.backend}`;
  const excerpt = (text: string): string =>
    text.length > OUTPUT_CHARS ? `${text.slice(0, OUTPUT_CHARS)}…` : text;
</script>

<svelte:window onkeydown={onKeydown} onpointerdown={onPointerDown} />

<div class="heartbeat" bind:this={root}>
  <div class="strip" role="group" aria-label="Today's work: {beats.length} runs and sessions">
    <span class="tick" style:left="25%" aria-hidden="true"></span>
    <span class="tick" style:left="50%" aria-hidden="true"></span>
    <span class="tick" style:left="75%" aria-hidden="true"></span>
    {#each beats as beat (beat.key)}
      {@const place = placeOnDay(beat, now)}
      <button
        type="button"
        class="spark {beat.tone}"
        style:left="{place.left}%"
        style:width="{place.width}%"
        title={describe(beat)}
        aria-label={describe(beat)}
        onclick={() => void open(beat)}
      ></button>
    {/each}
    <span class="now" style:left="{nowLeft}%" aria-hidden="true"></span>
  </div>

  {#if tank}
    <div
      class="tank"
      class:hot={tank.hot}
      title={tank.capUsd === null
        ? `Spent today: ${money(tank.todayUsd)}, no daily limit`
        : `Spent today: ${money(tank.todayUsd)} of ${money(tank.capUsd)}`}
    >
      {#if tank.capUsd !== null}
        <span class="cells" aria-hidden="true">
          {#each { length: FUEL_SEGMENTS } as _, i (i)}<i class:full={i < tank.filled}></i>{/each}
        </span>
      {/if}
      <span class="amount">{money(tank.todayUsd)}</span>
    </div>
  {/if}

  {#if shown}
    <div class="run" role="dialog" aria-label="Run {shown.skill_name}">
      <p class="run-head">
        <strong>{shown.skill_name}</strong>
        <span class="status {shown.status}">{shown.status}</span>
      </p>
      <p class="meta">{runMeta(shown)}</p>
      {#if shown.error}<p class="error">{shown.error}</p>{/if}
      {#if shown.stdout}<pre>{excerpt(shown.stdout)}</pre>{/if}
    </div>
  {/if}
</div>

<style>
  .heartbeat {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: 0 var(--ax-space-2);
  }

  /* Today, midnight to midnight: a hairline with the quarter hours, the sparks on it. */
  .strip {
    position: relative;
    width: calc(180px * var(--ax-ui-scale));
    height: calc(20px * var(--ax-ui-scale));
    background: linear-gradient(var(--ax-border-strong), var(--ax-border-strong)) center / 100% 1px no-repeat;
  }

  .tick {
    position: absolute;
    top: 30%;
    bottom: 30%;
    width: 1px;
    background: var(--ax-border);
  }

  .now {
    position: absolute;
    top: 10%;
    bottom: 10%;
    width: 1px;
    background: var(--ax-text-muted);
  }

  .spark {
    position: absolute;
    top: calc(50% - 4px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    min-width: calc(3px * var(--ax-ui-scale));
    padding: 0;
    border: 0;
    border-radius: calc(2px * var(--ax-ui-scale));
    background: color-mix(in srgb, var(--ax-text-muted) 70%, transparent);
    cursor: pointer;
  }

  .spark:hover {
    background: var(--ax-text);
  }

  .spark.live {
    background: var(--ax-glow);
    box-shadow: var(--ax-glow-shadow);
    animation: var(--ax-anim-breathe);
  }

  .spark.bad {
    background: var(--ax-danger);
    box-shadow: 0 0 calc(6px * var(--ax-ui-scale)) color-mix(in srgb, var(--ax-danger) 60%, transparent);
  }

  .tank {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .cells {
    display: inline-flex;
    gap: calc(2px * var(--ax-ui-scale));
  }

  .cells i {
    width: calc(7px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    clip-path: var(--ax-hex-clip);
    background: var(--ax-border-strong);
  }

  .cells i.full {
    background: var(--ax-text-muted);
  }

  .tank.hot {
    color: var(--ax-danger);
  }

  .tank.hot .cells i.full {
    background: var(--ax-danger);
    filter: drop-shadow(0 0 calc(4px * var(--ax-ui-scale)) color-mix(in srgb, var(--ax-danger) 60%, transparent));
    animation: var(--ax-anim-breathe);
  }

  .run {
    position: absolute;
    top: calc(100% + var(--ax-space-3));
    left: 0;
    width: calc(360px * var(--ax-ui-scale));
    padding: var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: var(--ax-elev-3);
    box-shadow: var(--ax-shadow-panel);
    font: var(--ax-type-body);
  }

  .run p {
    margin: 0 0 var(--ax-space-1);
  }

  .run-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .status {
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  .status.failed,
  .error {
    color: var(--ax-danger);
  }

  .meta {
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  pre {
    margin: var(--ax-space-2) 0 0;
    max-height: calc(200px * var(--ax-ui-scale));
    overflow: auto;
    padding: var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-bg);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: pre-wrap;
  }
</style>
