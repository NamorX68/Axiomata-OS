<!--
  The Agents panel of the activity rail (`docs/plans/agents-model.md`, AM4, Q16–Q18), in four parts: the master tiles;
  the owner's favorites, one click starts one; "Start an agent" (role → light · medium · heavy → engine → Start,
  `AgentStart`); and the sessions — only those running or waiting, and the ended ones whose work is not taken over yet
  ("Waiting for you: take over or discard"). An ended session without work of its own is not listed; the weekly
  clean-up removes it. It is a view of the sidebar column, like Files, and the same in every mode of the Studio.
  An agent is a role at a tier on an engine; its session is named "Glut Medium Allrounder (Sonnet 5.5)" (Q8). The edit
  form never describes a harness, a command or a model — engines are made only in the Models & roles pane.
  Deleting a profile does not close the panes showing it — those may have something running, and the pane
  says the profile is gone instead of taking a live shell with it. The wording here says as much.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { AgentSpec, Harness, IdeAgent } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { projectRoles, type Role } from "../core/roster";
  import { toast } from "../core/toast";
  import AgentStart from "./AgentStart.svelte";
  import {
    agentsWithWork,
    isFavorite,
    listFavorites,
    sessionLines,
    sessionSections,
    startAgentChoice,
    toggleFavorite,
    type Favorite,
    type FavoriteView,
  } from "./agentStart";
  import { agentUnsavedWork, specOf, specReady } from "./agents";
  import { removalQuestion } from "./agentRemoval";
  import { agentStatus } from "./agentStatus";
  import { cardOf } from "./agentCard";
  import ProjectRolesNotice from "./ProjectRolesNotice.svelte";
  import RoleChip from "./RoleChip.svelte";
  import { engineName, harnessBadge, harnessLabel } from "./rosterLook";
  import { engineCatalog, engineLine, refreshEngines, rosterVersion } from "./rosterStore";
  import StatusDot from "./StatusDot.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import HexAvatar from "../ui/HexAvatar.svelte";
  import { ring, type RingState } from "../ui/glow";

  const statuses = agentStatus.statuses;

  let {
    agents,
    openAgentIds,
    disabled = false,
    projectId = null,
    onManage,
    onOpen,
    onMaster,
    onStarted,
    onEdit,
    onRemove,
  }: {
    agents: IdeAgent[];
    /** The agents that have a pane open somewhere (either layout): the others read as closed. */
    openAgentIds: ReadonlySet<number>;
    /** No project open — there is nothing an agent could belong to. */
    disabled?: boolean;
    /** The open project, for the roles it brings (`ProjectRolesNotice`). */
    projectId?: number | null;
    /** Opens the Models & roles pane: where engines and roles are made. */
    onManage: () => void;
    onOpen: (agent: IdeAgent) => void;
    /** Opens the project's master pane on an engine: as the owner uses it outside, in the real folder (CP3). */
    onMaster: (harness: Harness) => void;
    /** A session was started (from a favorite or "Start an agent"): the agent list is read again and its pane opens. */
    onStarted: (agent: IdeAgent) => void;
    onEdit: (id: number, spec: AgentSpec) => void;
    /** `force`: the owner agreed to throw unsaved work in the worktree away. */
    onRemove: (id: number, force: boolean) => void;
  } = $props();

  /** Asks before an agent goes — once more when its worktree holds unsaved work (A3c) — and only then removes it. */
  async function askRemove(id: number, question: string) {
    // Unknown counts as none: the backend then refuses on its own if there is some, and nothing is lost.
    const unsaved = await agentUnsavedWork(id).catch(() => false);
    if (confirm(removalQuestion(question, unsaved))) onRemove(id, unsaved);
  }

  /** A number = editing that agent; `undefined` = no form open. */
  let editing = $state<number | undefined>(undefined);
  /** The roles in force for this project — what "Start an agent" and the edit form offer. */
  let roleNames = $state<string[]>([]);
  let roleList = $state<Role[]>([]);
  const roleKinds = $derived(new Map(roleList.map((role) => [role.name, role.kind])));

  let form = $state<AgentSpec>({ name: "", engine_id: "", role: "" });
  let favorites = $state<FavoriteView[]>([]);
  /** The sessions holding work not taken over, read from git when the agents change. */
  let withWork = $state<Set<number>>(new Set());

  const engines = $derived($engineCatalog);

  /** The engines a project has a master pane for, in the order the tiles show them. */
  const MASTER_HARNESSES = ["glut", "claude_code", "opencode"] as const satisfies readonly Harness[];
  /** The project's first master on each engine: the engine's tile at the top, which opens it or starts one more. */
  const masters = $derived(
    new Map(
      agents
        .filter((a) => a.agent_role === "master")
        .sort((a, b) => b.id - a.id)
        .map((a) => [a.harness, a]),
    ),
  );
  const tiled = $derived(new Set([...masters.values()].map((a) => a.id)));
  /**
   * The further masters (`master-glut-2`, …, started by a click on an engine's tile while its master is open) head the
   * list of agents, newest first: the owner just opened them and looks for them there.
   */
  const sections = $derived(
    sessionSections(
      agents,
      agents.filter((a) => a.agent_role === "master" && !tiled.has(a.id)).sort((a, b) => b.id - a.id),
      (a) => openAgentIds.has(a.id),
      (a) => $statuses.byAgent.get(a.id)?.state,
      withWork,
    ),
  );

  async function loadRoles(id: number | null) {
    if (id === null) {
      roleNames = [];
      roleList = [];
      favorites = [];
      return;
    }
    try {
      const roles = await projectRoles(id);
      if (id === projectId) {
        roleNames = roles.effective.map((role) => role.name);
        roleList = roles.effective;
      }
    } catch {
      roleNames = [];
      roleList = [];
    }
    await loadFavorites(id);
  }

  async function loadFavorites(id: number | null = projectId) {
    if (id === null) return;
    try {
      const views = await listFavorites(id);
      if (id === projectId) favorites = views;
    } catch {
      favorites = [];
    }
  }

  /** Counts reads of the work, so a slow answer for an older list never overwrites a newer one. */
  let workRead = 0;
  /**
   * The sessions that could be "waiting for you": no pane open, harness ended, idle or never seen. When one joins or
   * leaves them — a session ends with new work — the work is read again; a change between working and waiting is not.
   */
  const settledKey = $derived(
    agents
      .filter((a) => {
        const state = $statuses.byAgent.get(a.id)?.state;
        return !openAgentIds.has(a.id) || state === "ended" || state === "idle" || state === undefined;
      })
      .map((a) => `${a.id}:${$statuses.byAgent.get(a.id)?.state ?? "-"}`)
      .join(","),
  );
  $effect(() => {
    const id = projectId;
    void settledKey;
    const mine = ++workRead;
    if (id === null) return;
    const all = agents.map((a) => a.id);
    void agentsWithWork(id)
      .then((ids) => {
        if (mine === workRead) withWork = new Set(ids);
      })
      // Unjudgeable counts as work: every ended session is shown rather than hidden.
      .catch(() => {
        if (mine === workRead) withWork = new Set(all);
      });
  });

  /** A favorite's tile: one click starts it — on its engine, or on the tier's default resolved now. */
  async function startFavorite(view: FavoriteView) {
    if (projectId === null || view.unavailable) return;
    try {
      onStarted(await startAgentChoice(projectId, view.role, view.tier, view.engine ?? null));
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  async function toggleStar(favorite: Favorite) {
    try {
      await toggleFavorite(favorite);
      await loadFavorites();
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  /** A session's choice as a favorite: its role, the tier its engine has (else its role's), its engine. */
  function favoriteOf(agent: IdeAgent): Favorite | null {
    const engine = engines.find((e) => e.id === agent.engine_id);
    const tier = engine?.rated_tier ?? roleList.find((r) => r.name === agent.agent_role)?.tier;
    if (!tier || !agent.engine_id) return null;
    return { role: agent.agent_role, tier, engine: agent.engine_id };
  }

  onMount(() => {
    void refreshEngines();
  });

  // The roles are made in another pane: read them again when one was saved or deleted there.
  $effect(() => {
    void $rosterVersion;
    void loadRoles(projectId);
  });

  function startEdit(agent: IdeAgent) {
    editing = agent.id;
    form = specOf(agent);
    void refreshEngines();
  }

  function submit() {
    if (!specReady(form) || editing === undefined) return;
    onEdit(editing, { ...form, name: form.name.trim() });
    editing = undefined;
  }

  /** A session's two lines (Q8), the second after the engine it runs on now; `null` for an older name. */
  const linesOf = (agent: IdeAgent) =>
    sessionLines(
      agent.name,
      engines.find((e) => e.id === agent.engine_id),
    );

  /** The ring of an agent tile (G11): working or waiting for the owner; anything else has none. */
  function ringOf(tone: string | undefined): RingState {
    return tone === "working" || tone === "waiting" ? tone : null;
  }
</script>

<div class="panel">
  <SectionHead label="Agents" rule level={2} />
  <div class="body">
    {#if !disabled}
      <!-- The engines as the owner uses them outside: own settings and rules, the real folder, the team tools added. -->
      <div class="masters" role="group" aria-label="Master panes">
        {#each MASTER_HARNESSES as harness (harness)}
          {@const agent = masters.get(harness)}
          {@const card = agent
            ? cardOf(agent, $statuses.byAgent.get(agent.id), openAgentIds.has(agent.id), $statuses.checkedAt)
            : null}
          <div class="master-tile" style:--harness="var(--ax-harness-{harness})">
          <button
            class="master {card?.status.tone ?? ''}"
            use:ring={ringOf(card?.status.tone)}
            type="button"
            title={harness === "opencode"
              ? "Opencode as you run it yourself — your settings, your models, in the project folder; no team tools"
              : `${harnessLabel(harness)} as you use it outside — your settings and rules, in the project folder, ` +
                "with the team tools. Click again while it is open for one more beside it"}
            onclick={() => onMaster(harness)}
          >
            <HexAvatar
              text={harnessBadge(harness)}
              live={card?.status.tone === "working"}
              ring="var(--harness)"
              size="lg"
              label={harnessLabel(harness)}
            />
            <span class="master-name">{harnessLabel(harness)}</span>
            <span class="master-kind">Master</span>
            {#if card}
              <span class="state"><StatusDot view={card.status} /> {card.status.label}</span>
            {/if}
          </button>
          {#if agent}
            <!-- Removing a master forgets its session; the next click starts a fresh one. -->
            <div class="row-actions">
              <IconButton
                icon="trash-2"
                label="Remove the {harnessLabel(harness)} master"
                size="sm"
                onclick={() =>
                  void askRemove(agent.id, `Remove the ${harnessLabel(harness)} master? Panes already running it stay open.`)}
              />
            </div>
          {/if}
          </div>
        {/each}
      </div>
    {/if}
    <ProjectRolesNotice {projectId} />
    {#if !disabled && projectId !== null && editing === undefined}
      {#if favorites.length > 0}
        <section class="favorites" aria-label="Favorites">
          <SectionHead label="Favorites" count={favorites.length} />
          <div class="fav-tiles">
            {#each favorites as f (`${f.role}/${f.tier}/${f.engine ?? ""}`)}
              <div class="fav-tile" class:unavailable={f.unavailable !== null}>
                <button
                  type="button"
                  class="fav"
                  disabled={f.unavailable !== null}
                  title={f.unavailable ?? `Start ${f.name}`}
                  onclick={() => void startFavorite(f)}
                >
                  <span class="fav-title">{f.title}</span>
                  <span class="fav-sub">
                    {f.unavailable ? "unavailable" : f.subtitle}{f.engine ? "" : " · tier default"}
                  </span>
                </button>
                <div class="row-actions">
                  <IconButton
                    icon="star"
                    size="sm"
                    pressed
                    label="Remove {f.name} from the favorites"
                    onclick={() => void toggleStar(f)}
                  />
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/if}
      {#if roleList.length > 0}
        <AgentStart
          roles={roleList}
          {projectId}
          {favorites}
          {onStarted}
          onFavorites={() => void loadFavorites()}
        />
      {/if}
    {/if}
    {#snippet sessionCard(agent: IdeAgent)}
          {@const card = cardOf(agent, $statuses.byAgent.get(agent.id), openAgentIds.has(agent.id), $statuses.checkedAt)}
          {@const lines = linesOf(agent)}
          <li
            class="card {card.status.tone}"
            style:--harness="var(--ax-harness-{agent.harness})"
            use:ring={ringOf(card.status.tone)}
          >
            <button class="open" type="button" {disabled} title={card.status.title} onclick={() => onOpen(agent)}>
              <span class="top">
                <HexAvatar
                  text={harnessBadge(agent.harness)}
                  live={card.status.tone === "working"}
                  ring="var(--harness)"
                  label={harnessLabel(agent.harness)}
                  title={engineName(agent, engines)}
                />
                {#if lines}
                  <!-- Named by `agent_lines` (Q8): the title takes the card's width, the subtitle says harness and
                       model and ends in the state; a role chip and an engine line would only say the same again. -->
                  <span class="who">
                    <span class="name" title={agent.name}>{lines.title}</span>
                    <span class="subname">
                      <span class="subtext">{lines.subtitle}</span>
                      <span class="state small"><StatusDot view={card.status} /> {card.status.label}</span>
                    </span>
                  </span>
                {:else}
                  <span class="who">
                    <span class="name">{agent.name}</span>
                    <span class="meta">
                      <RoleChip label={agent.agent_role} kind={roleKinds.get(agent.agent_role)} />
                      <span class="engine">{engineName(agent, engines)}</span>
                    </span>
                  </span>
                  <span class="state">
                    <StatusDot view={card.status} />
                    {card.status.label}
                  </span>
                {/if}
              </span>
              {#if agent.branch}
                <!-- Where the work sits: the branch checked out in this agent's worktree. -->
                <span class="branchline" title="Checked out in its worktree">{agent.branch}</span>
              {/if}
              {#if card.plan || card.step}
                <span class="doing">{card.step ?? card.plan}</span>
              {/if}
              {#if card.progress}
                <span class="progress" title="{card.progress.done} of {card.progress.total} steps done">
                  <span class="bar"><span class="fill" style:width="{(card.progress.done / card.progress.total) * 100}%"></span></span>
                  <span class="count">{card.progress.done}/{card.progress.total}</span>
                </span>
              {/if}
              {#if card.since}<span class="since">{card.status.tone === "ended" ? "last" : card.status.label} · {card.since}</span>{/if}
            </button>
            <!-- Quiet until the card is hovered or focused (editor-look I4). -->
            <div class="row-actions">
              {#if favoriteOf(agent)}
                {@const fav = favoriteOf(agent)}
                {#if fav}
                  <IconButton
                    icon="star"
                    size="sm"
                    pressed={isFavorite(favorites, fav)}
                    label={isFavorite(favorites, fav)
                      ? "Remove this agent from the favorites"
                      : "Keep this agent as a favorite"}
                    onclick={() => void toggleStar(fav)}
                  />
                {/if}
              {/if}
              {#if agent.agent_role !== "master"}
                <!-- A master keeps its engine and role (`roster::update_agent_on_engine` refuses). -->
                <IconButton icon="pencil" label="Edit {agent.name}" size="sm" onclick={() => startEdit(agent)} />
              {/if}
              <IconButton
                icon="trash-2"
                label="Remove {agent.name}"
                size="sm"
                onclick={() =>
                  void askRemove(
                    agent.id,
                    `Remove the agent “${agent.name}”? Its worktree and branch go with it; panes already running it stay open.`,
                  )}
              />
            </div>
          </li>
    {/snippet}
    {#if sections.active.length > 0}
      <SectionHead label="Sessions" count={sections.active.length} />
      <ul class="cards">
        {#each sections.active as agent (agent.id)}{@render sessionCard(agent)}{/each}
      </ul>
    {/if}
    {#if sections.waiting.length > 0}
      <SectionHead label="Waiting for you: take over or discard" count={sections.waiting.length} tone="warn" />
      <ul class="cards">
        {#each sections.waiting as agent (agent.id)}{@render sessionCard(agent)}{/each}
      </ul>
    {/if}
    {#if sections.active.length === 0 && sections.waiting.length === 0}
      <p class="empty">{disabled ? "Open a project to work with agents." : "No session runs or waits for you."}</p>
    {/if}
    {#if editing === undefined}
      <button class="add" type="button" onclick={onManage}><Icon name="settings" size="sm" /> Models &amp; roles…</button>
    {:else}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <p class="label">Edit agent</p>
        <input type="text" bind:value={form.name} placeholder="Name" spellcheck="false" />
        <select bind:value={form.role} aria-label="Role">
          {#each roleNames as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <select bind:value={form.engine_id} aria-label="Engine">
          <option value="" disabled>Engine…</option>
          {#each engines as engine (engine.id)}
            <option value={engine.id} title={engineLine(engine)}>{engine.label}</option>
          {/each}
        </select>
        <p class="hint">
          Another role or engine for this session only. Models, engines and roles are made and changed under
          <button type="button" class="link" onclick={onManage}>Models &amp; roles</button>.
        </p>
        <div class="form-actions">
          <button type="submit" class="ax-btn primary" disabled={!specReady(form)}>Save</button>
          <button type="button" class="ax-btn" onclick={() => (editing = undefined)}>Cancel</button>
        </div>
      </form>
    {/if}
  </div>
</div>

<style>
  /* The harness's light at the top of an agent tile (docs/plans/glass.md, G1): a soft glow from the top edge in the
     harness's colour, instead of the hard bar it was. */
  .master,
  .card {
    --harness-light: radial-gradient(
      120% 70% at 50% 0%,
      color-mix(in srgb, var(--harness) 16%, transparent),
      transparent 70%
    );
  }

  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
  }

  .link {
    padding: 0;
    background: none;
    border: 0;
    color: var(--ax-accent);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }

  .add:disabled {
    opacity: 0.5;
    cursor: default;
  }

  ul {
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
  }

  .cards {
    gap: var(--ax-space-3);
  }

  /* A card per agent (UX6b): a raised plate with the light edge; the harness's colour lives in the avatar's ring, and
     a card whose agent works glows and breathes — the accent rule's "alive". */
  .card {
    position: relative;
    display: block;
    padding: 0;
    border: 1px solid var(--ax-border);
    border-top-color: color-mix(in srgb, var(--harness) 55%, transparent);
    border-radius: var(--ax-radius-md);
    background: var(--harness-light), var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  .card:hover,
  .card:focus-within {
    border-color: var(--ax-border-strong);
    background: var(--harness-light), var(--ax-elev-3);
  }

  .card.ended {
    opacity: 0.75;
  }

  .open {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-3);
    background: none;
    border: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .open:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .top {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
  }

  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--ax-space-2);
    min-width: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .engine {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The master panes first and larger than the agents' cards: the engines as the owner works with them every day. The
     harness's colour is the tile's edge; only a working master glows (the accent rule). */
  .masters {
    display: grid;
    /* Two a row, the owner's order: Glut and Claude Code side by side, Opencode below. */
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--ax-space-3);
    margin-bottom: var(--ax-space-4);
  }

  .master-tile {
    position: relative;
    display: flex;
    min-width: 0;
  }

  .master-tile:hover .row-actions,
  .master-tile:focus-within .row-actions {
    opacity: 1;
  }

  .master {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-1);
    min-width: 0;
    padding: var(--ax-space-4) var(--ax-space-2) var(--ax-space-3);
    border: 1px solid color-mix(in srgb, var(--harness) 45%, var(--ax-border));
    border-top-color: color-mix(in srgb, var(--harness) 55%, transparent);
    border-radius: var(--ax-radius-md);
    background: var(--harness-light), var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    cursor: pointer;
  }

  .master:hover,
  .master:focus-visible {
    border-color: var(--harness);
    background: var(--harness-light), var(--ax-elev-3);
  }

  .master-name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--ax-font-size-base);
    font-weight: 600;
  }

  .master-kind {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* The owner's favorites (Q18): a small tile each, one click starts it; an unavailable one is dimmed, not hidden. */
  .favorites {
    margin-bottom: var(--ax-space-3);
  }

  .fav-tiles {
    display: grid;
    /* As many as the column takes without cutting the names: one in a narrow sidebar. */
    grid-template-columns: repeat(auto-fill, minmax(calc(170px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-2);
  }

  .fav-tile {
    position: relative;
    display: flex;
    min-width: 0;
  }

  .fav-tile:hover .row-actions,
  .fav-tile:focus-within .row-actions {
    opacity: 1;
  }

  .fav {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: calc(2px * var(--ax-ui-scale));
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    text-align: left;
    cursor: pointer;
  }

  .fav:hover:not(:disabled) {
    border-color: var(--ax-border-strong);
    background: var(--ax-surface-3);
  }

  .fav-tile.unavailable .fav {
    opacity: 0.55;
    cursor: default;
  }

  .fav-title,
  .fav-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fav-title {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }

  .fav-sub,
  .subname {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .subname {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
  }

  .subtext {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state.small {
    flex: 0 0 auto;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* Where the work sits: the branch checked out in this agent's worktree. */
  .branchline {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }

  .doing {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    color: var(--ax-text);
  }

  .progress {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .bar {
    flex: 1;
    height: calc(4px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--ax-accent);
  }

  .count,
  .since {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .row-actions {
    position: absolute;
    right: var(--ax-space-1);
    bottom: var(--ax-space-1);
    display: flex;
    gap: var(--ax-space-1);
    opacity: 0;
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-md);
  }

  .card:hover .row-actions,
  .card:focus-within .row-actions {
    opacity: 1;
  }

  .empty {
    margin: 0 0 var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding-top: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: var(--ax-line-height);
  }

  .label {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
  }

  .form-actions {
    display: flex;
    gap: var(--ax-space-2);
  }

  input,
  select {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    resize: vertical;
  }

  input:focus-visible,
  select:focus-visible {
    outline: var(--ax-focus-ring);
  }

  /* "New agent…" and "Models & roles…": rows of their own, like the agents above them. */
  .add {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .add:hover {
    background: var(--ax-surface-2);
    color: var(--ax-text);
  }
</style>
