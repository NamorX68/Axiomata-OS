<script lang="ts">
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function toggle(key: string, e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, [key]: checked }));
  }
</script>

<div class="settings">
  <label><input type="checkbox" checked={$config.spin !== false} onchange={(e) => toggle("spin", e)} /> Slow spin</label>
  <label><input type="checkbox" checked={$config.labels !== false} onchange={(e) => toggle("labels", e)} /> Labels</label>
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
    gap: var(--ax-space-2);
  }
</style>
