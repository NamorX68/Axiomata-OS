<script lang="ts">
  import type { ModuleContext } from "../core/types";

  // Scaffolding for step 2 — replaced by real modules in steps 8–10.
  let { ctx }: { ctx: ModuleContext } = $props();
</script>

<div class="dummy">dummy module · {ctx.instanceId.slice(0, 8)}</div>

<style>
  .dummy {
    padding: var(--ax-space-3);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
</style>
