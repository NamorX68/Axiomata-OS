<script lang="ts">
  import type { ModuleContext } from "../core/types";

  // Scaffolding for step 2 — replaced by real modules in steps 8–10.
  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;
</script>

<div class="dummy" class:compact={$config.compact === true}>
  <p>dummy module · {ctx.instanceId.slice(0, 8)}</p>
  <p class="cfg">compact: {$config.compact === true ? "on" : "off"}</p>
</div>

<style>
  .dummy {
    padding: var(--ax-space-3);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .dummy.compact {
    padding: var(--ax-space-1) var(--ax-space-2);
  }
  p {
    margin: 0;
  }
  .cfg {
    color: var(--ax-accent);
  }
</style>
