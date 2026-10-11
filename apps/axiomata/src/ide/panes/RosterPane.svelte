<!--
  The Studio's engines and roles (`docs/plans/studio-roster-ui.md`, R-1): one large pane in the dock, a list on the
  left and its form beside it. They live with the Studio, not in the app's general settings, so they move along when
  the Studio becomes a program of its own (A11); the data behind them (`axiomata-roster`) already takes its paths from
  outside.
  Both sections stay mounted while the other is shown, so a half-filled form survives a look at the other list.
-->
<script lang="ts">
  import EnginesSection from "../EnginesSection.svelte";
  import RolesSection from "../RolesSection.svelte";

  type Part = "engines" | "roles";

  let part = $state<Part>("engines");
  /** Bumped when the engine catalog changed, so the roles' engine picker reloads. */
  let enginesVersion = $state(0);
</script>

<div class="roster">
  <header>
    <div class="switch" role="group" aria-label="Engines or roles">
      <button type="button" class:active={part === "engines"} aria-pressed={part === "engines"} onclick={() => (part = "engines")}>
        Engines
      </button>
      <button type="button" class:active={part === "roles"} aria-pressed={part === "roles"} onclick={() => (part = "roles")}>
        Roles
      </button>
    </div>
    <p class="chain">
      <strong>Engine</strong> = what thinks (harness + model) · <strong>Role</strong> = what it is for (kind of
      work, instructions, limits) · <strong>Agent</strong> = a session in the project: a role on an engine, with
      its own branch.
    </p>
  </header>
  <div class="part" class:off={part !== "engines"}>
    <EnginesSection onChange={() => (enginesVersion += 1)} />
  </div>
  <div class="part" class:off={part !== "roles"}>
    <RolesSection {enginesVersion} />
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

  .switch button.active {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
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
