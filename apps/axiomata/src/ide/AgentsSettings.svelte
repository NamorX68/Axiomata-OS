<!--
  The Studio's own settings for agents (a2a.md CP-A1): the engine catalog and the roles, as the "Agents" tab of the
  inspector. They live with the Studio, not in the app's general settings, so they move along when the Studio becomes
  a program of its own (A11); the data behind them (`axiomata-roster`) already takes its paths from outside.
-->
<script lang="ts">
  import EnginesSection from "./EnginesSection.svelte";
  import RolesSection from "./RolesSection.svelte";

  /** Bumped when the engine catalog changed, so the roles' engine picker reloads. */
  let enginesVersion = $state(0);
</script>

<div class="agents-settings">
  <EnginesSection onChange={() => (enginesVersion += 1)} />
  <RolesSection {enginesVersion} />
</div>

<style>
  .agents-settings {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-4);
  }
</style>
