<!--
  "An agent waits for you" (`docs/plans/agent-waiting.md`, W1–W3): a hexagon in the `warn` tone with the number of
  waiting agents — the shape, not a second orange, tells it apart from the glow of an agent at work. It nudges under
  motion Full and holds still under Calm and Off. A click opens the one agent's pane in the Studio; with several, a
  small list opens, and each row opens its agent. Nothing is drawn while nobody waits.

  The list is moved under `body` (`ui/portal.ts`) and fixed to the window (`placeList`), not to the badge: a badge in a
  scrolling Kanban column, a transformed panel or at the Dock's left end would otherwise open a list that is cut off. Escape and ⌘W close the list alone (`core/closeStack.ts`).

  A permission request the app can answer is shown under its agent with its answers (`AskAnswer.svelte`, card #88); then
  even one waiting agent opens the list rather than its pane. A session idle on a draft plan says so.
-->
<script lang="ts">
  import { badgeClick, placeList, waitedFor, type WaitingAgent } from "../core/agentWaiting";
  import { emit } from "../core/bus";
  import { pushClose } from "../core/closeStack";
  import AskAnswer from "./AskAnswer.svelte";
  import { portal } from "./portal";

  interface Props {
    agents: WaitingAgent[];
    size?: "sm" | "md" | "lg";
  }

  let { agents, size = "md" }: Props = $props();

  /** Space between the badge and its list, and the least space between the list and the window's edge, in px. */
  const GAP_PX = 6;
  const MARGIN_PX = 8;
  /** How often "since" is read again while the list is open. */
  const TICK_MS = 30_000;

  let listOpen = $state(false);
  let root = $state<HTMLElement | null>(null);
  let badge = $state<HTMLButtonElement | null>(null);
  let list = $state<HTMLElement | null>(null);
  let position = $state<{ left: number; top: number } | null>(null);
  let now = $state(Date.now());

  const label = $derived(
    agents.length === 1
      ? `${agents[0]?.name ?? "An agent"} is waiting for you`
      : `${agents.length} agents are waiting for you`,
  );

  function openAgent(agent: WaitingAgent): void {
    listOpen = false;
    emit("shell:agent", { projectId: agent.project_id, agentId: agent.agent_id });
  }

  function onclick(event: MouseEvent): void {
    // The badge may sit on something clickable (a Kanban card): the click is the badge's alone.
    event.stopPropagation();
    const click = badgeClick(agents);
    if (click.kind === "open") openAgent(click.agent);
    else if (click.kind === "list") listOpen = !listOpen;
  }

  function onKeydown(event: KeyboardEvent): void {
    if (listOpen && event.key === "Escape") {
      // Captured before any panel's own Escape: only the list closes, not the Kanban or the Studio under it.
      event.preventDefault();
      event.stopPropagation();
      listOpen = false;
    }
  }

  function onPointerDown(event: PointerEvent): void {
    if (!listOpen || !(event.target instanceof Node)) return;
    // The list lives under `body` (portal), so it is not inside `root`.
    if (!root?.contains(event.target) && !list?.contains(event.target)) listOpen = false;
  }

  // The list closes by itself when the agents it listed stopped waiting.
  $effect(() => {
    if (badgeClick(agents).kind !== "list") listOpen = false;
  });

  // While the list is open: ⌘W closes it first, and "since" keeps counting.
  $effect(() => {
    if (!listOpen) return;
    now = Date.now();
    const offClose = pushClose(() => (listOpen = false));
    const timer = setInterval(() => (now = Date.now()), TICK_MS);
    return () => {
      offClose();
      clearInterval(timer);
    };
  });

  // Placed once the list is drawn and its size is known; again when the window changes size.
  $effect(() => {
    if (!listOpen || !list || !badge) {
      position = null;
      return;
    }
    const place = (): void => {
      if (!list || !badge) return;
      position = placeList(
        badge.getBoundingClientRect(),
        { width: list.offsetWidth, height: list.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
        GAP_PX,
        MARGIN_PX,
      );
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  });
</script>

<svelte:window onkeydowncapture={onKeydown} onpointerdown={onPointerDown} />

{#if agents.length > 0}
  <span class="waiting" bind:this={root}>
    <button
      type="button"
      class="badge {size}"
      bind:this={badge}
      title={label}
      aria-label={label}
      aria-expanded={agents.length > 1 ? listOpen : undefined}
      {onclick}
    >
      <span class="hex">{agents.length}</span>
    </button>
    {#if listOpen}
      <div
        class="list"
        class:placed={position !== null}
        bind:this={list}
        use:portal
        style:left={position ? `${position.left}px` : undefined}
        style:top={position ? `${position.top}px` : undefined}
        role="dialog"
        aria-label="Agents waiting for you"
      >
        <p class="head">Waiting for you</p>
        <ul>
          {#each agents as agent (agent.agent_id)}
            <li>
              <button
                type="button"
                class="entry"
                onclick={(event) => {
                  event.stopPropagation();
                  openAgent(agent);
                }}
              >
                <span class="name">{agent.name}</span>
                <span class="card">
                  {agent.card_id !== null ? `#${agent.card_id} ${agent.card_title ?? ""}` : "no card"}
                </span>
                <span class="since">{waitedFor(agent.since, now)}</span>
              </button>
              {#if agent.ask}
                <div class="request"><AskAnswer agentId={agent.agent_id} ask={agent.ask} /></div>
              {:else if agent.reason === "plan_draft"}
                <p class="note">waits for your answer on its plan</p>
              {/if}
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </span>
{/if}

<style>
  .waiting {
    display: inline-flex;
  }

  /* The button stays a box so its focus ring is not clipped away; the hexagon is drawn inside it. */
  .badge {
    display: inline-grid;
    place-items: center;
    padding: 0;
    border: none;
    border-radius: var(--ax-radius-md);
    background: transparent;
    cursor: pointer;
  }

  .badge:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 1px;
  }

  .hex {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    clip-path: var(--ax-hex-clip);
    background: var(--ax-warning);
    color: var(--ax-text-invert);
    font: 600 var(--ax-font-size-xs) / 1 var(--ax-font-sans);
    animation: var(--ax-anim-nudge);
  }

  .sm {
    width: calc(16px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
  }

  .sm .hex {
    font-size: calc(10px * var(--ax-ui-scale));
  }

  .md {
    width: calc(22px * var(--ax-ui-scale));
    height: calc(25px * var(--ax-ui-scale));
  }

  .lg {
    width: calc(44px * var(--ax-ui-scale));
    height: calc(50px * var(--ax-ui-scale));
  }

  .lg .hex {
    font-size: calc(20px * var(--ax-ui-scale));
  }

  .list {
    position: fixed;
    z-index: var(--ax-z-popover);
    width: calc(340px * var(--ax-ui-scale));
    max-height: calc(320px * var(--ax-ui-scale));
    overflow: auto;
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: var(--ax-elev-3);
    box-shadow: var(--ax-shadow-panel);
    text-align: left;
    /* Measured before it is placed: invisible for that one frame instead of flashing in the corner. */
    visibility: hidden;
  }

  .list.placed {
    visibility: visible;
  }

  .head {
    margin: 0 0 var(--ax-space-1);
    padding: 0 var(--ax-space-1);
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: var(--ax-space-2);
    align-items: baseline;
    width: 100%;
    padding: var(--ax-space-1);
    border: none;
    border-radius: var(--ax-radius-md);
    background: transparent;
    color: var(--ax-text);
    font: var(--ax-type-body);
    text-align: left;
    cursor: pointer;
  }

  .entry:hover,
  .entry:focus-visible {
    background: var(--ax-selected-bg);
  }

  .card {
    overflow: hidden;
    color: var(--ax-text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .since {
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
  }

  .request,
  .note {
    margin: 0 0 var(--ax-space-1);
    padding: 0 var(--ax-space-1) var(--ax-space-1);
  }

  .note {
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
  }
</style>
