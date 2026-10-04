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
  import { engineLine, refreshEngines } from "./rosterStore";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

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
      // The Agents panel picks from the same catalog.
      void refreshEngines();
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
    An engine is harness + model + environment — the part that costs money and can be unreachable. Engines are made
    only here; an agent in the Agents panel is built by choosing one, and roles refer to them by id.
  </p>

  {#if loading}
    <p class="hint">Loading…</p>
  {:else if engines.length === 0}
    <p class="hint">No engines yet.</p>
  {:else}
    <ul class="cards">
      {#each engines as e (e.id)}
        <li class="card" class:editing={form?.id === e.id && !isNew} style:--harness="var(--ax-harness-{e.harness})">
          <button type="button" class="open" onclick={() => startEdit(e)}>
            <span class="top">
              <span class="avatar" aria-hidden="true">{e.label.trim().charAt(0).toUpperCase() || "?"}</span>
              <span class="who">
                <span class="name">{e.label}</span>
                <span class="meta">{engineLine(e)}</span>
              </span>
            </span>
            <span class="since">
              <code>{e.id}</code> · {e.billing === "subscription" ? "subscription" : "metered"} · {e.sessions} agent{e.sessions ===
              1
                ? ""
                : "s"}
            </span>
          </button>
          <div class="row-actions">
            <IconButton icon="pencil" label="Edit {e.label}" size="sm" onclick={() => startEdit(e)} />
            <IconButton icon="trash-2" label="Remove {e.label}" size="sm" onclick={() => remove(e)} />
          </div>
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
      <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> Add engine…</button>
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
  /* A card per engine, the same look as an agent in the Agents panel: the harness's colour on its edge. */
  .cards {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
  }
  .card {
    position: relative;
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
  .card.editing {
    box-shadow: 0 0 0 1px var(--ax-accent) inset;
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
  .name,
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-weight: 600;
  }
  .meta,
  .since {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .since code {
    font-family: var(--ax-font-mono);
  }
  /* Quiet until the card is hovered or focused (editor-look I4). */
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
