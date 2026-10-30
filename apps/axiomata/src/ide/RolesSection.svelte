<!--
  Roles (a2a.md CP-A1; `docs/plans/agents-model.md` AM3, Q6): what an agent is for — instruction, rights, limits, kind
  of work and a default tier. No engine any more: the tier's engine runs (Models tab). An `engine` or
  `fallback_engines` a file still carries is kept as it is on save but not shown (AM6 migrates it). One `AGENT.md` per
  role under `~/.axiomata/agents/`; this is the form for it. The form keeps lists and numbers as text (`RoleForm`);
  `checkRoleForm` builds the role or points at the first bad field, and Rust has the last word.
  Laid out as a list with its form beside it (`docs/plans/studio-roster-ui.md`, R-1). The name typed becomes the id
  (R-5): the form shows the id a new role gets, and the rarely used fields sit under "Advanced".
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { messageOf } from "../core/errors";
  import {
    TIERS,
    blankRoleForm,
    checkRoleForm,
    deleteRole,
    listRoles,
    roleToForm,
    saveRole,
    type Role,
    type RoleForm,
    type SkippedRole,
  } from "../core/roster";
  import { toast } from "../core/toast";
  import RoleChip from "./RoleChip.svelte";
  import { roleIdFor } from "./rosterLook";
  import { bumpRoster } from "./rosterStore";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

  /** The fields this form shows a problem beside; any other problem is shown above the buttons. */
  const SHOWN_FIELDS = ["name", "description", "kind", "creates", "maxCost", "maxTokens", "maxSteps"];

  let roles = $state<Role[]>([]);
  let skipped = $state<SkippedRole[]>([]);
  let loading = $state(true);
  let form = $state<RoleForm | null>(null);
  let isNew = $state(false);
  let problem = $state<{ field: string; message: string } | null>(null);
  let saving = $state(false);

  async function reload() {
    try {
      const loaded = await listRoles();
      roles = loaded.roles;
      skipped = loaded.skipped;
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

  /** The id a new role gets: the name typed as a slug, clear of the taken ones (an empty name: kind and tier). */
  const derivedId = $derived(
    form && isNew
      ? roleIdFor(form.name, form.kind, form.tier, roles.map((r) => r.name))
      : (form?.name ?? ""),
  );

  /** The fields under "Advanced": a problem in one of them opens the section, so it is never hidden from the owner. */
  const advancedProblem = $derived(problem !== null && ["creates"].includes(problem.field));

  async function save() {
    if (!form) return;
    // The engine fields go back as the file had them; they are no longer checked here (Q6).
    const checked = checkRoleForm({ ...form, name: derivedId }, null);
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
      bumpRoster();
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
      bumpRoster();
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  const tierLabel = (id: Role["tier"]) => TIERS.find((t) => t.id === id)?.label ?? id;

  onMount(reload);
</script>

<section class="split" class:has-form={form !== null}>
  <div class="list">
    <div class="list-head">
      <h3>Roles</h3>
      <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> Add role…</button>
    </div>
    {#if loading}
      <p class="hint">Loading…</p>
    {:else}
      {#if roles.length === 0}
        <p class="hint">No roles yet.</p>
      {:else}
        <ul class="cards">
          {#each roles as r (r.name)}
            <li class="card" class:editing={form?.name === r.name && !isNew}>
              <button type="button" class="open" onclick={() => startEdit(r)}>
                <span class="top">
                  <span class="avatar" aria-hidden="true">{r.name.trim().charAt(0).toUpperCase() || "?"}</span>
                  <span class="who">
                    <span class="name">{r.name}</span>
                    <span class="meta">
                      <RoleChip label={r.kind} kind={r.kind} />
                      default tier {tierLabel(r.tier).toLowerCase()}
                    </span>
                  </span>
                </span>
                {#if r.description}<span class="since">{r.description}</span>{/if}
              </button>
              <div class="row-actions">
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
  </div>

  <div class="detail">
    {#if form}
      <button type="button" class="back" onclick={() => (form = null)}>← All roles</button>
      <h3>{isNew ? "New role" : form.name}</h3>
      {#if isNew}
        <label class="field">
          <span>Name</span>
          <input type="text" spellcheck="false" bind:value={form.name} placeholder="e.g. Reviewer light" />
          {#if problem?.field === "name"}<em class="problem">{problem.message}</em>{/if}
        </label>
        <p class="idline">ID and file name: <code>{derivedId || "—"}</code></p>
      {/if}
      <label class="field">
        <span>Description</span>
        <input type="text" bind:value={form.description} />
        {#if problem?.field === "description"}<em class="problem">{problem.message}</em>{/if}
      </label>
      <div class="pair">
        <label class="field">
          <span>Kind of work</span>
          <input type="text" spellcheck="false" list="role-kinds" bind:value={form.kind} placeholder="implement, review, plan, test, doc" />
          <datalist id="role-kinds">
            {#each ["implement", "review", "plan", "test", "doc"] as k (k)}<option value={k}></option>{/each}
          </datalist>
          {#if problem?.field === "kind"}<em class="problem">{problem.message}</em>{/if}
        </label>
        <div class="field">
          <span id="role-tier">Default tier</span>
          <div class="segments" role="radiogroup" aria-labelledby="role-tier">
            {#each TIERS as t (t.id)}
              <button
                type="button"
                role="radio"
                aria-checked={form.tier === t.id}
                class:mine={form.tier === t.id}
                title={t.hint}
                onclick={() => form && (form.tier = t.id)}>{t.label}</button
              >
            {/each}
          </div>
        </div>
      </div>
      <label class="field">
        <span>Working instructions (Markdown)</span>
        <textarea rows="12" spellcheck="false" bind:value={form.instructions}></textarea>
      </label>
      <label class="field">
        <span>Rights (harness rules, one per line)</span>
        <textarea rows="2" spellcheck="false" bind:value={form.permissions}></textarea>
      </label>
      <div class="field">
        <span>Limits of a session (empty = the tier's default)</span>
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
      </div>
      <details class="advanced" open={advancedProblem}>
        <summary>Advanced</summary>
        <label class="field">
          <span>Creates cards without asking (kinds, comma-separated)</span>
          <input type="text" spellcheck="false" bind:value={form.creates} placeholder="e.g. test, doc" />
          {#if problem?.field === "creates"}<em class="problem">{problem.message}</em>{/if}
        </label>
      </details>
      <!-- A problem of a field the form does not show (or of none) is said here, never swallowed. -->
      {#if problem && !SHOWN_FIELDS.includes(problem.field)}<p class="problem">{problem.message}</p>{/if}
      <div class="actions">
        <button type="button" class="ax-btn primary" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save role"}</button>
        <button type="button" class="ax-btn" onclick={() => (form = null)}>Cancel</button>
      </div>
    {:else if !loading}
      <p class="hint">
        A role says what an agent is for: kind of work, default tier, rights, limits and the working instructions —
        no engine: the tier's engine runs (Models tab). It lives as a file at
        <code>~/.axiomata/agents/&lt;id&gt;/AGENT.md</code> and can never start a program itself. Pick one on the left
        to change it, or add a new one.
      </p>
    {/if}
  </div>
</section>

<style>
  /* The list and its form side by side, each scrolling on its own. */
  .split {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(calc(220px * var(--ax-ui-scale)), 34%) minmax(0, 1fr);
  }
  /* In a narrow pane the list and the form take turns: the form never sits at the end of a long list. */
  .back {
    display: none;
    margin-bottom: var(--ax-space-2);
    padding: 0;
    background: none;
    border: 0;
    color: var(--ax-accent);
    font: inherit;
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }
  /* Container queries cannot use the UI scale, so the breakpoint is a plain width. */
  @container (max-width: 640px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
    }
    .split.has-form .list,
    .split:not(.has-form) .detail {
      display: none;
    }
    .back {
      display: inline-block;
    }
    .list {
      border-right: 0;
    }
  }
  .list,
  .detail {
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
  }
  .list {
    border-right: 1px solid var(--ax-border);
  }
  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-3);
  }
  h3 {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-lg);
  }
  .list-head h3 {
    margin: 0;
  }
  .hint {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
    line-height: var(--ax-line-height);
    color: var(--ax-text-muted);
  }
  .hint code,
  .problem code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }
  /* A card per role, like an engine's and an agent's in the Agents panel: the engine's colour on its edge. */
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
    border-left: calc(3px * var(--ax-ui-scale)) solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
  }
  .card:hover,
  .card:focus-within {
    border-color: var(--ax-border-strong);
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
    padding: var(--ax-space-1) var(--ax-space-2);
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
  .advanced {
    margin-bottom: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .advanced summary {
    cursor: pointer;
    margin-bottom: var(--ax-space-2);
  }
  .idline {
    margin: 0 0 var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .idline code {
    font-family: var(--ax-font-mono);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  /* The default tier as one choice of three, like a model's tier in the Models tab. */
  .segments {
    display: inline-flex;
    align-self: flex-start;
    padding: calc(2px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
  }
  .segments button {
    padding: calc(2px * var(--ax-ui-scale)) var(--ax-space-3);
    border: 0;
    border-radius: var(--ax-radius-pill);
    background: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }
  .segments button:hover {
    color: var(--ax-text);
  }
  .segments button.mine {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
    font-weight: 600;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--ax-space-3);
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
  .field textarea {
    resize: vertical;
  }
  .field input:focus,
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
</style>
