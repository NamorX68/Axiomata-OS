<!--
  The Agents panel of the activity rail: which agents this project has (with their live status), the form to
  add or edit one, and the click that puts one into a pane. It is a view of the sidebar column, like Files.
  An agent is *a role on an engine*: the role's "Start" chip opens the agent of that role that has no pane, or makes
  one on the role's engine (`docs/plans/studio-roster-ui.md`, R-3); the form is for something quite different. The
  form never describes a harness, a command or a model — engines are made only in the Models & roles pane
  (`EnginesSection`).
  Deleting a profile does not close the panes showing it — those may have something running, and the pane
  says the profile is gone instead of taking a live shell with it. The wording here says as much.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { AgentSpec, Harness, IdeAgent } from "../core/backend";
  import { projectRoles, type Role } from "../core/roster";
  import { agentUnsavedWork, blankSpec, specOf, specReady } from "./agents";
  import { removalQuestion } from "./agentRemoval";
  import { agentStatus } from "./agentStatus";
  import { cardOf } from "./agentCard";
  import ProjectRolesNotice from "./ProjectRolesNotice.svelte";
  import RoleChip from "./RoleChip.svelte";
  import {
    agentNameFor,
    engineName,
    groupRoles,
    harnessBadge,
    harnessLabel,
    roleFamilies,
    startableAgent,
    variantLabel,
  } from "./rosterLook";
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
    onCreate,
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
    /** May answer when the agent exists, so a second click on a chip waits for the first. */
    onCreate: (spec: AgentSpec) => void | Promise<unknown>;
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

  /** `null` = the "new agent" form, a number = editing that agent. */
  let editing = $state<number | null | undefined>(undefined);
  /** The roles in force for this project — what the role picker offers — and the engine each carries. */
  let roleNames = $state<string[]>([]);
  let roleEngines = $state<Record<string, string | null>>({});
  let roleList = $state<Role[]>([]);
  const roleKinds = $derived(new Map(roleList.map((role) => [role.name, role.kind])));
  const roleGroups = $derived(groupRoles(roleList));

  let form = $state<AgentSpec>(blankSpec([]));

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
  const listed = $derived([
    ...agents.filter((a) => a.agent_role === "master" && !tiled.has(a.id)).sort((a, b) => b.id - a.id),
    ...agents.filter((a) => a.agent_role !== "master"),
  ]);

  async function loadRoles(id: number | null) {
    if (id === null) {
      roleNames = [];
      roleEngines = {};
      roleList = [];
      return;
    }
    try {
      const roles = await projectRoles(id);
      if (id === projectId) {
        roleNames = roles.effective.map((role) => role.name);
        roleEngines = Object.fromEntries(roles.effective.map((role) => [role.name, role.engine]));
        roleList = roles.effective;
      }
    } catch {
      roleNames = [];
      roleEngines = {};
      roleList = [];
    }
  }

  onMount(() => {
    void refreshEngines();
  });

  // The roles are made in another pane: read them again when one was saved or deleted there.
  $effect(() => {
    void $rosterVersion;
    void loadRoles(projectId);
  });

  function startNew() {
    editing = null;
    form = blankSpec(roleNames);
    // The role carries its engine: starting from the role, the engine is already there. Without one, and with a single
    // engine in the catalog, there is exactly one sensible choice: do not make the user pick it.
    adoptEngineOfRole();
    if (form.engine_id === "" && engines.length === 1) form.engine_id = engines[0].id;
    // A catalog edited in the settings since the panel was drawn: look again.
    void refreshEngines();
  }

  /** The engine the picked role carries, when the catalog still has it; otherwise the choice stays as it is. */
  function adoptEngineOfRole() {
    const own = roleEngines[form.role];
    if (own && engines.some((engine) => engine.id === own)) form.engine_id = own;
  }

  function startEdit(agent: IdeAgent) {
    editing = agent.id;
    form = specOf(agent);
    void refreshEngines();
  }

  function submit() {
    if (!specReady(form)) return;
    if (editing === null) onCreate({ ...form, name: form.name.trim() });
    else if (typeof editing === "number") onEdit(editing, { ...form, name: form.name.trim() });
    editing = undefined;
  }

  /**
   * A role's "Start" chip: the agent of that role with no pane opens again (its session continues in its worktree);
   * with none, one is made on the role's engine. A role that names no engine (the planner) opens the form with the role
   * chosen, because the engine is the owner's pick at every start.
   */
  /** The family of roles whose models are shown under its group (`kind/base`), `null` = none. */
  let openFamily = $state<string | null>(null);

  function toggleFamily(key: string) {
    openFamily = openFamily === key ? null : key;
  }

  /** A "Start" click is being served: the agent it makes is not in `agents` yet, so a second click would repeat it. */
  let starting = false;

  async function startRole(role: Role) {
    if (starting) return;
    const again = startableAgent(agents, role, openAgentIds);
    if (again) {
      onOpen(again);
      return;
    }
    if (role.engine && engines.some((engine) => engine.id === role.engine)) {
      starting = true;
      try {
        await onCreate({
          name: agentNameFor(role.name, agents.map((a) => a.name)),
          engine_id: role.engine,
          role: role.name,
        });
      } finally {
        starting = false;
      }
      return;
    }
    startNew();
    form.role = role.name;
    form.name = agentNameFor(role.name, agents.map((a) => a.name));
  }

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
    {#if roleList.length > 0 && !disabled && editing === undefined}
      <div class="starts" role="group" aria-label="Start an agent in a role">
        <span class="starts-label">Start an agent</span>
        {#each roleGroups as group (group.kind)}
          {@const families = roleFamilies(group.roles)}
          {@const open = families.find((family) => `${group.kind}/${family.base}` === openFamily)}
          <span class="group-title">{group.title}</span>
          <div class="chips">
            {#each families as family (family.base)}
              {#if family.roles.length === 1}
                <button
                  type="button"
                  class="start"
                  title={family.roles[0].description || `Start ${family.roles[0].name}`}
                  onclick={() => startRole(family.roles[0])}
                >
                  <RoleChip label={family.roles[0].name} kind={family.roles[0].kind} />
                </button>
              {:else}
                <!-- One job on several models: the chip opens the models under the group, the click there starts. -->
                <button
                  type="button"
                  class="start family"
                  class:expanded={open === family}
                  aria-expanded={open === family}
                  title="{family.base} on {family.roles.length} models"
                  onclick={() => toggleFamily(`${group.kind}/${family.base}`)}
                >
                  <RoleChip label={family.base} kind={group.kind} />
                  <span class="caret"><Icon name="chevron-down" size="sm" /></span>
                </button>
              {/if}
            {/each}
            {#if open}
              <div class="variants" role="group" aria-label="{open.base} on a model">
                {#each open.roles as role (role.name)}
                  <button
                    type="button"
                    class="variant"
                    title={role.description || `Start ${role.name}`}
                    onclick={() => {
                      openFamily = null;
                      void startRole(role);
                    }}>{variantLabel(role, open.base, engines)}</button
                  >
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
    {#if listed.length > 0}
      <ul class="cards">
        {#each listed as agent (agent.id)}
          {@const card = cardOf(agent, $statuses.byAgent.get(agent.id), openAgentIds.has(agent.id), $statuses.checkedAt)}
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
        {/each}
      </ul>
    {:else}
      <p class="empty">{disabled ? "Open a project to work with agents." : "No agents in this project yet."}</p>
    {/if}
    {#if editing === undefined}
      <button class="add" type="button" disabled={disabled || engines.length === 0} onclick={startNew}>
        <Icon name="plus" size="sm" /> New agent…
      </button>
      {#if engines.length === 0 && !disabled}
        <p class="hint">No engines yet — an agent runs on an engine. Add one under Models &amp; roles.</p>
      {/if}
      <button class="add" type="button" onclick={onManage}><Icon name="settings" size="sm" /> Models &amp; roles…</button>
    {:else}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <p class="label">{editing === null ? "New agent" : "Edit agent"}</p>
        <input type="text" bind:value={form.name} placeholder="Name" spellcheck="false" />
        <select bind:value={form.role} aria-label="Role" onchange={adoptEngineOfRole}>
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
          The role brings its engine; pick another one here only for this agent. Roles and engines are made and
          changed under <button type="button" class="link" onclick={onManage}>Models &amp; roles</button>.
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

  /* The roles of the project as chips: one click starts an agent in that role. */
  /* One line per kind of work: its title in a column of its own, its jobs beside it. */
  .starts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: var(--ax-space-2) var(--ax-space-3);
    margin-bottom: var(--ax-space-3);
  }

  .starts-label {
    grid-column: 1 / -1;
  }

  .family {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
  }

  .family.expanded {
    border-color: var(--ax-border-strong);
  }

  .caret {
    display: inline-flex;
    transition: transform var(--ax-dur-fast) var(--ax-ease);
  }

  .family.expanded .caret {
    transform: rotate(180deg);
  }

  /* The models of the open job, on a line of their own under its chips. */
  .variants {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
  }

  .variant {
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    background: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .variant:hover,
  .variant:focus-visible {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-3);
  }

  .group-title {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1) var(--ax-space-2);
  }

  .starts-label {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .start {
    padding: 0;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    cursor: pointer;
  }

  .start:hover,
  .start:focus-visible {
    border-color: var(--ax-border-strong);
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
