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
    type GoalSuggestion,
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
    grillerOf,
    defaultPlanId,
    needsLabel,
    newestFirst,
    plannerDone,
    plannerOf,
    planStatusLabel,
    proposalsOf,
    proposalTitle,
    allCardsOfPlan,
    canDeletePlan,
    rolesOfKind,
    sessionEngine,
    cardEditable,
    newCardColumn,
    cardsOfPlan,
    readyToTakeOver,
    runsByItself,
    spendLine,
  } from "../planning";
  import { gitApi } from "../../fileapp/gitBackend";
  import { projectRootId } from "../../fileapp/projectModel";
  import { unpushedNote } from "../cardStart";
  import { agentStatus } from "../agentStatus";
  import { grillAfter, setGrillWish } from "../grillAfter";
  import { refreshAgents, session } from "../projectSession";
  import { flowSelection } from "../flowSelection";
  import ProposalEditor from "./ProposalEditor.svelte";
  import SessionPicker from "./SessionPicker.svelte";
  import { engineCatalog, refreshEngines } from "../rosterStore";

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
  // The planner and the grilling session are told apart by their roles' kinds: until the roles are in, neither can be started.
  let rolesLoaded = $state(false);
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
      .then((loaded) => {
        roles = loaded.roles;
        rolesLoaded = true;
      })
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
  const planner = $derived(plan ? plannerOf($session.agents, plan.id, roles) : null);
  const griller = $derived(plan ? grillerOf($session.agents, plan.id, roles) : null);
  // Cards that left the board when the plan was taken over are still the plan's: a finished plan shows what it did.
  const working = $derived(
    plan && data ? allCardsOfPlan(data.cards, plan.id).filter((card) => card.state !== "proposed") : [],
  );
  const loadError = $derived(data?.error ?? "");

  // A planner writes its cards from another process: while one is at work the board is read again every few seconds.
  $effect(() => {
    if (!visible || boardId === null || plan?.status !== "draft" || (planner === null && griller === null)) return;
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

  // A session starts from a ROLE, which carries its engine; only a role that names none asks for one — once, and the pick is
  // saved into the role (`SessionPicker.svelte`). Engines and roles are changed under "Engines & roles".
  let plannerRole = $state("");
  let plannerEngine = $state("");
  let grillRole = $state("");
  let grillEngine = $state("");
  const plannerNames = $derived(rolesOfKind(roles, "plan", "planner"));
  const grillNames = $derived(rolesOfKind(roles, "grill", "grill"));

  function showPlanner(agentId: number): void {
    emit("shell:agent", { projectId: project.id, agentId });
  }

  /**
   * Starts a session of `roleName` for the plan. A role without an engine gets the picked one written into it first, so the
   * next start is one click; a role with one is started as it is.
   */
  async function startSession(roleName: string, pickedEngine: string, grill: boolean): Promise<PlanSession | null> {
    const role = roles.find((r) => r.name === roleName);
    if (!plan || !role) return null;
    const choice = sessionEngine(role, $engineCatalog, pickedEngine);
    if (choice.engineId === "") return null;
    const current = plan;
    // The session reads the goal from the database: what was typed and not yet written would not reach it.
    await saveGoal();
    let made: PlanSession | null = null;
    await run(async () => {
      if (choice.ask) {
        await invoke("save_role", { role: { ...role, engine: choice.engineId } });
        roles = (await listRoles()).roles;
      }
      made = await invoke<PlanSession>("start_plan_session", {
        planId: current.id,
        projectId: project.id,
        grill,
        role: roleName,
      });
      await refreshAgents();
      showPlanner(made.agent.id);
    });
    return made;
  }

  async function startPlanner(): Promise<void> {
    const made = await startSession(plannerRole || plannerNames[0] || "", plannerEngine, false);
    if (made) toast(`The planner ${made.agent.name} is reading the project.`, "info");
  }

  async function startGrill(): Promise<boolean> {
    const made = await startSession(grillRole || grillNames[0] || "", grillEngine, true);
    if (made) toast(`${made.agent.name} is now questioning you about the cards in the terminal.`, "info");
    return made !== null;
  }

  // "Danach grillen": the grilling session starts by itself once the planner has proposed cards and waits — or, when
  // the
  // planner is gone (a restart ended it), as soon as there are proposals: the grill then hands its findings to the
  // owner.
  // A plain flag guards against a second start while the first is on its way; the tick is cleared once the start is
  // over,
  // whichever way it went, and a start that did not happen says so (the picks or the goal may be wrong) instead of
  // retrying
  // in a loop. Only while the panel is open — an effect of an unmounted panel does not run, and the grill then starts
  // when
  // the owner is back.
  let grillStarting = false;
  const statuses = agentStatus.statuses;
  const grillWanted = $derived(plan ? ($grillAfter[plan.id] ?? false) : false);
  const grillEngineChosen = $derived(
    sessionEngine(
      roles.find((r) => r.name === (grillRole || grillNames[0])) ?? null,
      $engineCatalog,
      grillEngine,
    ).engineId !== "",
  );
  $effect(() => {
    const current = plan;
    if (!current || current.status !== "draft" || !grillWanted || !grillEngineChosen || busy) return;
    if (griller !== null || grillStarting || proposals.length === 0) return;
    if (planner !== null && !plannerDone($statuses.byAgent.get(planner.id)?.state, proposals.length)) return;
    grillStarting = true;
    void startGrill().then((started) => {
      grillStarting = false;
      setGrillWish(current.id, false);
      if (!started) toast("The grilling could not be started; tick the box again.", "warning");
    });
  });

  // A sharper goal a grilling session proposed: it waits for the owner, who takes it over or discards it. Read again while
  // a grilling session exists (it writes from another process) and whenever the plan changes.
  let suggestion = $state<GoalSuggestion | null>(null);
  $effect(() => {
    const current = plan;
    const grilling = griller !== null;
    if (!current || current.status !== "draft") {
      suggestion = null;
      return;
    }
    let stale = false;
    const read = (): void => {
      invoke<GoalSuggestion | null>("plan_goal_suggestion", { id: current.id })
        .then((value) => {
          if (!stale) suggestion = value;
        })
        .catch(() => {
          // The box is a courtesy; `board plan goal` shows the same.
        });
    };
    read();
    const timer = grilling && visible ? setInterval(read, 3000) : null;
    return () => {
      stale = true;
      if (timer !== null) clearInterval(timer);
    };
  });

  // Whether the goal was sharpened in an interview: the approval says so when it was not (a hint, never a gate).
  let grilled = $state(true);
  $effect(() => {
    const current = plan;
    void suggestion;
    // Starting a grilling session marks the plan as grilled in the backend.
    void griller?.id;
    if (!current || current.status !== "draft") return;
    let stale = false;
    invoke<boolean>("plan_grilled", { id: current.id })
      .then((value) => {
        if (!stale) grilled = value;
      })
      .catch(() => {
        // The hint is a courtesy.
      });
    return () => {
      stale = true;
    };
  });

  async function takeOverGoal(): Promise<void> {
    if (!plan) return;
    const current = plan;
    const read = suggestion;
    if (!read) return;
    await run(async () => {
      // `at` is the proposal on screen: one a session has replaced since is refused, not applied unseen.
      await invoke("apply_goal_suggestion", { id: current.id, at: read.at });
      // What the owner typed since is replaced on purpose: they chose the proposal over it.
      goalDirty = false;
      suggestion = null;
      await reload();
    });
  }

  async function discardGoal(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await run(async () => {
      await invoke("discard_goal_suggestion", { id: current.id });
      suggestion = null;
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
          ? "The plan is no longer a draft."
          : runByItself
            ? `Plan released: ${moved} card(s) are being worked on.`
            : `Plan released: ${moved} card(s) wait in the first open column.`,
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
      toast(`Plan “${current.name}” was taken over (${result.commit.slice(0, 8)}): ${result.card_ids.length} card(s).`, "info");
      for (const note of result.cleanup) toast(`Not cleaned up: ${note}`, "warning");
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
        toast(`Card #${card.id} is integrated into the plan.`, "info");
      } else if (result.outcome === "conflict") {
        toast(`Card #${card.id} still does not fit into the plan (${result.files.join(", ")}).`, "warning");
      } else if (result.outcome === "busy") {
        toast(`The worker of card #${card.id} is still moving; try again in a moment.`, "warning");
      }
      await Promise.all([reload(), refreshAgents()]);
    });
  }

  async function redo(card: BoardCard): Promise<void> {
    await run(async () => {
      closePanesOf(await invoke<number[]>("redo_card", { cardId: card.id }));
      await Promise.all([reload(), refreshAgents()]);
      toast(`Card #${card.id} will be redone on the plan's new state.`, "info");
    });
  }

  async function resumePlan(): Promise<void> {
    if (!plan) return;
    const current = plan;
    await run(async () => {
      await invoke("resume_plan", { id: current.id });
      await reload();
      spend = await invoke<PlanSpend>("plan_spend", { id: current.id });
      toast(`Plan “${current.name}” keeps running: a new limit was set on top of the current spend.`, "info");
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
      toast(`Plan “${current.name}” was deleted; its cards stay on the board, without a plan.`, "info");
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
  // The proposals a session changed after proposing them: the owner re-reads those (`plan_changed_proposals`).
  let changedByPlanner = $state<number[]>([]);
  $effect(() => {
    const current = plan;
    void data?.cards;
    if (!current) {
      changedByPlanner = [];
      return;
    }
    let stale = false;
    invoke<number[]>("plan_changed_proposals", { id: current.id })
      .then((ids) => {
        if (!stale) changedByPlanner = ids;
      })
      .catch(() => {
        // A marker is a courtesy.
      });
    return () => {
      stale = true;
    };
  });
  // Another plan opened in between must not keep an editor open for a card that is not on screen.
  $effect(() => {
    void plan?.id;
    editing = null;
  });
  // Where a card the owner adds goes: the proposal column of a draft, the first plain open column of a running plan.
  const newColumnId = $derived(plan && data ? newCardColumn(data.columns, plan) : null);

  let discarding = $state<number | null>(null);
  async function discardProposal(card: BoardCard): Promise<void> {
    await run(async () => {
      const fresh = await freshCard(card.id);
      discarding = null;
      // A card that was approved in the Kanban meanwhile is a live card, not a proposal to throw away.
      if (!fresh || fresh.state !== "proposed") {
        error = "The card is no longer a proposal and stays.";
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
  <aside class="plans" aria-label="Plans">
    {#if boards.length > 1}
      <select
        aria-label="Board"
        value={boardId}
        onchange={(event) => chooseBoard(Number((event.currentTarget as HTMLSelectElement).value))}
      >
        {#each boards as board (board.id)}<option value={board.id}>{board.name}</option>{/each}
      </select>
    {/if}
    <button class="ax-btn primary" type="button" disabled={boardId === null} onclick={() => (creating = !creating)}>
      New plan …
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
          Goal
          <textarea
            bind:value={newGoal}
            rows="10"
            maxlength="8000"
            aria-label="Goal of the plan"
            placeholder="Was soll am Ende da sein?"
          ></textarea>
        </label>
        <div class="row">
          <button class="ax-btn primary" type="submit" disabled={busy || newName.trim() === ""}>Anlegen</button>
          <button class="ax-btn" type="button" onclick={() => (creating = false)}>Cancel</button>
        </div>
      </form>
    {/if}
    <ul>
      {#each plans as item (item.id)}
        <li>
          <button type="button" class:on={item.id === openPlanId} onclick={() => void openPlan(item.id)}>
            <span class="head">
              <span class="name">{item.name}</span>
              <span class="status {item.status}">{planStatusLabel(item.status)}</span>
            </span>
            {#if item.goal.trim() !== ""}<span class="goal-line">{item.goal}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    {#if data && plans.length === 0 && !creating}
      <p class="hint">No plan on this board yet.</p>
    {/if}
    <span class="grow"></span>
    <button
      class="ax-btn"
      type="button"
      title="Put Planning, Agents and the Flow view back in their places"
      onclick={() => emit("studio:reset-flow")}>Reset layout</button
    >
  </aside>

  <section class="detail" aria-label="Plan">
    {#if error || loadError}<p class="error" role="alert">{error || loadError}</p>{/if}
    {#if boardId === null}
      <p class="hint">There is no board yet. Create one in the Kanban.</p>
    {:else if !plan}
      <p class="hint">Pick a plan or create one: you write the goal, the planner cuts it into cards.</p>
    {:else}
      <header>
        <h2>{plan.name}</h2>
        <span class="status {plan.status}">{planStatusLabel(plan.status)}</span>
        {#if runsByItself(plan)}<span class="status auto">runs by itself</span>{/if}
        <span class="spacer"></span>
        {#if plan.status === "draft"}
          <button class="ax-btn" type="button" disabled={busy} onclick={() => void closePlan()}>Close</button>
        {/if}
        {#if deleting}
          <button class="ax-btn danger" type="button" disabled={busy} onclick={() => void deletePlan()}>Really delete</button>
          <button class="ax-btn" type="button" onclick={() => (deleting = false)}>Cancel</button>
        {:else}
          <button
            class="ax-btn"
            type="button"
            disabled={busy || !canDeletePlan(plan, allCardsOfPlan(data?.cards ?? [], plan.id))}
            title={canDeletePlan(plan, allCardsOfPlan(data?.cards ?? [], plan.id))
              ? "Delete the plan; its cards stay on the board without a plan"
              : "The plan is still running: take it over or close it first"}
            onclick={() => (deleting = true)}>Delete</button
          >
        {/if}
      </header>

      <label class="goal">
        Goal
        <textarea
          bind:value={goalDraft}
          rows="7"
          maxlength="8000"
          aria-label="Goal"
          disabled={plan.status !== "draft"}
          oninput={() => (goalDirty = true)}
          onblur={() => void saveGoal()}
          placeholder="Was soll am Ende da sein?"
        ></textarea>
      </label>

      {#if suggestion && plan.status === "draft"}
        <section class="suggestion" aria-label="Suggested goal">
          <h3>Sharpened goal suggested</h3>
          <p class="muted">From the conversation with the grill session. It replaces your goal only when you take it over.</p>
          <pre class="proposed-goal">{suggestion.goal}</pre>
          <div class="row">
            <button class="ax-btn primary" type="button" disabled={busy} onclick={() => void takeOverGoal()}>Take over</button>
            <button class="ax-btn" type="button" disabled={busy} onclick={() => void discardGoal()}>Discard</button>
          </div>
        </section>
      {/if}

      {#if plan.status === "draft"}
        <div class="planner">
          {#if planner}
            <span>Planer: <strong>{planner.name}</strong></span>
            <button class="ax-btn" type="button" onclick={() => showPlanner(planner.id)}>Show pane</button>
          {/if}
          {#if griller}
            <span>Grill: <strong>{griller.name}</strong></span>
            <button class="ax-btn" type="button" onclick={() => showPlanner(griller.id)}>Show pane</button>
          {/if}
          {#if rolesLoaded}
            <div class="start">
              {#if canStartPlanner(plan, $session.agents, roles) && plannerNames.length > 0}
                <SessionPicker
                  label="Role of the planner"
                  names={plannerNames}
                  {roles}
                  catalog={$engineCatalog}
                  bind:role={plannerRole}
                  bind:engine={plannerEngine}
                />
              {/if}
              {#if griller === null && grillNames.length > 0}
                <label class="check">
                  <input
                    type="checkbox"
                    checked={grillWanted}
                    onchange={(event) => setGrillWish(plan.id, event.currentTarget.checked)}
                  />
                  Danach grillen
                </label>
                {#if grillWanted}
                  <SessionPicker
                    label="Role for grilling"
                    names={grillNames}
                    {roles}
                    catalog={$engineCatalog}
                    bind:role={grillRole}
                    bind:engine={grillEngine}
                  />
                {/if}
              {/if}
              {#if canStartPlanner(plan, $session.agents, roles) && plannerNames.length > 0}
                <button
                  class="ax-btn primary"
                  type="button"
                  disabled={busy || goalDraft.trim() === ""}
                  title={goalDraft.trim() === "" ? "The planner needs a goal." : ""}
                  onclick={() => void startPlanner()}
                >
                  Planer starten
                </button>
              {/if}
            </div>
            {#if grillWanted && griller === null}
              <span class="muted hint-line">
                {!grillEngineChosen
                  ? "Pick an engine for the grill role, or the grilling will not start."
                  : planner
                    ? "The grilling starts as soon as the planner has proposed its cards and waits for you."
                    : proposals.length > 0
                      ? "The grilling starts in a moment: the cards are there, the planner is no longer running."
                      : "After planning, a grill session questions you about the cards."}
              </span>
            {/if}
          {/if}
        </div>
      {/if}

      <h3>Proposals {proposals.length > 0 ? `(${proposals.length})` : ""}</h3>
      {#if proposals.length === 0}
        <p class="hint">
          {planner ? "The planner has not proposed anything yet." : "No proposals waiting for your yes."}
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
                columnId={newColumnId}
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
              {#if changedByPlanner.includes(card.id)}<span class="status auto" title="The planner changed this proposal after creating it">changed by the planner</span>{/if}
            </div>
            <div class="fields">
              <label>
                Role
                <select
                  value={card.agent ?? ""}
                  aria-label="Role of {proposalTitle(card)}"
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
                Level
                <select
                  value={card.tier ?? ""}
                  aria-label="Level of {proposalTitle(card)}"
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
              <button class="ax-btn" type="button" disabled={busy} onclick={() => void approveProposal(card)}>Accept</button>
              <button class="ax-btn" type="button" disabled={busy} onclick={() => (editing = card.id)}>Edit</button>
              {#if discarding === card.id}
                <button class="ax-btn danger" type="button" disabled={busy} onclick={() => void discardProposal(card)}>Wirklich verwerfen</button>
                <button class="ax-btn" type="button" onclick={() => (discarding = null)}>Cancel</button>
              {:else}
                <button class="ax-btn" type="button" disabled={busy} onclick={() => (discarding = card.id)}>Discard</button>
              {/if}
            </div>
          </li>
          {/if}
        {/each}
      </ul>

      {#if plan.status !== "closed"}
        {#if editing === "new"}
          <ProposalEditor
            card={null}
            {plan}
            planCards={allCardsOfPlan(data?.cards ?? [], plan.id)}
            columnId={newColumnId}
            {roles}
            {freshCard}
            onSaved={reload}
            onCancel={() => (editing = null)}
          />
        {:else}
          <div class="row">
            <button class="ax-btn" type="button" disabled={busy || newColumnId === null} onclick={() => (editing = "new")}>
              {plan.status === "draft" ? "Add card" : "Add card to the running plan"}
            </button>
          </div>
        {/if}
      {/if}

      {#if plan.status === "draft"}
        <div class="approve">
          <button class="ax-btn primary" type="button" disabled={busy || !canApprove(plan, cards)} onclick={() => void approvePlan()}>
            Release plan
          </button>
          <label class="check">
            <input type="checkbox" bind:checked={runByItself} />
            Automatisch abarbeiten
          </label>
          <span class="muted">
            {runByItself
              ? "Cards start by themselves as soon as their predecessors are done, as many at once as possible; the review runs by itself."
              : "The proposals move to the first open column; you start every card yourself."}
            Der Planer ist danach fertig.
          </span>
          {#if !grilled}
            <span class="muted hint-line">
              This plan was not grilled: its goal has not been questioned yet. You can still release it.
            </span>
          {/if}
        </div>
      {/if}

      {#if spend}
        <div class="approve">
          <span>Verbrauch: {spendLine(spend)}</span>
          {#if spend.plan_over}
            <span class="error" role="status">Pausiert: {spend.plan_over}.</span>
            <button class="ax-btn primary" type="button" disabled={busy} onclick={() => void resumePlan()}>Continue</button>
            <span class="muted">Sets the plan's limit to the spend so far plus a new allowance.</span>
          {/if}
          {#if spend.day_over}
            <span class="error" role="status">
              Daily limit of the Studio sessions reached: {spend.day_over}. Nothing new starts until tomorrow; raise it:
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
            Take over plan
          </button>
          <span class="muted">
            All cards are on the plan's line. The work goes into the project's main branch, its commits stay
            separate as long as the branch has not moved. Nothing is pushed; a conflict is rolled back.
          </span>
          {#if takeOverConflict.length > 0}
            <p class="error" role="alert">
              Conflict with the main branch, nothing was changed: {takeOverConflict.join(", ")}
            </p>
          {/if}
        </div>
      {/if}

      {#if working.length > 0}
        <h3>Cards of the plan</h3>
        <ul class="cards">
          {#each working as card (card.id)}
            {#if editing === card.id}
              <li class="editing">
                <ProposalEditor
                  {card}
                  {plan}
                  planCards={allCardsOfPlan(data?.cards ?? [], plan.id)}
                  columnId={newColumnId}
                  {roles}
                  {freshCard}
                  onSaved={reload}
                  onCancel={() => (editing = null)}
                />
              </li>
            {:else}
            <li>
              <span>#{card.id} {proposalTitle(card)}</span>
              <span class="muted">{card.agent ?? "—"} · {STATE_LABEL[card.state]}</span>
              {#if plan.status !== "closed" && cardEditable(card)}
                <button class="ax-btn" type="button" disabled={busy} onclick={() => (editing = card.id)}>Edit</button>
              {/if}
              {#if leftCards.includes(card.id)}
                <span class="error" role="status">Does not fit the plan twice; the Studio gave up.</span>
                <button class="ax-btn" type="button" disabled={busy} onclick={() => void integrateAgain(card)}>Erneut integrieren</button>
                <button class="ax-btn" type="button" disabled={busy} onclick={() => void redo(card)}>Neu machen</button>
              {/if}
            </li>
            {/if}
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
    grid-template-columns: calc(340px * var(--ax-ui-scale)) 1fr;
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
    gap: var(--ax-space-2);
  }
  .plans li button {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .plans li button:hover {
    background: var(--ax-surface-3);
  }
  .plans li .head {
    display: flex;
    justify-content: space-between;
    gap: var(--ax-space-2);
    min-width: 0;
  }
  .plans li .goal-line {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .plans li button.on {
    background: var(--ax-accent-muted);
    border-color: var(--ax-accent);
  }
  .grow {
    flex: 1;
  }
  .start {
    display: flex;
    align-items: flex-end;
    gap: var(--ax-space-2);
    flex-wrap: wrap;
  }
  .hint-line {
    flex-basis: 100%;
  }
  .suggestion {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }
  .suggestion h3 {
    margin: 0;
  }
  .proposed-goal {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: calc(260px * var(--ax-ui-scale));
    overflow-y: auto;
    padding: var(--ax-space-2);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font: inherit;
    font-size: var(--ax-font-size-sm);
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
