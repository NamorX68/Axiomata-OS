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
  function setStageFrom(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, stageFrom: value }));
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
  <label class="row">
    Stage from
    <select value={$config.stageFrom === "bottom" ? "bottom" : "right"} onchange={setStageFrom}>
      <option value="right">right</option>
      <option value="bottom">bottom</option>
    </select>
  </label>
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
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .row select {
    margin-left: auto;
  }
</style>
