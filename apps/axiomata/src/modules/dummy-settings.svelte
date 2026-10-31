<script lang="ts">
  import type { ModuleContext } from "../core/types";
  import { tCanvas } from "../i18n/canvas";

  // Flip-side settings for the step-2 dummy: proves per-instance config
  // round-trips through `ctx.config`.
  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function setCompact(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, compact: checked }));
  }
</script>

<div class="settings">
  <label>
    <input type="checkbox" checked={$config.compact === true} onchange={setCompact} />
    {tCanvas("compactLayout")}
  </label>
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }
  label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
</style>
