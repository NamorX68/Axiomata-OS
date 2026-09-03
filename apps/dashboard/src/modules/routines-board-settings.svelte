<!-- Flip side of routines-board: display options + the add-routine form. -->
<script lang="ts">
  import type { NewRoutine, Routine } from "../core/backend";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let name = $state("");
  let cron = $state("");
  let targetType = $state<"skill" | "prompt">("skill");
  let targetValue = $state("");
  let backend = $state("");
  let adding = $state(false);
  let result = $state("");

  function setShowDisabled(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, showDisabled: checked }));
  }
  function setSort(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, sort: value }));
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (adding) return;
    adding = true;
    const routine: NewRoutine = {
      name: name.trim(),
      cron_expr: cron.trim(),
      target: { type: targetType, value: targetValue.trim() },
      backend: backend === "" ? null : backend,
      enabled: true,
    };
    try {
      const created = await ctx.invoke<Routine>("add_routine", { new: routine });
      result = `Added #${created.id}.`;
      name = "";
      cron = "";
      targetValue = "";
    } catch (err) {
      result = `Add failed: ${String(err)}`;
    } finally {
      adding = false;
    }
  }
</script>

<div class="settings">
  <label class="row">
    <input type="checkbox" checked={$config.showDisabled !== false} onchange={setShowDisabled} />
    Show disabled routines
  </label>
  <label class="row">
    Sort
    <select value={$config.sort === "name" ? "name" : "next"} onchange={setSort}>
      <option value="next">soonest first</option>
      <option value="name">by name</option>
    </select>
  </label>

  <form onsubmit={add}>
    <h3>Add routine</h3>
    <input type="text" placeholder="name" bind:value={name} required />
    <input
      type="text"
      class="mono"
      placeholder="0 0 9 * * *  (sec min hour dom mon dow)"
      bind:value={cron}
      required
    />
    <div class="pair">
      <select bind:value={targetType}>
        <option value="skill">skill</option>
        <option value="prompt">prompt</option>
      </select>
      <input type="text" placeholder={targetType === "skill" ? "skill name" : "prompt text"} bind:value={targetValue} required />
    </div>
    <div class="pair">
      <select bind:value={backend}>
        <option value="">default backend</option>
        <option value="claude-code">claude-code</option>
        <option value="ollama">ollama</option>
      </select>
      <button type="submit" disabled={adding}>{adding ? "Adding…" : "Add"}</button>
    </div>
    {#if result}<p class="result">{result}</p>{/if}
  </form>
</div>

<style>
  .settings {
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .row select {
    margin-left: auto;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-top: var(--ax-space-1);
    padding-top: var(--ax-space-1);
    border-top: 1px solid var(--ax-border);
  }
  h3 {
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .pair {
    display: flex;
    gap: var(--ax-space-2);
  }
  .pair input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .mono {
    font-family: var(--ax-font-mono);
  }
  .result {
    margin: 0;
    color: var(--ax-text-muted);
  }
</style>
