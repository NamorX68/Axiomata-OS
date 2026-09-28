<!--
  The agent menu beside the project menu: which agents this project has, a form
  to add or edit one, and the button that puts one into a pane.

  Deleting a profile does not close the panes showing it — those may have
  something running, and the pane says the profile is gone instead of taking a
  live shell with it. The wording here says as much.
-->
<script lang="ts">
  import type { AgentFields, IdeAgent } from "../core/backend";
  import { HARNESSES, blankFields, fieldsOf } from "./agents";
  import { agentStatus, describeStatus } from "./agentStatus";
  import StatusDot from "./StatusDot.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

  const statuses = agentStatus.statuses;

  let {
    agents,
    disabled = false,
    onOpen,
    onCreate,
    onEdit,
    onRemove,
  }: {
    agents: IdeAgent[];
    /** No project open — there is nothing an agent could belong to. */
    disabled?: boolean;
    onOpen: (agent: IdeAgent) => void;
    onCreate: (fields: AgentFields) => void;
    onEdit: (id: number, fields: AgentFields) => void;
    onRemove: (id: number) => void;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLElement | undefined>();
  /** `null` = the "new agent" form, a number = editing that agent. */
  let editing = $state<number | null | undefined>(undefined);

  // A menu opens fresh: a form left open when it closed (a click elsewhere) is folded again.
  $effect(() => {
    if (!open) editing = undefined;
  });
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

<svelte:window
  onclick={(event) => {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }}
/>

<div class="picker" bind:this={root}>
  <button class="current" type="button" {disabled} aria-expanded={open} onclick={() => (open = !open)}>
    Agents<span class="count">{agents.length}</span>
    <span class="caret" aria-hidden="true"><Icon name="chevron-down" size="sm" /></span>
  </button>

  {#if open}
    <div class="menu">
      {#if agents.length > 0}
        <ul>
          {#each agents as agent (agent.id)}
            <li>
              <button
                class="pick"
                type="button"
                onclick={() => {
                  onOpen(agent);
                  open = false;
                }}
              >
                <span class="name">
                  <StatusDot view={describeStatus($statuses.byAgent.get(agent.id), agent, $statuses.checkedAt)} />
                  {agent.name}
                </span>
                <span class="meta">{agent.harness}{agent.model ? ` · ${agent.model}` : ""}</span>
              </button>
              <!-- Quiet until the row is hovered or focused (editor-look I4); the command is in the edit form. -->
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
        <p class="empty">No agents in this project yet.</p>
      {/if}

      {#if editing === undefined}
        <button class="add" type="button" onclick={startNew}><Icon name="plus" size="sm" /> New agent…</button>
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
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }

  .current {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .current:hover:not(:disabled) {
    border-color: var(--ax-accent);
  }

  .current:disabled {
    opacity: var(--ax-tile-glass-opacity);
    cursor: default;
  }

  .count {
    padding: 0 var(--ax-space-2);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .caret {
    color: var(--ax-text-muted);
  }

  .menu {
    position: absolute;
    top: calc(100% + var(--ax-space-2));
    left: 0;
    z-index: 2;
    width: calc(416px * var(--ax-ui-scale));
    max-height: 70vh;
    overflow-y: auto;
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  ul {
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
  }

  /* A row per agent (editor-look I4): the pick on the left, its actions on the right on hover. */
  li {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-md);
  }

  li:hover,
  li:focus-within {
    background: var(--ax-surface-2);
  }

  .pick {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
    padding: 0;
    background: none;
    border: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .name {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .meta {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .row-actions {
    display: flex;
    gap: var(--ax-space-1);
    opacity: 0;
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
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
