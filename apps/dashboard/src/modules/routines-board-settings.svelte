<!-- Flip side of routines-board: display options + the add-routine form. -->
<script lang="ts">
  import type { NewRoutine, Routine } from "../core/backend";
  import type { ModuleContext } from "../core/types";
  import RoutineForm, { type RoutineFormFields } from "./RoutineForm.svelte";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let adding = $state(false);
  let result = $state("");
  // Bumped on every successful add to remount `RoutineForm` with fresh
  // (empty) internal state — simpler than threading a `reset()` method
  // through the shared component.
  let formKey = $state(0);

  function setShowDisabled(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, showDisabled: checked }));
  }
  function setSort(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, sort: value }));
  }

  async function add(fields: RoutineFormFields) {
    if (adding) return;
    adding = true;
    const routine: NewRoutine = {
      name: fields.name,
      cron_expr: fields.cronExpr,
      target: { type: fields.targetType, value: fields.targetValue },
      backend: fields.backend === "" ? null : fields.backend,
      enabled: true,
    };
    try {
      const created = await ctx.invoke<Routine>("add_routine", { new: routine });
      result = `Added #${created.id}.`;
      formKey += 1;
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

  <div class="add-form">
    <h3>Add routine</h3>
    {#key formKey}
      <RoutineForm submitLabel={adding ? "Adding…" : "Add"} busy={adding} onSubmit={add} />
    {/key}
    {#if result}<p class="result">{result}</p>{/if}
  </div>
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

  .add-form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-top: var(--ax-space-1);
    padding-top: var(--ax-space-1);
    border-top: 1px solid var(--ax-border);
  }
  h3 {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .result {
    margin: 0;
    color: var(--ax-text-muted);
  }
</style>
