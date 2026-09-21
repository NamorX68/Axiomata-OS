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
    <span class="caret" aria-hidden="true">▾</span>
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
                <span class="name">{agent.name}</span>
                <span class="meta">{agent.harness}{agent.model ? ` · ${agent.model}` : ""}</span>
                <code>{agent.effective_command}</code>
              </button>
              <div class="row-actions">
                <button type="button" onclick={() => startEdit(agent)}>Edit</button>
                <button
                  type="button"
                  class="danger"
                  onclick={() => {
                    if (confirm(`Remove the agent “${agent.name}”? Panes already running it stay open.`)) {
                      onRemove(agent.id);
                    }
                  }}>Remove</button
                >
              </div>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty">No agents in this project yet.</p>
      {/if}

      {#if editing === undefined}
        <button class="add" type="button" onclick={startNew}>New agent…</button>
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
            <button type="submit" disabled={!form.name.trim()}>Save</button>
            <button type="button" onclick={() => (editing = undefined)}>Cancel</button>
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
    width: 26rem;
    max-height: 70vh;
    overflow-y: auto;
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  ul {
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }

  li {
    padding: var(--ax-space-2);
    background: var(--ax-surface-2);
    border-radius: var(--ax-radius-sm);
  }

  .pick {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    width: 100%;
    padding: 0;
    background: none;
    border: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .meta {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  code {
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    word-break: break-all;
  }

  .row-actions {
    display: flex;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
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

  .row-actions button,
  .form-actions button,
  .add {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .row-actions button:hover,
  .form-actions button:hover:not(:disabled),
  .add:hover {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }

  .row-actions .danger:hover {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }

  .form-actions button:disabled {
    opacity: var(--ax-tile-glass-opacity);
    cursor: default;
  }
</style>
