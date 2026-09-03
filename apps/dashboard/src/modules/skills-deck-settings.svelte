<script lang="ts">
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function set(key: string, value: string) {
    config.update((c) => ({ ...c, [key]: value }));
  }
  const value = (e: Event) => (e.currentTarget as HTMLSelectElement).value;
</script>

<div class="settings">
  <label>
    Backend
    <select value={typeof $config.filterBackend === "string" ? $config.filterBackend : ""} onchange={(e) => set("filterBackend", value(e))}>
      <option value="">all</option>
      <option value="claude-code">claude-code</option>
      <option value="ollama">ollama</option>
    </select>
  </label>
  <label>
    Order
    <select value={$config.order === "recent" ? "recent" : "name"} onchange={(e) => set("order", value(e))}>
      <option value="name">by name</option>
      <option value="recent">recently run first</option>
    </select>
  </label>
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
</style>
