<!--
  "Starten": the owner's click that makes a session for a card (A2A CP-A6a). Asks for the two things the card does not
  know — the project (repository) the work happens in, and, when the card's role names no engine of its own, the
  engine —
  then has the backend make the session and take the card, and asks the Studio to open the session's pane.
-->
<script lang="ts">
  import { invokeBackend as invoke, type BoardCard, type CardSession, type IdeProject } from "../core/backend";
  import { emit } from "../core/bus";
  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { roleEngine, startableProjects } from "../ide/cardStart";
  import { listRoles, type Role } from "../core/roster";
  import { listProjects } from "../ide/projects";
  import { engineCatalog, engineLine, refreshEngines } from "../ide/rosterStore";

  let { card, onDone }: { card: BoardCard; onDone: () => void } = $props();

  let projects = $state<IdeProject[]>([]);
  let projectId = $state<number | null>(null);
  /** Empty = the role's own engine, offered only when the role has one. */
  let engineId = $state("");
  let roles = $state<Role[]>([]);
  /** The role the card names, else the one every card without a role is given. */
  const roleName = $derived(card.agent || "allrounder");
  const ownEngine = $derived(
    roleEngine(
      roles.find((role) => role.name === roleName),
      $engineCatalog,
    ),
  );
  const ownEngineLabel = $derived($engineCatalog.find((engine) => engine.id === ownEngine)?.label ?? null);
  let error = $state("");
  let busy = $state(false);

  // A role that names no engine leaves the choice to the owner: the first engine is preselected, so the form never
  // starts on an empty choice that the backend would refuse.
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
    listProjects()
      .then((all) => {
        projects = startableProjects(all);
        projectId = projects[0]?.id ?? null;
      })
      .catch((err: unknown) => (error = messageOf(err)));
  });

  async function start(): Promise<void> {
    if (projectId === null || busy) return;
    busy = true;
    error = "";
    try {
      const session = await invoke<CardSession>("start_card_session", {
        cardId: card.id,
        projectId,
        engineId: engineId || null,
      });
      toast(`Karte #${card.id} läuft in der Sitzung ${session.agent.name}.`, "info");
      emit("shell:agent", { projectId, agentId: session.agent.id });
      onDone();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="start-form"
  onsubmit={(event) => {
    event.preventDefault();
    void start();
  }}
>
  {#if projects.length === 0}
    <p class="hint">Es gibt kein Projekt, in dem die Karte laufen könnte. Lege im Studio eines an.</p>
  {:else}
    <label>
      Projekt
      <select bind:value={projectId} aria-label="Projekt">
        {#each projects as project (project.id)}<option value={project.id}>{project.name}</option>{/each}
      </select>
    </label>
    <label>
      Engine
      <select bind:value={engineId} aria-label="Engine">
        {#if ownEngine !== null}
          <option value="">Die der Rolle {roleName}{ownEngineLabel ? ` (${ownEngineLabel})` : ""}</option>
        {/if}
        {#each $engineCatalog as engine (engine.id)}
          <option value={engine.id}>{engine.label} — {engineLine(engine)}</option>
        {/each}
      </select>
    </label>
    {#if ownEngine === null && roles.length > 0}
      <p class="hint">Die Rolle „{roleName}“ nennt keine Engine — wähle eine.</p>
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy || projectId === null}>
      {busy ? "Startet …" : "Starten"}
    </button>
    <button class="ax-btn" type="button" onclick={onDone}>Abbrechen</button>
  </div>
</form>

<style>
  .start-form {
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

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
  }

  .error {
    margin: 0;
    color: var(--ax-warning);
  }
</style>
