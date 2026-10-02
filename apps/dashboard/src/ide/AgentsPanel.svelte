<!--
  The Agents panel of the activity rail: which agents this project has (with their live status), the form to
  add or edit one, and the click that puts one into a pane. It is a view of the sidebar column, like Files.
  Deleting a profile does not close the panes showing it — those may have something running, and the pane
  says the profile is gone instead of taking a live shell with it. The wording here says as much.
-->
<script lang="ts">
  import type { AgentFields, IdeAgent } from "../core/backend";
  import { HARNESSES, blankFields, fieldsOf } from "./agents";
  import { agentStatus } from "./agentStatus";
  import { cardOf } from "./agentCard";
  import StatusDot from "./StatusDot.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

  const statuses = agentStatus.statuses;

  let {
    agents,
    openAgentIds,
    disabled = false,
    onOpen,
    onCreate,
    onEdit,
    onRemove,
  }: {
    agents: IdeAgent[];
    /** The agents that have a pane open somewhere (either layout): the others read as closed. */
    openAgentIds: ReadonlySet<number>;
    /** No project open — there is nothing an agent could belong to. */
    disabled?: boolean;
    onOpen: (agent: IdeAgent) => void;
    onCreate: (fields: AgentFields) => void;
    onEdit: (id: number, fields: AgentFields) => void;
    onRemove: (id: number) => void;
  } = $props();

  /** `null` = the "new agent" form, a number = editing that agent. */
  let editing = $state<number | null | undefined>(undefined);

  let form = $state<AgentFields>(blankFields());

  function startNew() {
    editing = null;
    form = blankFields();
  }

  function startEdit(agent: IdeAgent) {
    editing = agent.id;
    form = fieldsOf(agent);
  }

  function submit() {
    if (!form.name.trim()) return;
    if (editing === null) onCreate({ ...form });
    else if (typeof editing === "number") onEdit(editing, { ...form });
    editing = undefined;
  }
</script>

<div class="panel">
  <p class="title">AGENTS</p>
  <div class="body">
    {#if agents.length > 0}
      <ul class="cards">
        {#each agents as agent (agent.id)}
          {@const card = cardOf(agent, $statuses.byAgent.get(agent.id), openAgentIds.has(agent.id), $statuses.checkedAt)}
          <li class="card {card.status.tone}" style:--harness="var(--ax-harness-{agent.harness})">
            <button class="open" type="button" {disabled} title={card.status.title} onclick={() => onOpen(agent)}>
              <span class="top">
                <span class="avatar" aria-hidden="true">{agent.name.trim().charAt(0).toUpperCase() || "?"}</span>
                <span class="who">
                  <span class="name">{agent.name}</span>
                  <span class="meta">{agent.harness === "claude_code" ? "Claude Code" : agent.harness}{agent.model ? ` · ${agent.model}` : ""}</span>
                </span>
                <span class="state">
                  <StatusDot view={card.status} />
                  {card.status.label}
                </span>
              </span>
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
              <IconButton icon="pencil" label="Edit {agent.name}" size="sm" onclick={() => startEdit(agent)} />
              <IconButton
                icon="trash-2"
                label="Remove {agent.name}"
                size="sm"
                onclick={() => {
                  if (confirm(`Remove the agent “${agent.name}”? Panes already running it stay open.`)) {
                    onRemove(agent.id);
                  }
                }}
              />
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">{disabled ? "Open a project to work with agents." : "No agents in this project yet."}</p>
    {/if}
    {#if editing === undefined}
      <button class="add" type="button" {disabled} onclick={startNew}><Icon name="plus" size="sm" /> New agent…</button>
    {:else}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <p class="label">{editing === null ? "New agent" : "Edit agent"}</p>
        <input type="text" bind:value={form.name} placeholder="Name" spellcheck="false" />
        <select bind:value={form.harness}>
          {#each HARNESSES as harness (harness.id)}
            <option value={harness.id} title={harness.hint}>{harness.label}</option>
          {/each}
        </select>
        <input
          type="text"
          bind:value={form.command}
          placeholder="Command — empty runs the harness's own"
          spellcheck="false"
        />
        <input
          type="text"
          value={form.model ?? ""}
          oninput={(event) => (form.model = event.currentTarget.value.trim() || null)}
          placeholder="Model id, e.g. openrouter/deepseek/deepseek-v4-flash-0731"
          spellcheck="false"
        />
        <textarea bind:value={form.env} rows="2" placeholder="KEY=value per line" spellcheck="false"></textarea>
        <p class="hint">
          The model is passed as <code>--model</code>, so it must be the id the harness knows —
          <code>opencode models</code> lists them. A display name will not work. Left empty, the
          harness picks; with a command of your own, it is yours to pass.
        </p>
        <div class="form-actions">
          <button type="submit" class="ax-btn primary" disabled={!form.name.trim()}>Save</button>
          <button type="button" class="ax-btn" onclick={() => (editing = undefined)}>Cancel</button>
        </div>
      </form>
    {/if}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .title {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
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

  /* A card per agent: the harness's colour on its edge, the step it is on, how far it is. */
  .card {
    position: relative;
    display: block;
    padding: 0;
    border: 1px solid var(--ax-border);
    border-left: calc(3px * var(--ax-ui-scale)) solid var(--harness, var(--ax-border-strong));
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
  }

  .card:hover,
  .card:focus-within {
    border-color: var(--ax-border-strong);
    border-left-color: var(--harness, var(--ax-border-strong));
    background: var(--ax-surface-3);
  }

  .card.working {
    box-shadow: 0 0 0 1px var(--ax-accent) inset;
  }

  .card.waiting {
    box-shadow: 0 0 0 1px var(--ax-warning) inset;
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

  .avatar {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: calc(28px * var(--ax-ui-scale));
    height: calc(28px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--harness, var(--ax-text-muted)) 22%, transparent);
    color: var(--harness, var(--ax-text-muted));
    font-weight: 600;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
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
  select,
  textarea {
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
  select:focus-visible,
  textarea:focus-visible {
    outline: var(--ax-focus-ring);
  }

  /* "New agent…": a row of its own, like the agents above it. */
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
