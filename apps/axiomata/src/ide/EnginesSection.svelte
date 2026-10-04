<!--
  Engines (a2a.md CP-A1): the owner's catalog of harness + model + environment. Sessions and roles refer to an
  engine by id, so switching an engine happens here, once. Rust is authoritative for validation; `checkEngine`
  only points at a field before the round trip.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { messageOf } from "../core/errors";
  import {
    BILLINGS,
    blankEngine,
    checkEngine,
    deleteEngine,
    listEngines,
    saveEngine,
    type Engine,
    type EngineEntry,
  } from "../core/roster";
  import { toast } from "../core/toast";
  import { HARNESSES } from "./agents";

  /** Called after the catalog changed, so a sibling (the roles' engine picker) can reload. */
  let { onChange = () => {} }: { onChange?: () => void } = $props();

  let engines = $state<EngineEntry[]>([]);
  let loading = $state(true);
  /** `null` = no form open. `isNew` decides whether the id can still be typed. */
  let form = $state<Engine | null>(null);
  let isNew = $state(false);
  let problem = $state<{ field: string; message: string } | null>(null);
  let saving = $state(false);

  async function reload() {
    try {
      engines = await listEngines();
    } catch (err) {
      toast(`Could not load the engines: ${messageOf(err)}`, "warning");
    } finally {
      loading = false;
    }
  }

  function startNew() {
    form = blankEngine();
    isNew = true;
    problem = null;
  }

  function startEdit(entry: EngineEntry) {
    const { sessions: _sessions, ...engine } = entry;
    form = { ...engine };
    isNew = false;
    problem = null;
  }

  async function save() {
    if (!form) return;
    const checked = checkEngine(form);
    if (!checked.ok) {
      problem = { field: checked.field, message: checked.message };
      return;
    }
    if (isNew && engines.some((e) => e.id === checked.value.id)) {
      problem = { field: "id", message: `There is already an engine “${checked.value.id}”.` };
      return;
    }
    saving = true;
    try {
      await saveEngine(checked.value);
      form = null;
      await reload();
      onChange();
    } catch (err) {
      problem = { field: "", message: messageOf(err) };
    } finally {
      saving = false;
    }
  }

  async function remove(entry: EngineEntry) {
    if (!window.confirm(`Remove the engine “${entry.label}”?`)) return;
    try {
      await deleteEngine(entry.id);
      if (form?.id === entry.id) form = null;
      await reload();
      onChange();
    } catch (err) {
      // "In use" arrives as a readable refusal from Rust — show it as it is.
      toast(messageOf(err), "warning");
    }
  }

  onMount(reload);
</script>

<section>
  <h3>Engines</h3>
  <p class="lead">
    An engine is harness + model + environment — the part that costs money and can be unreachable. Roles and agent
    sessions refer to it by id, so a switch happens only here. Existing agents got their engine from their profile.
  </p>

  {#if loading}
    <p class="hint">Loading…</p>
  {:else if engines.length === 0}
    <p class="hint">No engines yet.</p>
  {:else}
    <ul class="list">
      {#each engines as e (e.id)}
        <li class:editing={form?.id === e.id && !isNew}>
          <button type="button" class="row" onclick={() => startEdit(e)}>
            <span class="title">{e.label}</span>
            <span class="meta">
              <code>{e.id}</code> · {HARNESSES.find((h) => h.id === e.harness)?.label ?? e.harness}
              · {e.model ?? "default model"} · {e.sessions} session{e.sessions === 1 ? "" : "s"}
            </span>
          </button>
          <button type="button" class="remove" title="Remove" aria-label="Remove engine" onclick={() => remove(e)}>
            ✕
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if form}
    <div class="form">
      <label class="field">
        <span>ID {isNew ? "" : "(fixed)"}</span>
        <input type="text" spellcheck="false" bind:value={form.id} disabled={!isNew} placeholder="e.g. claude-opus" />
        {#if problem?.field === "id"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Label</span>
        <input type="text" bind:value={form.label} placeholder="e.g. Claude Code · Opus" />
        {#if problem?.field === "label"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Harness</span>
        <select bind:value={form.harness}>
          {#each HARNESSES as h (h.id)}
            <option value={h.id}>{h.label} — {h.hint}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Model</span>
        <input
          type="text"
          spellcheck="false"
          value={form.model ?? ""}
          oninput={(e) => form && (form.model = e.currentTarget.value)}
          placeholder="empty = the harness picks; Opencode: provider/model"
        />
      </label>
      <label class="field">
        <span>Command (empty = the harness default)</span>
        <input type="text" spellcheck="false" bind:value={form.command} />
      </label>
      <label class="field">
        <span>Environment (<code>KEY=value</code> per line)</span>
        <textarea rows="2" spellcheck="false" bind:value={form.env}></textarea>
        {#if problem?.field === "env"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Billing</span>
        <select bind:value={form.billing}>
          {#each BILLINGS as b (b.id)}
            <option value={b.id}>{b.label} — {b.hint}</option>
          {/each}
        </select>
      </label>
      {#if problem && !problem.field}<p class="problem">{problem.message}</p>{/if}
      <div class="actions">
        <button type="button" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save engine"}</button>
        <button type="button" class="plain" onclick={() => (form = null)}>Cancel</button>
      </div>
    </div>
  {:else}
    <div class="actions">
      <button type="button" onclick={startNew}>Add engine</button>
    </div>
  {/if}
</section>

<style>
  .lead,
  .hint {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .list {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .list li {
    display: flex;
    align-items: stretch;
    gap: var(--ax-space-1);
  }
  .row {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: var(--ax-space-2);
    text-align: left;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  li.editing .row {
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .title {
    color: var(--ax-text);
  }
  .meta {
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .meta code {
    font-family: var(--ax-font-mono);
  }
  .remove {
    flex: 0 0 auto;
    padding: 0 var(--ax-space-2);
    background: transparent;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
  }
  .remove:hover {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }
  .form {
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    margin-bottom: var(--ax-space-3);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .field > span {
    color: var(--ax-text-muted);
  }
  .field input,
  .field select,
  .field textarea {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .field select {
    font-family: inherit;
  }
  .field input:focus,
  .field select:focus,
  .field textarea:focus {
    outline: none;
    border-color: var(--ax-accent);
  }
  .field input:disabled {
    opacity: 0.6;
  }
  .problem {
    font-style: normal;
    font-size: var(--ax-font-size-xs);
    color: var(--ax-warning);
  }
  p.problem {
    margin: 0 0 var(--ax-space-2);
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
  .plain {
    background: transparent;
  }
</style>
