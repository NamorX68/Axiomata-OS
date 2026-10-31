<script lang="ts">
  import type { ModuleContext } from "../core/types";
  import { tModules } from "../i18n/modules";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function toggleShowDone(e: Event) {
    const on = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, showDone: on }));
  }
</script>

<div class="settings">
  <label>
    {tModules("showDoneDefault")}
    <input type="checkbox" checked={$config.showDone === true} onchange={toggleShowDone} />
  </label>
  <p class="muted">{tModules("backedByBefore")}<code>ToDo.md</code>{tModules("backedByAfter")}</p>
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
  }
  input[type="checkbox"] {
    accent-color: var(--ax-accent);
  }
  .muted {
    color: var(--ax-text-muted);
    margin: 0;
  }
  code {
    font-family: var(--ax-font-mono);
  }
</style>
