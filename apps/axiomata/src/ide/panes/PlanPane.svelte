<!--
  The Flow's planning panel (A2A CP-A7b, A36): the plans of a board, a form for a new one, and for the plan that is
  open the owner's own words (its goal), the planner that works on it, and its proposals with the owner's yes.

  The owner makes a plan with a goal, starts a planner on an engine of their choice (its pane opens beside this one),
  reads the cards the planner proposes — changing the role or the tier of one, saying yes or no to each — and approves
  the plan, which moves the proposals to Offen and ends the planner. Nothing here starts a card: that stays the Kanban's
  "Starten" until the automatic mode (CP-A8).
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import {
    invokeBackend as invoke,
    type Board,
    type BoardCard,
    type BoardPlan,
    type CardIntegrationResult,
    type CardTier,
    type IdeProject,
    type PlanSession,
    type PlanSpend,
    type PlanTakeOver,
  } from "../../core/backend";
  import { boardStore, refreshBoard, type BoardData } from "../../core/boardStore";
  import { emit } from "../../core/bus";
  import { messageOf } from "../../core/errors";
  import { STATE_LABEL, fieldsOf } from "../../core/kanban";
  import { listRoles, type Role } from "../../core/roster";
  import { toast } from "../../core/toast";
  import {
    assignableRoles,
    canApprove,
    canStartPlanner,
    defaultPlanId,
    needsLabel,
    newestFirst,
    plannerOf,
    planStatusLabel,
    proposalsOf,
    proposalTitle,
    allCardsOfPlan,
    canDeletePlan,
    cardsOfPlan,
    readyToTakeOver,
    runsByItself,
    spendLine,
  } from "../planning";
  import { gitApi } from "../../fileapp/gitBackend";
  import { projectRootId } from "../../fileapp/projectModel";
  import { unpushedNote } from "../cardStart";
  import { refreshAgents, session } from "../projectSession";
  import { flowSelection } from "../flowSelection";
  import ProposalEditor from "./ProposalEditor.svelte";
  import { engineCatalog, engineLine, refreshEngines } from "../rosterStore";

  let { project, visible }: { project: IdeProject; tabId: string; visible: boolean } = $props();

  const TIERS: { id: CardTier | ""; label: string }[] = [
    { id: "", label: "—" },
    { id: "light", label: "leicht" },
    { id: "medium", label: "mittel" },
    { id: "heavy", label: "schwer" },
  ];

  let boards = $state<Board[]>([]);
  let boardId = $state<number | null>(null);
  let data = $state<BoardData | null>(null);
  let roles = $state<Role[]>([]);
  let planId = $state<number | null>(null);
  let error = $state("");
  let busy = $state(false);

  // The board a project's plans are made on is remembered per project; the first board is the start.
  const boardKey = $derived(`ax-plan-board:${project.id}`);
  function rememberedBoard(): number | null {
    try {
      const raw = localStorage.getItem(boardKey);
      return raw === null ? null : Number(raw);
    } catch {
      return null;
    }
  }
  function chooseBoard(id: number): void {
    boardId = id;
    planId = null;
    try {
      localStorage.setItem(boardKey, String(id));
    } catch {
      // A remembered board is a convenience.
    }
  }

  $effect(() => {
    void refreshEngines();
    listRoles()
      .then((loaded) => (roles = loaded.roles))
      .catch(() => {
        // Without the roles a proposal's role is shown, not changed.
      });
    invoke<Board[]>("list_boards")
      .then((list) => {
        boards = list;
        const wanted = rememberedBoard();
        boardId = list.find((board) => board.id === wanted)?.id ?? list[0]?.id ?? null;
      })
      .catch((err: unknown) => (error = messageOf(err)));
  });

  // The board's data, shared with the Kanban's own store.
  $effect(() => {
    if (boardId === null) {
      data = null;
      return;
    }
    return boardStore(boardId).subscribe((value) => (data = value));
  });

  const plans = $derived(newestFirst(data?.plans ?? []));
  // Which plan is open: the one the owner picked while it still exists, else the newest draft.
  const openPlanId = $derived(
    planId !== null && plans.some((p) => p.id === planId) ? planId : defaultPlanId(plans),
  );
  const plan = $derived(plans.find((p) => p.id === openPlanId) ?? null);
  // The team's tiles and the graph show this plan: they have no selector of their own (`ide/flowSelection.ts`).
  $effect(() => flowSelection.set({ boardId, planId: plan?.id ?? null }));

  const cards = $derived(plan && data ? cardsOfPlan(data.cards, plan.id) : []);
  const proposals = $derived(plan && data ? proposalsOf(data.cards, plan.id) : []);
  const planner = $derived(plan ? plannerOf($session.agents, plan.id) : null);
  // Cards that left the board when the plan was taken over are still the plan's: a finished plan shows what it did.
  const working = $derived(
    plan && data ? allCardsOfPlan(data.cards, plan.id).filter((card) => card.state !== "proposed") : [],
  );
  const loadError = $derived(data?.error ?? "");

  // A planner writes its cards from another process: while one is at work the board is read again every few seconds.
  $effect(() => {
    if (!visible || boardId === null || plan?.status !== "draft" || planner === null) return;
    const id = boardId;
    const timer = setInterval(() => void refreshBoard(id), 3000);
    return () => clearInterval(timer);
  });

  // The board changes from outside the app too (the CLI, an agent): the plans are read again when the pane is shown and
  // when the window gets the focus back, as the Kanban does — the shared store does not poll.
  $effect(() => {
    if (!visible || boardId === null) return;
    const id = boardId;
    void refreshBoard(id);
    const again = () => void refreshBoard(id);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  });

  async function run(step: () => Promise<unknown>): Promise<boolean> {
    if (busy) return false;
    busy = true;
    error = "";
    try {
      await step();
      return true;
    } catch (err) {
      error = messageOf(err);
      return false;
    } finally {
      busy = false;
    }
  }

  async function reload(): Promise<void> {
    if (boardId !== null) await refreshBoard(boardId);
  }

  /* ---------------------------------------------------------- new plan --- */

  let creating = $state(false);
  let newName = $state("");
  let newGoal = $state("");

  async function createPlan(): Promise<void> {
    if (boardId === null || newName.trim() === "") return;
    const made = await run(async () => {
      const created = await invoke<BoardPlan>("create_board_plan", {
        boardId,
        fields: {
          name: newName.trim(),
          goal: newGoal.trim(),
          project_id: project.id,
          auto_start_max: null,
          max_cost_usd: null,
          max_tokens: null,
        },
      });
      await reload();
      planId = created.id;
    });
    if (made) {
      creating = false;
      newName = "";
      newGoal = "";
    }
  }

  /* -------------------------------------------------------------- goal --- */

  let goalDraft = $state("");
  let goalFor: number | null = null;
  /** The owner typed since the draft was taken from the plan: a goal changed from outside must not undo that. */
  let goalDirty = false;
  // A plan that is opened (or whose goal changed from outside while the owner is not typing) brings its own goal.
  $effect(() => {
    if (!plan) return;
    if (goalFor !== plan.id || !goalDirty) {
      goalDraft = plan.goal;
      goalFor = plan.id;
      goalDirty = false;
    }
  });

  /**
   * Writes the goal the owner typed, if there is one. Not gated on `busy`: a blur that arrives while another step runs
   * must not drop what was typed. `update_board_plan` replaces the plan's settings, so the others go along.
   */
  async function saveGoal(): Promise<void> {
    if (!plan || !goalDirty || goalFor !== plan.id) return;
    const current = plan;
    const goal = goalDraft;
    goalDirty = false;
    try {
      await invoke("update_board_plan", {
        id: current.id,
        fields: {
          name: current.name,
          goal,
          auto_start_max: current.auto_start_max,
          max_cost_usd: current.max_cost_usd,
          max_tokens: current.max_tokens,
        },
      });
      await reload();
    } catch (err) {
      goalDirty = true;
      error = messageOf(err);
    }
  }

  /** Another plan is opened: what was typed into this one's goal is written first. */
  async function openPlan(id: number): Promise<void> {
    await saveGoal();
    planId = id;
  }

  // The pane is closed with a goal not yet written: written now, from what is left of this component.
  onDestroy(() => void saveGoal());

  /* ----------------------------------------------------------- planner --- */

  let pickedEngine = $state("");
  // The planner role names no engine, so one must be chosen: the first is preselected, and the form never starts empty.
  const engineId = $derived(
    $engineCatalog.some((engine) => engine.id === pickedEngine) ? pickedEngine : ($engineCatalog[0]?.id ?? ""),
  );

  function showPlanner(agentId: number): void {
    emit("shell:agent", { projectId: project.id, agentId });
  }

  async function startPlanner(): Promise<void> {
    if (!plan || engineId === "") return;
    const current = plan;
    // The planner reads the goal from the database: what was typed and not yet written would not reach it.
    await saveGoal();
    await run(async () => {
      const made = await invoke<PlanSession>("start_plan_session", {
        planId: current.id,
        projectId: project.id,
        engineId,
      });
      await refreshAgents();
      toast(`Der Planer ${made.agent.name} liest das Projekt.`, "info");
      showPlanner(made.agent.id);
    });
  }

  /** The pane of a planner whose plan has had its say has nothing left to show: the Studio closes it. */
  function endPlanner(planIdEnded: number): void {
    const ids = $session.agents.filter((agent) => agent.plan_id === planIdEnded).map((agent) => agent.id);
    if (ids.length > 0) emit("studio:close-agent-panes", { agentIds: ids });
  }

  /* --------------------------------------------------------- the owner's --- */

  /**
   * Whether the plan, once approved, runs by itself (A4, CP-A8): its cards start as soon as what they build on is done —
   * as many at once as the dependencies allow —, are reviewed and are integrated into the plan's line. On by default:
   * a plan the owner has to feed card by card is what this panel exists to avoid.
   */
  let runByItself = $state(true);

  async function approvePlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await saveGoal();
    await run(async () => {
      // One step in the backend: the setting and the yes go together, so a yes that fails leaves no setting behind, and an
      // unchecked box makes the plan manual even if it was set to run by itself before.
      const moved = await invoke<number | null>("approve_board_plan", {
        id: current.id,
        runByItself,
        projectId: project.id,
      });
      endPlanner(current.id);
      await Promise.all([reload(), refreshAgents()]);
      toast(
        moved === null
          ? "Der Plan ist kein Entwurf mehr."
          : runByItself
            ? `Plan freigegeben: ${moved} Karte(n) werden abgearbeitet.`
            : `Plan freigegeben: ${moved} Karte(n) warten in der ersten offenen Spalte.`,
        "info",
      );
    });
  }

  /* ----------------------------------------------------- take the plan over --- */

  /** The files that conflict with the project's branch; the plan is as it was. */
  let takeOverConflict = $state<string[]>([]);

  /** The studio never pushes: the owner is told what waits, and pushes from the Studio's Git tab. */
  async function sayWhatIsNotPushed(projectId: number): Promise<void> {
    try {
      const state = await gitApi.status(projectRootId(projectId));
      const note = state.state === "ready" ? unpushedNote(state.status) : null;
      if (note) toast(note, "warning");
    } catch {
      // The note is a courtesy; the Git tab shows the same number.
    }
  }

  async function takeOverPlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    takeOverConflict = [];
    await run(async () => {
      const result = await invoke<PlanTakeOver>("take_over_plan", { id: current.id });
      if (result.outcome === "conflict") {
        takeOverConflict = result.files;
        return;
      }
      await Promise.all([reload(), refreshAgents()]);
      toast(`Plan „${current.name}“ ist übernommen (${result.commit.slice(0, 8)}): ${result.card_ids.length} Karte(n).`, "info");
      for (const note of result.cleanup) toast(`Nicht aufgeräumt: ${note}`, "warning");
      await sayWhatIsNotPushed(result.project_id);
    });
  }

  // What a plan that runs by itself has spent (CP-A8c): read again while the pane is shown, since the sessions that spend
  // it live in other processes. A failed read leaves the last figures.
  let spend = $state<PlanSpend | null>(null);
  $effect(() => {
    const current = plan;
    if (!visible || !current || !runsByItself(current)) {
      spend = null;
      return;
    }
    const read = (): void => {
      invoke<PlanSpend>("plan_spend", { id: current.id })
        .then((value) => (spend = value))
        .catch(() => {
          // The figures are a courtesy; the limits are enforced without them.
        });
    };
    read();
    const timer = setInterval(read, 5000);
    return () => clearInterval(timer);
  });

  // Cards the studio gave up integrating (they did not fit the plan's line twice): they wait for the owner (CP-A8c).
  let leftCards = $state<number[]>([]);
  $effect(() => {
    const current = plan;
    if (!visible || !current || !runsByItself(current)) {
      leftCards = [];
      return;
    }
    const read = (): void => {
      invoke<number[]>("plan_cards_left_for_owner", { id: current.id })
        .then((ids) => (leftCards = ids))
        .catch(() => {
          // The buttons are a courtesy; the CLI (`board integrate`, `board redo`) does the same.
        });
    };
    read();
    const timer = setInterval(read, 5000);
    return () => clearInterval(timer);
  });

  function closePanesOf(ids: number[]): void {
    if (ids.length > 0) emit("studio:close-agent-panes", { agentIds: ids });
  }

  async function integrateAgain(card: BoardCard): Promise<void> {
    await run(async () => {
      const result = await invoke<CardIntegrationResult>("integrate_card", { cardId: card.id });
      if (result.outcome === "done") {
        closePanesOf(result.agent_ids);
        toast(`Karte #${card.id} ist im Plan integriert.`, "info");
      } else if (result.outcome === "conflict") {
        toast(`Karte #${card.id} passt immer noch nicht in den Plan (${result.files.join(", ")}).`, "warning");
      } else if (result.outcome === "busy") {
        toast(`Der Arbeiter von Karte #${card.id} ist noch im Zug; versuch es gleich noch einmal.`, "warning");
      }
      await Promise.all([reload(), refreshAgents()]);
    });
  }

  async function redo(card: BoardCard): Promise<void> {
    await run(async () => {
      closePanesOf(await invoke<number[]>("redo_card", { cardId: card.id }));
      await Promise.all([reload(), refreshAgents()]);
      toast(`Karte #${card.id} wird auf dem neuen Stand des Plans noch einmal gemacht.`, "info");
    });
  }

  async function resumePlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await run(async () => {
      await invoke("resume_plan", { id: current.id });
      await reload();
      spend = await invoke<PlanSpend>("plan_spend", { id: current.id });
      toast(`Plan „${current.name}“ läuft weiter: neues Limit auf dem aktuellen Verbrauch aufgesetzt.`, "info");
    });
  }

  let deleting = $state(false);
  // The confirmation is for the plan it was asked about: another plan opened in between starts without it.
  $effect(() => {
    void plan?.id;
    deleting = false;
  });

  async function deletePlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await run(async () => {
      await invoke("delete_board_plan", { id: current.id });
      endPlanner(current.id);
      deleting = false;
      planId = null;
      await Promise.all([reload(), refreshAgents()]);
      toast(`Plan „${current.name}“ ist gelöscht; seine Karten bleiben auf dem Brett, ohne Plan.`, "info");
    });
  }

  async function closePlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await run(async () => {
      await invoke("close_board_plan", { id: current.id });
      endPlanner(current.id);
      await Promise.all([reload(), refreshAgents()]);
    });
  }

  async function approveProposal(card: BoardCard): Promise<void> {
    await run(async () => {
      await invoke("approve_card_proposal", { cardId: card.id });
      await reload();
    });
  }

  /** The card as the board has it now — the list on screen may be older than a planner's last write. */
  async function freshCard(id: number): Promise<BoardCard | null> {
    await reload();
    return data?.cards.find((card) => card.id === id) ?? null;
  }

  // The proposal being edited (its id), or "new" for a card the owner adds; at most one at a time.
  let editing = $state<number | "new" | null>(null);
  // Another plan opened in between must not keep an editor open for a card that is not on screen.
  $effect(() => {
    void plan?.id;
    editing = null;
  });
  const proposalColumnId = $derived(data?.columns.find((column) => column.stage === "proposal")?.id ?? null);

  let discarding = $state<number | null>(null);
  async function discardProposal(card: BoardCard): Promise<void> {
    await run(async () => {
      const fresh = await freshCard(card.id);
      discarding = null;
      // A card that was approved in the Kanban meanwhile is a live card, not a proposal to throw away.
      if (!fresh || fresh.state !== "proposed") {
        error = "Die Karte ist kein Vorschlag mehr und bleibt.";
        return;
      }
      await invoke("delete_card", { id: card.id });
      await reload();
    });
  }

  /**
   * The owner's change to a proposal. `update_card` replaces every field, so the card is read fresh first — a planner
   * may have rewritten its text since the list was drawn — and the select that was changed is put back to what the card
   * really holds when the change did not go through.
   */
  async function changeProposal(
    card: BoardCard,
    select: HTMLSelectElement,
    fields: { agent?: string | null; tier?: CardTier | null },
  ): Promise<void> {
    const done = await run(async () => {
      const fresh = (await freshCard(card.id)) ?? card;
      await invoke("update_card", { id: card.id, fields: { ...fieldsOf(fresh), ...fields } });
      await reload();
    });
    const now = data?.cards.find((c) => c.id === card.id) ?? card;
    if (!done) select.value = "agent" in fields ? (now.agent ?? "") : (now.tier ?? "");
  }
</script>

<div class="plan-pane">
  <aside class="plans" aria-label="Pläne">
    {#if boards.length > 1}
      <select
        aria-label="Brett"
        value={boardId}
        onchange={(event) => chooseBoard(Number((event.currentTarget as HTMLSelectElement).value))}
      >
        {#each boards as board (board.id)}<option value={board.id}>{board.name}</option>{/each}
      </select>
    {/if}
    <button class="ax-btn primary" type="button" disabled={boardId === null} onclick={() => (creating = !creating)}>
      Neuer Plan …
    </button>
    {#if creating}
      <form
        class="new-plan"
        onsubmit={(event) => {
          event.preventDefault();
          void createPlan();
        }}
      >
        <label>
          Name
          <input bind:value={newName} aria-label="Name des Plans" maxlength="200" />
        </label>
        <label>
          Ziel
          <textarea bind:value={newGoal} rows="5" maxlength="8000" aria-label="Ziel des Plans" placeholder="Was soll am Ende da sein?"></textarea>
        </label>
        <div class="row">
          <button class="ax-btn primary" type="submit" disabled={busy || newName.trim() === ""}>Anlegen</button>
          <button class="ax-btn" type="button" onclick={() => (creating = false)}>Abbrechen</button>
        </div>
      </form>
    {/if}
    <ul>
      {#each plans as item (item.id)}
        <li>
          <button type="button" class:on={item.id === openPlanId} onclick={() => void openPlan(item.id)}>
            <span class="name">{item.name}</span>
            <span class="status {item.status}">{planStatusLabel(item.status)}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if data && plans.length === 0 && !creating}
      <p class="hint">Noch kein Plan auf diesem Brett.</p>
    {/if}
    <span class="grow"></span>
    <button
      class="ax-btn"
      type="button"
      title="Planung, Agents und Flowansicht wieder an ihren Platz legen"
      onclick={() => emit("studio:reset-flow")}>Anordnung zurücksetzen</button
    >
  </aside>

  <section class="detail" aria-label="Plan">
    {#if error || loadError}<p class="error" role="alert">{error || loadError}</p>{/if}
    {#if boardId === null}
      <p class="hint">Es gibt noch kein Brett. Lege im Kanban eines an.</p>
    {:else if !plan}
      <p class="hint">Wähle einen Plan oder lege einen an: Das Ziel schreibst du, der Planer schneidet es in Karten.</p>
    {:else}
      <header>
        <h2>{plan.name}</h2>
        <span class="status {plan.status}">{planStatusLabel(plan.status)}</span>
        {#if runsByItself(plan)}<span class="status auto">läuft automatisch</span>{/if}
        <span class="spacer"></span>
        {#if plan.status === "draft"}
          <button class="ax-btn" type="button" disabled={busy} onclick={() => void closePlan()}>Schließen</button>
        {/if}
        {#if deleting}
          <button class="ax-btn danger" type="button" disabled={busy} onclick={() => void deletePlan()}>Wirklich löschen</button>
          <button class="ax-btn" type="button" onclick={() => (deleting = false)}>Abbrechen</button>
        {:else}
          <button
            class="ax-btn"
            type="button"
            disabled={busy || !canDeletePlan(plan, allCardsOfPlan(data?.cards ?? [], plan.id))}
            title={canDeletePlan(plan, allCardsOfPlan(data?.cards ?? [], plan.id))
              ? "Den Plan löschen; seine Karten bleiben ohne Plan auf dem Brett"
              : "Der Plan läuft noch: erst übernehmen oder schließen"}
            onclick={() => (deleting = true)}>Löschen</button
          >
        {/if}
      </header>

      <label class="goal">
        Ziel
        <textarea
          bind:value={goalDraft}
          rows="4"
          maxlength="8000"
          aria-label="Ziel"
          disabled={plan.status !== "draft"}
          oninput={() => (goalDirty = true)}
          onblur={() => void saveGoal()}
          placeholder="Was soll am Ende da sein?"
        ></textarea>
      </label>

      {#if plan.status === "draft"}
        <div class="planner">
          {#if planner}
            <span>Planer: <strong>{planner.name}</strong></span>
            <button class="ax-btn" type="button" onclick={() => showPlanner(planner.id)}>Pane zeigen</button>
          {:else if canStartPlanner(plan, $session.agents)}
            <label class="engine">
              Engine des Planers
              <select
                value={engineId}
                aria-label="Engine des Planers"
                onchange={(event) => (pickedEngine = (event.currentTarget as HTMLSelectElement).value)}
              >
                {#each $engineCatalog as engine (engine.id)}
                  <option value={engine.id}>{engine.label} — {engineLine(engine)}</option>
                {/each}
              </select>
            </label>
            <button
              class="ax-btn primary"
              type="button"
              disabled={busy || engineId === "" || goalDraft.trim() === ""}
              title={goalDraft.trim() === "" ? "Der Planer braucht ein Ziel." : ""}
              onclick={() => void startPlanner()}
            >
              Planer starten
            </button>
          {/if}
        </div>
      {/if}

      <h3>Vorschläge {proposals.length > 0 ? `(${proposals.length})` : ""}</h3>
      {#if proposals.length === 0}
        <p class="hint">
          {planner ? "Der Planer hat noch nichts vorgeschlagen." : "Keine Vorschläge, die auf dein Ja warten."}
        </p>
      {/if}
      <ul class="proposals">
        {#each proposals as card (card.id)}
          {#if editing === card.id && plan}
            <li class="editing">
              <ProposalEditor
                {card}
                {plan}
                planCards={allCardsOfPlan(data?.cards ?? [], plan.id)}
                {proposalColumnId}
                {roles}
                {freshCard}
                onSaved={reload}
                onCancel={() => (editing = null)}
              />
            </li>
          {:else}
          <li>
            <div class="title">
              <strong>{proposalTitle(card)}</strong>
              <span class="muted">#{card.id}{needsLabel(card) ? ` · braucht ${needsLabel(card)}` : ""}</span>
            </div>
            <div class="fields">
              <label>
                Rolle
                <select
                  value={card.agent ?? ""}
                  aria-label="Rolle von {proposalTitle(card)}"
                  onchange={(event) => {
                    const select = event.currentTarget as HTMLSelectElement;
                    void changeProposal(card, select, { agent: select.value || null });
                  }}
                >
                  <option value="">—</option>
                  {#each assignableRoles(roles, card.agent) as name (name)}<option value={name}>{name}</option>{/each}
                </select>
              </label>
              <label>
                Stufe
                <select
                  value={card.tier ?? ""}
                  aria-label="Stufe von {proposalTitle(card)}"
                  onchange={(event) => {
                    const select = event.currentTarget as HTMLSelectElement;
                    void changeProposal(card, select, { tier: (select.value || null) as CardTier | null });
                  }}
                >
                  {#each TIERS as tier (tier.id)}<option value={tier.id}>{tier.label}</option>{/each}
                </select>
              </label>
            </div>
            {#if card.agent_reason}<p class="reason">{card.agent_reason}</p>{/if}
            {#if card.acceptance}<p class="acceptance">{card.acceptance}</p>{/if}
            <div class="row">
              <button class="ax-btn" type="button" disabled={busy} onclick={() => void approveProposal(card)}>Annehmen</button>
              <button class="ax-btn" type="button" disabled={busy} onclick={() => (editing = card.id)}>Bearbeiten</button>
              {#if discarding === card.id}
                <button class="ax-btn danger" type="button" disabled={busy} onclick={() => void discardProposal(card)}>Wirklich verwerfen</button>
                <button class="ax-btn" type="button" onclick={() => (discarding = null)}>Abbrechen</button>
              {:else}
                <button class="ax-btn" type="button" disabled={busy} onclick={() => (discarding = card.id)}>Verwerfen</button>
              {/if}
            </div>
          </li>
          {/if}
        {/each}
      </ul>

      {#if plan.status === "draft"}
        {#if editing === "new"}
          <ProposalEditor
            card={null}
            {plan}
            planCards={allCardsOfPlan(data?.cards ?? [], plan.id)}
            {proposalColumnId}
            {roles}
            {freshCard}
            onSaved={reload}
            onCancel={() => (editing = null)}
          />
        {:else}
          <div class="row">
            <button class="ax-btn" type="button" disabled={busy || proposalColumnId === null} onclick={() => (editing = "new")}>
              Karte hinzufügen
            </button>
          </div>
        {/if}
      {/if}

      {#if plan.status === "draft"}
        <div class="approve">
          <button class="ax-btn primary" type="button" disabled={busy || !canApprove(plan, cards)} onclick={() => void approvePlan()}>
            Plan freigeben
          </button>
          <label class="check">
            <input type="checkbox" bind:checked={runByItself} />
            Automatisch abarbeiten
          </label>
          <span class="muted">
            {runByItself
              ? "Karten starten von selbst, sobald ihre Vorgänger fertig sind, so viele gleichzeitig wie möglich; der Review läuft von selbst."
              : "Die Vorschläge wandern in die erste offene Spalte; du startest jede Karte selbst."}
            Der Planer ist danach fertig.
          </span>
        </div>
      {/if}

      {#if spend}
        <div class="approve">
          <span>Verbrauch: {spendLine(spend)}</span>
          {#if spend.plan_over}
            <span class="error" role="status">Pausiert: {spend.plan_over}.</span>
            <button class="ax-btn primary" type="button" disabled={busy} onclick={() => void resumePlan()}>Weiter</button>
            <span class="muted">Setzt das Limit des Plans auf den bisherigen Verbrauch plus eine neue Zuteilung.</span>
          {/if}
          {#if spend.day_over}
            <span class="error" role="status">
              Tageslimit der Studio-Sitzungen erreicht: {spend.day_over}. Bis morgen startet nichts Neues; anheben:
              <code>studio_daily_usd_cap</code> in der Config.
            </span>
          {:else if spend.day_cap_usd !== null}
            <span class="muted">Heute: ${spend.today.cost_usd.toFixed(2)} von ${spend.day_cap_usd.toFixed(2)}</span>
          {/if}
        </div>
      {/if}

      {#if readyToTakeOver(plan, cards)}
        <div class="approve">
          <button class="ax-btn primary" type="button" disabled={busy} onclick={() => void takeOverPlan()}>
            Plan übernehmen
          </button>
          <span class="muted">
            Alle Karten sind auf der Linie des Plans. Die Arbeit geht in den Hauptzweig des Projekts, ihre Commits bleiben
            einzeln, solange der Zweig sich nicht bewegt hat. Es wird nichts gepusht; ein Konflikt wird zurückgenommen.
          </span>
          {#if takeOverConflict.length > 0}
            <p class="error" role="alert">
              Konflikt mit dem Hauptzweig, nichts wurde verändert: {takeOverConflict.join(", ")}
            </p>
          {/if}
        </div>
      {/if}

      {#if working.length > 0}
        <h3>Karten des Plans</h3>
        <ul class="cards">
          {#each working as card (card.id)}
            <li>
              <span>#{card.id} {proposalTitle(card)}</span>
              <span class="muted">{card.agent ?? "—"} · {STATE_LABEL[card.state]}</span>
              {#if leftCards.includes(card.id)}
                <span class="error" role="status">Passt zweimal nicht in den Plan; das Studio hat aufgegeben.</span>
                <button class="ax-btn" type="button" disabled={busy} onclick={() => void integrateAgain(card)}>Erneut integrieren</button>
                <button class="ax-btn" type="button" disabled={busy} onclick={() => void redo(card)}>Neu machen</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>
</div>

<style>
  .plan-pane {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: calc(240px * var(--ax-ui-scale)) 1fr;
    overflow: hidden;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }
  .plans {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    border-right: 1px solid var(--ax-border);
    overflow-y: auto;
  }
  .plans ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .plans li button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .plans li button:hover {
    background: var(--ax-surface-2);
  }
  .plans li button.on {
    background: var(--ax-accent-muted);
    border-color: var(--ax-accent);
  }
  .grow {
    flex: 1;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    flex: none;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .status.draft {
    color: var(--ax-accent);
  }
  .new-plan,
  .planner,
  .approve {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
  }
  .planner,
  .approve {
    flex-direction: row;
    align-items: center;
    flex-wrap: wrap;
  }
  .detail {
    padding: var(--ax-space-3) var(--ax-space-4);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  h2,
  h3 {
    margin: 0;
    font-size: var(--ax-font-size-base);
  }
  h3 {
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .spacer {
    flex: 1;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
  }
  input,
  select,
  textarea {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font: inherit;
  }
  textarea {
    resize: vertical;
  }
  .row {
    display: flex;
    gap: var(--ax-space-2);
  }
  .proposals,
  .cards {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  .proposals li {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
  }
  .title {
    display: flex;
    justify-content: space-between;
    gap: var(--ax-space-2);
  }
  .fields {
    display: flex;
    gap: var(--ax-space-3);
  }
  .cards li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text);
  }
  .muted,
  .hint {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .hint {
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }
  .reason,
  .acceptance {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .reason {
    color: var(--ax-text-muted);
  }
  .error {
    margin: 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
</style>
