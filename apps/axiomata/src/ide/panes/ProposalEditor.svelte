<!--
  The owner's editor for one proposal of a plan that is still a draft — or the form for a card the owner adds by hand
  (A2A, the plan's panel). Title, description, acceptance criteria, role, level and the cards it has to wait for: what
  the planner proposed is a start, and a plan that "does not fit" is mended here, before it is approved. Saving writes
  the card and its "needs first" edges; a refusal of the board (a cycle, another plan) is shown as it comes.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { invokeBackend as invoke, type BoardCard, type BoardPlan, type CardTier } from "../../core/backend";
  import { messageOf } from "../../core/errors";
  import { fieldsOf } from "../../core/kanban";
  import type { Role } from "../../core/roster";
  import { assignableRoles, cardEditable, needsCandidates, needsDiff, proposalForm, proposalSavable, proposalTitle } from "../planning";

  let {
    card,
    plan,
    planCards,
    columnId,
    roles,
    freshCard,
    onSaved,
    onCancel,
  }: {
    /** The proposal to edit; `null` for a card the owner adds. */
    card: BoardCard | null;
    plan: BoardPlan;
    /** Every card of the plan. */
    planCards: BoardCard[];
    /** Where a new card goes: the proposal column of a draft, the first plain open column of a running plan. */
    columnId: number | null;
    roles: Role[];
    /** The card as the board has it now: a planner may have rewritten it since the list was drawn. */
    freshCard: (id: number) => Promise<BoardCard | null>;
    onSaved: () => Promise<void>;
    onCancel: () => void;
  } = $props();

  const TIERS: { id: CardTier | ""; label: string }[] = [
    { id: "", label: "—" },
    { id: "light", label: "leicht" },
    { id: "medium", label: "mittel" },
    { id: "heavy", label: "schwer" },
  ];

  // The form starts from the card as it is when the editor opens; the editor is re-made for another card, not updated.
  let form = $state(proposalForm(untrack(() => card) ?? undefined));
  let busy = $state(false);
  let error = $state("");

  const candidates = $derived(needsCandidates(planCards, card?.id ?? null));
  const titleOf = (id: number): string => {
    const other = planCards.find((c) => c.id === id);
    return other ? `#${id} ${proposalTitle(other)}` : `#${id}`;
  };

  function toggleNeed(id: number): void {
    form.needs = form.needs.includes(id) ? form.needs.filter((n) => n !== id) : [...form.needs, id].sort((a, b) => a - b);
  }

  async function save(): Promise<void> {
    if (busy || !proposalSavable(form)) return;
    busy = true;
    error = "";
    try {
      let id: number;
      let have: number[] = [];
      if (card) {
        const fresh = (await freshCard(card.id)) ?? card;
        if (!cardEditable(fresh)) throw new Error("Die Karte ist schon in Arbeit oder erledigt; sie lässt sich hier nicht mehr ändern.");
        await invoke("update_card", {
          id: card.id,
          fields: {
            ...fieldsOf(fresh),
            title: form.title.trim(),
            body: form.body,
            acceptance: form.acceptance,
            agent: form.agent || null,
            tier: form.tier || null,
          },
        });
        id = card.id;
        have = fresh.depends_on;
      } else {
        if (columnId === null) throw new Error("Das Brett hat dafür keine Spalte.");
        const created = await invoke<BoardCard>("create_card", {
          new: {
            column_id: columnId,
            title: form.title.trim(),
            body: form.body,
            labels: [],
            assignee: null,
            due_at: null,
            plan_id: plan.id,
            agent: form.agent || null,
            agent_reason: null,
            tier: form.tier || null,
            kind: null,
            acceptance: form.acceptance,
          },
        });
        id = created.id;
      }
      const { add, remove } = needsDiff(have, form.needs);
      for (const needs of remove) await invoke("remove_card_dependency", { cardId: id, needs });
      for (const needs of add) await invoke("add_card_dependency", { cardId: id, needs });
      await onSaved();
      onCancel();
    } catch (err) {
      error = messageOf(err);
      // The card itself may have been written before an edge was refused: the list shows what is true.
      await onSaved();
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="editor"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <label>
    Titel
    <input bind:value={form.title} aria-label="Titel" maxlength="200" />
  </label>
  <label>
    Beschreibung
    <textarea bind:value={form.body} rows="4" aria-label="Beschreibung"></textarea>
  </label>
  <label>
    Abnahmekriterien
    <textarea bind:value={form.acceptance} rows="3" aria-label="Abnahmekriterien" placeholder="Woran sieht man, dass die Karte fertig ist?"></textarea>
  </label>
  <div class="two">
    <label>
      Rolle
      <select bind:value={form.agent} aria-label="Rolle">
        <option value="">—</option>
        {#each assignableRoles(roles, form.agent || null) as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </label>
    <label>
      Stufe
      <select bind:value={form.tier} aria-label="Stufe">
        {#each TIERS as tier (tier.id)}<option value={tier.id}>{tier.label}</option>{/each}
      </select>
    </label>
  </div>
  <fieldset>
    <legend>Braucht zuerst</legend>
    {#if candidates.length === 0}
      <p class="muted">Der Plan hat keine andere Karte, auf die diese warten könnte.</p>
    {/if}
    {#each candidates as id (id)}
      <label class="check">
        <input type="checkbox" checked={form.needs.includes(id)} onchange={() => toggleNeed(id)} />
        {titleOf(id)}
      </label>
    {/each}
  </fieldset>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy || !proposalSavable(form)}>
      {card ? "Speichern" : "Karte hinzufügen"}
    </button>
    <button class="ax-btn" type="button" disabled={busy} onclick={onCancel}>Abbrechen</button>
  </div>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  input:not([type]),
  textarea,
  select {
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    padding: var(--ax-space-1) var(--ax-space-2);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  textarea {
    resize: vertical;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--ax-space-2);
  }
  fieldset {
    margin: 0;
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    max-height: calc(140px * var(--ax-ui-scale));
    overflow-y: auto;
  }
  legend {
    padding: 0 var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .check {
    flex-direction: row;
    align-items: center;
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }
  .muted {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .error {
    margin: 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
  .row {
    display: flex;
    gap: var(--ax-space-2);
  }
</style>
