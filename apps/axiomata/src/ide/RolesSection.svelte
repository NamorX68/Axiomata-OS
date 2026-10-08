<!--
  Roles (a2a.md CP-A1): what an agent is for. One `AGENT.md` per role under `~/.axiomata/agents/`; this is the
  form for it. A role names engines by id and never carries a command line. The form keeps lists and numbers as
  text (`RoleForm`); `checkRoleForm` builds the role or points at the first bad field, and Rust has the last word.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { messageOf } from "../core/errors";
  import {
    TIERS,
    blankRoleForm,
    checkRoleForm,
    deleteRole,
    listEngines,
    listRoles,
    roleToForm,
    saveRole,
    type EngineEntry,
    type Role,
    type RoleForm,
    type SkippedRole,
  } from "../core/roster";
  import { toast } from "../core/toast";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

  /** Bumped by the parent when the engine catalog changed, so the picker reloads. */
  let { enginesVersion = 0 }: { enginesVersion?: number } = $props();

  let roles = $state<Role[]>([]);
  let skipped = $state<SkippedRole[]>([]);
  let engines = $state<EngineEntry[]>([]);
  let loading = $state(true);
  let form = $state<RoleForm | null>(null);
  let isNew = $state(false);
  let problem = $state<{ field: string; message: string } | null>(null);
  let saving = $state(false);

  async function reload() {
    try {
      const [loaded, catalog] = await Promise.all([listRoles(), listEngines()]);
      roles = loaded.roles;
      skipped = loaded.skipped;
      engines = catalog;
    } catch (err) {
      toast(`Could not load the roles: ${messageOf(err)}`, "warning");
    } finally {
      loading = false;
    }
  }

  function startNew() {
    form = blankRoleForm();
    isNew = true;
    problem = null;
  }

  function startEdit(role: Role) {
    form = roleToForm(role);
    isNew = false;
    problem = null;
  }

  async function save() {
    if (!form) return;
    const checked = checkRoleForm(
      form,
      engines.map((e) => e.id),
    );
    if (!checked.ok) {
      problem = { field: checked.field, message: checked.message };
      return;
    }
    if (isNew && roles.some((r) => r.name === checked.value.name)) {
      problem = { field: "name", message: `There is already a role “${checked.value.name}”.` };
      return;
    }
    saving = true;
    try {
      await saveRole(checked.value);
      form = null;
      await reload();
    } catch (err) {
      problem = { field: "", message: messageOf(err) };
    } finally {
      saving = false;
    }
  }

  async function remove(role: Role) {
    if (!window.confirm(`Delete the role “${role.name}”? Its AGENT.md file is removed.`)) return;
    try {
      await deleteRole(role.name);
      if (form?.name === role.name && !isNew) form = null;
      await reload();
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  const tierLabel = (id: Role["tier"]) => TIERS.find((t) => t.id === id)?.label ?? id;

  onMount(reload);

  // A change of the catalog elsewhere must reach the picker; the first run is `onMount`'s.
  let firstRun = true;
  $effect(() => {
    void enginesVersion;
    if (firstRun) {
      firstRun = false;
      return;
    }
    untrack(() => void reload());
  });
</script>

<section>
  <h3>Roles</h3>
  <p class="lead">
    A role says what an agent is for: kind of work, tier, engine, limits and the working instructions. It lives as a
    file at <code>~/.axiomata/agents/&lt;name&gt;/AGENT.md</code>. A role names engines only by id — it can never start
    a program itself.
  </p>

  {#if loading}
    <p class="hint">Loading…</p>
  {:else}
    {#if roles.length === 0}
      <p class="hint">No roles yet.</p>
    {:else}
      <ul class="cards">
        {#each roles as r (r.name)}
          {@const engine = engines.find((e) => e.id === r.engine)}
          <li
            class="card"
            class:editing={form?.name === r.name && !isNew}
            style:--harness={engine ? `var(--ax-harness-${engine.harness})` : undefined}
          >
            <button type="button" class="open" onclick={() => startEdit(r)}>
              <span class="top">
                <span class="avatar" aria-hidden="true">{r.name.trim().charAt(0).toUpperCase() || "?"}</span>
                <span class="who">
                  <span class="name">{r.name} <span class="tier">{tierLabel(r.tier)}</span></span>
                  <span class="meta">
                    {r.kind} · {r.engine ? (engine?.label ?? r.engine) : "engine chosen at start"}
                  </span>
                </span>
              </span>
              {#if r.description}<span class="since">{r.description}</span>{/if}
            </button>
            <div class="row-actions">
              <IconButton icon="pencil" label="Edit {r.name}" size="sm" onclick={() => startEdit(r)} />
              <IconButton icon="trash-2" label="Delete {r.name}" size="sm" onclick={() => remove(r)} />
            </div>
          </li>
        {/each}
      </ul>
    {/if}
    {#each skipped as s (s.name)}
      <p class="problem">Skipped: <code>{s.name}</code> — {s.reason}</p>
    {/each}
  {/if}

  {#if form}
    <div class="form">
      <label class="field">
        <span>Name {isNew ? "" : "(fixed)"}</span>
        <input type="text" spellcheck="false" bind:value={form.name} disabled={!isNew} placeholder="e.g. implementer-light" />
        {#if problem?.field === "name"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Description</span>
        <input type="text" bind:value={form.description} />
        {#if problem?.field === "description"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <div class="pair">
        <label class="field">
          <span>Kind of work</span>
          <input type="text" spellcheck="false" bind:value={form.kind} placeholder="implement, review, plan, test, doc" />
          {#if problem?.field === "kind"}<em class="problem">{problem.message}</em>{/if}
        </label>
        <label class="field">
          <span>Tier</span>
          <select bind:value={form.tier}>
            {#each TIERS as t (t.id)}
              <option value={t.id}>{t.label} — {t.hint}</option>
            {/each}
          </select>
        </label>
      </div>
      <label class="field">
        <span>Default engine</span>
        <select bind:value={form.engine}>
          <option value="">— chosen at every start (e.g. the planner)</option>
          {#each engines as e (e.id)}
            <option value={e.id}>{e.label} ({e.id})</option>
          {/each}
        </select>
        {#if problem?.field === "engine"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Fallback engines (ids, comma-separated, in this order)</span>
        <input type="text" spellcheck="false" bind:value={form.fallback} placeholder={engines.map((e) => e.id).join(", ")} />
        {#if problem?.field === "fallback"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Creates cards without asking (kinds, comma-separated)</span>
        <input type="text" spellcheck="false" bind:value={form.creates} placeholder="e.g. test, doc" />
        {#if problem?.field === "creates"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <label class="field">
        <span>Rights (harness rules, one per line)</span>
        <textarea rows="2" spellcheck="false" bind:value={form.permissions}></textarea>
      </label>
      <div class="triple">
        <label class="field">
          <span>Cost limit (USD)</span>
          <input type="text" inputmode="decimal" bind:value={form.maxCost} placeholder="empty = default" />
          {#if problem?.field === "maxCost"}<em class="problem">{problem.message}</em>{/if}
        </label>
        <label class="field">
          <span>Token limit</span>
          <input type="text" inputmode="numeric" bind:value={form.maxTokens} placeholder="empty = default" />
          {#if problem?.field === "maxTokens"}<em class="problem">{problem.message}</em>{/if}
        </label>
        <label class="field">
          <span>Step limit</span>
          <input type="text" inputmode="numeric" bind:value={form.maxSteps} placeholder="empty = default" />
          {#if problem?.field === "maxSteps"}<em class="problem">{problem.message}</em>{/if}
        </label>
      </div>
      <label class="field">
        <span>Working instructions (Markdown)</span>
        <textarea rows="8" spellcheck="false" bind:value={form.instructions}></textarea>
      </label>
      {#if problem && !problem.field}<p class="problem">{problem.message}</p>{/if}
      <div class="actions">
        <button type="button" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save role"}</button>
        <button type="button" class="plain" onclick={() => (form = null)}>Cancel</button>
      </div>
    </div>
  {:else if !loading}
    <div class="actions">
      <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> Add role…</button>
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
  .lead code,
  .problem code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }
  /* A card per role, the same look as an engine's (and an agent's in the Agents panel): the engine's colour on its edge. */
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
  .tier {
    margin-left: var(--ax-space-1);
    padding: 0 var(--ax-space-1);
    font-size: var(--ax-font-size-xs);
    font-weight: 400;
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
    border-radius: var(--ax-radius-sm);
  }
  .meta,
  .since {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
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
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--ax-space-2);
  }
  .triple {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--ax-space-2);
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
  .field textarea {
    resize: vertical;
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
