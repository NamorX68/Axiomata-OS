<!--
  "Start …" on a card that is no step of a plan (`docs/plans/card-as-plan.md`, K1/K4): the card becomes the goal of a
  new plan in the chosen project and moves to In Progress; the Studio opens in the Flow on that plan, where the owner
  ticks or unticks "grill afterwards" and starts the planner. After the owner releases the plan its steps run by
  themselves (within a cost limit of its own), and the card follows them across the board.
-->
<script lang="ts">
  import { invokeBackend as invoke, type BoardCard, type BoardPlan, type IdeProject } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { startableProjects } from "../ide/cardStart";
  import { setGrillWish } from "../ide/grillAfter";
  import { showPlanInFlow } from "../ide/planRequest";
  import { listProjects } from "../ide/projects";
  import { tFlow } from "../i18n/flow";
  import { tKanban as t } from "../i18n/kanban";

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
      // K4: grilling is the default; the owner unticks "grill afterwards" in the Flow before starting the planner.
      setGrillWish(plan.id, true);
      showPlanInFlow(projectId, { boardId: plan.board_id, planId: plan.id });
      toast(t("nowPlan", { id: card.id, plan: plan.id }), "info");
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
    <p class="hint">{t("noProject")}</p>
  {:else}
    <label>
      {t("project")}
      <select bind:value={projectId} aria-label={t("project")}>
        {#each projects as project (project.id)}<option value={project.id}>{project.name}</option>{/each}
      </select>
    </label>
    <p class="hint">{t("planStartHint")}</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy || projectId === null}>
      {busy ? t("starting") : t("startAsPlan")}
    </button>
    <button class="ax-btn" type="button" onclick={onDone}>{tFlow("cancel")}</button>
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
