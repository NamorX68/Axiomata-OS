<!--
  The Studio's models and roles (`docs/plans/agents-model.md`, AM3, Q20; before it, engines and roles —
  `docs/plans/studio-roster-ui.md`, R-1): one large pane in the dock with two tabs. Models: which models are on, their
  tiers, the default engine per tier, the price limits and the owner's own engines. Roles: what an agent is for. They live with the Studio, not in the app's general settings, so they move along when
  the Studio becomes a program of its own (A11); the data behind them (`axiomata-roster`) already takes its paths from
  outside.
  Both sections stay mounted while the other is shown, so a half-filled form survives a look at the other list.
-->
<script lang="ts">
  import ModelsSection from "../ModelsSection.svelte";
  import RolesSection from "../RolesSection.svelte";

  type Part = "models" | "roles";

  let part = $state<Part>("models");
</script>

<div class="roster">
  <header>
    <div class="switch" role="tablist" aria-label="Models or roles">
      <button type="button" role="tab" class:active={part === "models"} aria-selected={part === "models"} onclick={() => (part = "models")}>
        Models
      </button>
      <button type="button" role="tab" class:active={part === "roles"} aria-selected={part === "roles"} onclick={() => (part = "roles")}>
        Roles
      </button>
    </div>
    <p class="chain">
      <strong>Model</strong> = what thinks, at a tier (light · medium · heavy), on the harness that runs it ·
      <strong>Role</strong> = what it is for (instructions, rights, limits, a default tier) · <strong>Agent</strong> =
      a role at a tier on an engine, started as a session with its own branch.
    </p>
  </header>
  <div class="part" class:off={part !== "models"}>
    <ModelsSection />
  </div>
  <div class="part" class:off={part !== "roles"}>
    <RolesSection />
  </div>
</div>

<style>
  .roster {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    /* The sections ask how wide the pane is, not the window: the dock may give it a narrow column. */
    container-type: inline-size;
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2) var(--ax-space-4);
    padding: var(--ax-space-2) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }

  .switch {
    display: flex;
    flex: 0 0 auto;
    gap: var(--ax-space-1);
    padding: calc(2px * var(--ax-ui-scale));
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
  }

  .switch button {
    padding: var(--ax-space-1) var(--ax-space-4);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .switch button:hover {
    color: var(--ax-text);
  }

  /* Selected ≠ alive (accent rule): the open tab is a selection, not orange. */
  .switch button.active {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }

  .chain {
    flex: 1 1 calc(260px * var(--ax-ui-scale));
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: var(--ax-line-height);
  }

  .chain strong {
    color: var(--ax-text);
    font-weight: 600;
  }

  .part {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  .part.off {
    display: none;
  }
</style>
