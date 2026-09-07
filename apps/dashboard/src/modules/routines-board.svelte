<!--
  routines-board — the M3 scheduler as a TIME | ROUTINE | STATUS board (per
  the reference screenshots). The soonest enabled routine is "next", the
  rest "queued", disabled ones "off". Toggle per row. Polls every POLL_MS.
  Config (flip side): `showDisabled`, `sort` ("next" | "name"); the flip side
  also hosts the add-routine form.

  Edit (✎) and delete (✕) live here on the front, not the flip side: a
  module has no way to flip its own tile from inside itself (see
  `core/types.ts` `ModuleContext` — no such handle is exposed), so an
  editable row expands in place into the shared `RoutineForm` instead of
  reusing the flip-side "Add" form. Delete fires immediately on click, same
  no-confirmation-dialog convention as Calendar's/Reminders' hover ✕
  (`modules/calendar.svelte`, `modules/reminders.svelte`) — nothing else in
  this app uses a native confirm() dialog either.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { NewRoutine, Routine } from "../core/backend";
  import { relativeTime, untilTime } from "../core/format";
  import type { ModuleContext } from "../core/types";
  import RoutineForm, { type RoutineFormFields } from "./RoutineForm.svelte";

  const POLL_MS = 5000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let routines = $state<Routine[]>([]);
  let error = $state("");
  let toggling = $state<number[]>([]);
  let deleting = $state<number[]>([]);
  // At most one row editing at a time (id, or null when idle).
  let editingId = $state<number | null>(null);
  let saving = $state(false);

  const showDisabled = $derived($config.showDisabled !== false);
  const sort = $derived($config.sort === "name" ? "name" : "next");

  const nextId = $derived.by(() => {
    const due = routines.filter((r) => r.enabled && r.next_fire_at);
    if (due.length === 0) return null;
    return due.reduce((a, b) => (Date.parse(a.next_fire_at!) <= Date.parse(b.next_fire_at!) ? a : b)).id;
  });

  const visible = $derived.by(() => {
    const list = routines.filter((r) => showDisabled || r.enabled);
    if (sort === "name") return [...list].sort((a, b) => a.name.localeCompare(b.name));
    const t = (r: Routine) => (r.enabled && r.next_fire_at ? Date.parse(r.next_fire_at) : Infinity);
    return [...list].sort((a, b) => t(a) - t(b) || a.name.localeCompare(b.name));
  });

  function statusOf(r: Routine): "next" | "queued" | "off" {
    if (!r.enabled) return "off";
    return r.id === nextId ? "next" : "queued";
  }

  async function refresh() {
    try {
      routines = await ctx.invoke<Routine[]>("list_routines");
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  async function toggle(r: Routine) {
    if (toggling.includes(r.id)) return;
    toggling = [...toggling, r.id];
    try {
      await ctx.invoke<boolean>("set_routine_enabled", { id: r.id, enabled: !r.enabled });
    } catch (err) {
      error = `toggle #${r.id}: ${String(err)}`;
    } finally {
      toggling = toggling.filter((id) => id !== r.id);
      await refresh();
    }
  }

  async function remove(r: Routine) {
    if (deleting.includes(r.id)) return;
    deleting = [...deleting, r.id];
    try {
      await ctx.invoke<boolean>("delete_routine", { id: r.id });
    } catch (err) {
      error = `delete #${r.id}: ${String(err)}`;
    } finally {
      deleting = deleting.filter((id) => id !== r.id);
      await refresh();
    }
  }

  async function saveEdit(r: Routine, fields: RoutineFormFields) {
    if (saving) return;
    saving = true;
    const updated: NewRoutine = {
      name: fields.name,
      cron_expr: fields.cronExpr,
      target: { type: fields.targetType, value: fields.targetValue },
      backend: fields.backend === "" ? null : fields.backend,
      // Editing never changes the on/off state — that's the separate toggle.
      enabled: r.enabled,
    };
    try {
      await ctx.invoke("update_routine", { id: r.id, new: updated });
      editingId = null;
    } catch (err) {
      error = `edit #${r.id}: ${String(err)}`;
    } finally {
      saving = false;
      await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="board">
  {#if error}
    <p class="error">{error}</p>
  {/if}
  {#if visible.length === 0 && !error}
    <p class="muted">{routines.length === 0 ? "No routines yet — add one on the flip side." : "All routines are disabled."}</p>
  {:else}
    <table>
      <thead>
        <tr><th>Time</th><th>Routine</th><th>Status</th><th></th></tr>
      </thead>
      <tbody>
        {#each visible as r (r.id)}
          {@const st = statusOf(r)}
          <tr class={st} title="{r.cron_expr} · {r.target.type}: {r.target.value} · last {relativeTime(r.last_fired_at)}">
            <td class="time">{r.enabled ? untilTime(r.next_fire_at) : "—"}</td>
            <td class="name">
              {r.name}
              <span class="target">{r.target.type === "skill" ? `/${r.target.value}` : "prompt"}</span>
            </td>
            <td class="status"><span class="pill {st}">{st}</span></td>
            <td class="act">
              <button
                type="button"
                class="edit"
                aria-label={`Edit ${r.name}`}
                title="Edit"
                onclick={() => (editingId = editingId === r.id ? null : r.id)}
              >
                ✎
              </button>
              <button
                type="button"
                class="del"
                disabled={deleting.includes(r.id)}
                aria-label={`Delete ${r.name}`}
                title="Delete"
                onclick={() => void remove(r)}
              >
                {deleting.includes(r.id) ? "…" : "✕"}
              </button>
              <button
                type="button"
                disabled={toggling.includes(r.id)}
                aria-label={r.enabled ? `Disable ${r.name}` : `Enable ${r.name}`}
                title={r.enabled ? "Disable" : "Enable"}
                onclick={() => toggle(r)}
              >
                {r.enabled ? "on" : "off"}
              </button>
            </td>
          </tr>
          {#if editingId === r.id}
            <tr class="edit-row">
              <td colspan="4">
                <RoutineForm
                  initial={{
                    name: r.name,
                    cronExpr: r.cron_expr,
                    targetType: r.target.type,
                    targetValue: r.target.value,
                    backend: r.backend ?? '',
                  }}
                  submitLabel={saving ? 'Saving…' : 'Save'}
                  busy={saving}
                  onSubmit={(fields) => void saveEdit(r, fields)}
                  onCancel={() => (editingId = null)}
                />
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .board {
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-3);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    text-align: left;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
    border-bottom: 1px solid var(--ax-border);
  }
  td {
    padding: var(--ax-space-1) var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
    vertical-align: middle;
    white-space: nowrap;
  }
  tr:last-child td {
    border-bottom: none;
  }
  tr.off td {
    color: var(--ax-text-muted);
  }
  tr.edit-row td {
    white-space: normal;
    background: var(--ax-surface-2);
  }
  tr.edit-row:last-child td {
    border-bottom: none;
  }

  .time {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-accent);
  }
  tr.off .time {
    color: var(--ax-text-muted);
  }
  .name {
    width: 100%;
    white-space: normal;
    font-weight: 600;
  }
  .target {
    margin-left: var(--ax-space-2);
    font-weight: 400;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }

  .pill {
    display: inline-block;
    padding: 1px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-sm);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ax-text-muted);
    background: var(--ax-surface-3);
  }
  .pill.next {
    color: var(--ax-text-invert);
    background: var(--ax-accent);
  }
  .pill.queued {
    color: var(--ax-text);
  }

  .act {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }
  .act button {
    min-width: 40px;
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .act .edit,
  .act .del {
    min-width: 0;
    color: var(--ax-text-muted);
    opacity: 0;
  }
  tr:hover .edit,
  tr:hover .del,
  .edit:focus-visible,
  .del:focus-visible {
    opacity: 1;
  }

  p {
    margin: 0;
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-2);
  }
</style>
