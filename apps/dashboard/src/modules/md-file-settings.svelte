<script lang="ts">
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let pathInput = $state(typeof $config.path === "string" ? $config.path : "");

  function open(e: SubmitEvent) {
    e.preventDefault();
    const clean = pathInput.trim();
    if (clean) config.update((c) => ({ ...c, path: clean, mode: "read" }));
  }
</script>

<div class="settings">
  <form onsubmit={open}>
    <label for="md-path-{ctx.instanceId}">Workspace file</label>
    <div class="pair">
      <input id="md-path-{ctx.instanceId}" type="text" placeholder="notes/inbox.md" bind:value={pathInput} />
      <button type="submit">Open</button>
    </div>
  </form>
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .pair {
    display: flex;
    gap: var(--ax-space-2);
  }
  .pair input {
    flex: 1 1 auto;
    min-width: 0;
    font-family: var(--ax-font-mono);
  }
</style>
