<!--
  "Start …" on a card that is no step of a plan (`docs/plans/card-as-plan.md`, K1/K4): the card becomes the goal of a
  new plan in the chosen project and moves to In Progress; the Studio opens in the Flow on that plan and starts its
  planner, which proposes the steps and then grills the owner about them. After the owner releases the plan its steps
  run by themselves, and the card follows them across the board.
-->
<script lang="ts">
  import { invokeBackend as invoke, type BoardCard, type BoardPlan, type IdeProject } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { startableProjects } from "../ide/cardStart";
  import { setGrillWish } from "../ide/grillAfter";
  import { showPlanInFlow } from "../ide/planRequest";
  import { listProjects } from "../ide/projects";

  interface Props {
    card: BoardCard;
    onDone: () => void;
    /** The Studio is opening on the plan: the board makes room for it. */
    onStarted?: () => void;
  }

  let { card, onDone, onStarted }: Props = $props();

  let projects = $state<IdeProject[]>([]);
  let projectId = $state<number | null>(null);
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    listProjects()
      .then((all) => {
        projects = startableProjects(all);
        projectId = projects[0]?.id ?? null;
      })
      .catch((err: unknown) => (error = messageOf(err)));
  });

  async function start(): Promise<void> {
    if (projectId === null || busy) return;
    busy = true;
    error = "";
    try {
      const plan = await invoke<BoardPlan>("start_card_plan", { cardId: card.id, projectId });
      // K4: first the planner splits the card, then it is grilled — the wish the Flow's "grill afterwards" holds.
      setGrillWish(plan.id, true);
      showPlanInFlow(projectId, { boardId: plan.board_id, planId: plan.id, startPlanner: true });
      toast(`Card #${card.id} is now plan #${plan.id}: the planner proposes its steps in the Studio.`, "info");
      onDone();
      onStarted?.();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="start-form"
  onsubmit={(event) => {
    event.preventDefault();
    void start();
  }}
>
  {#if projects.length === 0}
    <p class="hint">There is no project the card could run in. Create one in the Studio.</p>
  {:else}
    <label>
      Project
      <select bind:value={projectId} aria-label="Project">
        {#each projects as project (project.id)}<option value={project.id}>{project.name}</option>{/each}
      </select>
    </label>
    <p class="hint">
      The card becomes a plan: a planner proposes its steps and then questions you about them in the Studio. Once you
      release the plan, the steps run by themselves and the card moves along — to Review when everything is ready for
      your take-over.
    </p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy || projectId === null}>
      {busy ? "Starting …" : "Start as plan"}
    </button>
    <button class="ax-btn" type="button" onclick={onDone}>Cancel</button>
  </div>
</form>

<style>
  .start-form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin: 0 0 var(--ax-space-3);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    font-size: var(--ax-font-size-sm);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
  }

  select {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font: inherit;
  }

  .row {
    display: flex;
    gap: var(--ax-space-2);
  }

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
  }

  .error {
    margin: 0;
    color: var(--ax-warning);
  }
</style>
