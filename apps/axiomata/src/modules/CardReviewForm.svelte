<!--
  "Review starten": the owner starts the review of a reported card by hand (A2A CP-A6b) — what the studio does on its own
  as soon as a card is reported, except where it found no engine other than the one the work was done on, or the work
  changes files agents read their configuration from. The reviewer never runs on the worker's engine; the backend
  refuses it with the reason, so the list shows every engine.
-->
<script lang="ts">
  import { invokeBackend as invoke, type BoardCard, type ReviewSession } from "../core/backend";
  import { emit } from "../core/bus";
  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { listRoles, type Role } from "../core/roster";
  import { roleEngine } from "../ide/cardStart";
  import { engineCatalog, engineLine, refreshEngines } from "../ide/rosterStore";

  let { card, onDone }: { card: BoardCard; onDone: () => void } = $props();

  /** Empty = the reviewer role's own engine, offered only when the role has one. */
  let engineId = $state("");
  let roles = $state<Role[]>([]);
  const ownEngine = $derived(
    roleEngine(
      roles.find((role) => role.name === "reviewer"),
      $engineCatalog,
    ),
  );
  const ownEngineLabel = $derived($engineCatalog.find((engine) => engine.id === ownEngine)?.label ?? null);
  /** The work changes the files agents take their configuration from, and the owner accepts a reviewer on them. */
  let allowAgentConfig = $state(false);
  let error = $state("");
  let busy = $state(false);

  // A reviewer role that names no engine leaves the choice to the owner: the first engine is preselected. It may be the
  // worker's own, which the backend refuses with the reason — the owner then picks another.
  $effect(() => {
    if (engineId === "" && ownEngine === null && $engineCatalog.length > 0) engineId = $engineCatalog[0].id;
  });

  $effect(() => {
    void refreshEngines();
    listRoles()
      .then((loaded) => (roles = loaded.roles))
      .catch(() => {
        // Without the roles the form still works: the backend says what is missing.
      });
  });

  async function start(): Promise<void> {
    if (busy) return;
    busy = true;
    error = "";
    try {
      const session = await invoke<ReviewSession>("start_review_session", {
        cardId: card.id,
        engineId: engineId || null,
        allowAgentConfig,
      });
      toast(`Card #${card.id} is being reviewed by ${session.agent.name}.`, "info");
      emit("shell:agent", { projectId: session.agent.project_id, agentId: session.agent.id });
      onDone();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="review-form"
  onsubmit={(event) => {
    event.preventDefault();
    void start();
  }}
>
  <label>
    Engine des Reviewers
    <select bind:value={engineId} aria-label="Engine des Reviewers">
      {#if ownEngine !== null}
        <option value="">The reviewer role's own{ownEngineLabel ? ` (${ownEngineLabel})` : ""}</option>
      {/if}
      {#each $engineCatalog as engine (engine.id)}
        <option value={engine.id}>{engine.label} — {engineLine(engine)}</option>
      {/each}
    </select>
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={allowAgentConfig} />
    Auch wenn die Arbeit die Konfiguration von Agenten ändert (.claude, .mcp.json, opencode.json, AGENTS.md …)
  </label>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy}>{busy ? "Starting …" : "Start review"}</button>
    <button class="ax-btn" type="button" onclick={onDone}>Cancel</button>
  </div>
</form>

<style>
  .review-form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin: 0 0 var(--ax-space-3);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    font-size: var(--ax-font-size-sm);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
  }

  .check {
    flex-direction: row;
    align-items: baseline;
    gap: var(--ax-space-2);
  }

  select {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font: inherit;
  }

  .row {
    display: flex;
    gap: var(--ax-space-2);
  }

  .error {
    margin: 0;
    color: var(--ax-warning);
  }
</style>
