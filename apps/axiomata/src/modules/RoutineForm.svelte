<!--
  Shared add/edit form for a routine, used by the flip-side "Add routine"
  form (routines-board-settings.svelte) and the inline "Edit" row
  (routines-board.svelte) alike, so the interval picker and target/backend
  fields exist in exactly one place. The caller owns persistence: this
  component only assembles the fields and calls `onSubmit` — it never
  invokes a Tauri command itself.

  The interval picker (`core/routineInterval.ts`) replaces a bare cron text
  field with four friendly presets — minutes / hourly / daily / weekly — plus
  a "custom" escape hatch that shows the raw cron expression for anything the
  presets don't cover. Switching presets carries the schedule's current
  cron forward into "custom" rather than resetting it, so flipping over to
  hand-edit never loses what was already dialled in.
-->
<script lang="ts">
  import { cronToInterval, defaultInterval, describeInterval, intervalToCron, WEEKDAY_NAMES, type RoutineInterval } from "../core/routineInterval";

  export interface RoutineFormFields {
    name: string;
    cronExpr: string;
    targetType: "skill" | "prompt";
    targetValue: string;
    /** `""` means "use the default backend" (`null` on the wire). */
    backend: string;
  }

  let {
    initial,
    submitLabel,
    busy = false,
    onSubmit,
    onCancel,
  }: {
    /** Omit for a fresh "Add routine" form; pass the routine's current
     *  fields to prefill an edit. */
    initial?: RoutineFormFields;
    submitLabel: string;
    busy?: boolean;
    onSubmit: (fields: RoutineFormFields) => void;
    onCancel?: () => void;
  } = $props();

  // `initial` seeds the form once and is deliberately never re-read after —
  // each edit row is a distinct component instance (keyed by routine id in
  // the `{#each}` above), and the Add form is remounted via `{#key}` after a
  // successful submit, so there's no live instance whose `initial` prop
  // could change out from under it.
  // svelte-ignore state_referenced_locally
  let name = $state(initial?.name ?? "");
  // svelte-ignore state_referenced_locally
  let interval = $state<RoutineInterval>(initial ? cronToInterval(initial.cronExpr) : defaultInterval());
  // svelte-ignore state_referenced_locally
  let targetType = $state<"skill" | "prompt">(initial?.targetType ?? "skill");
  // svelte-ignore state_referenced_locally
  let targetValue = $state(initial?.targetValue ?? "");
  // svelte-ignore state_referenced_locally
  let backend = $state(initial?.backend ?? "");

  const KIND_LABELS: Record<RoutineInterval["kind"], string> = {
    minutes: "every N minutes",
    hourly: "every N hours",
    daily: "daily",
    weekly: "weekly",
    custom: "custom (cron)",
  };

  /** Switches the picker to a different shape, seeding it with sensible
   *  defaults — except "custom", which carries the *current* schedule
   *  forward as raw cron so switching over to hand-edit never discards
   *  whatever was already set up. */
  function setKind(kind: RoutineInterval["kind"]) {
    const carriedCron = intervalToCron(interval);
    interval =
      kind === "minutes"
        ? { kind, everyMinutes: 15 }
        : kind === "hourly"
          ? { kind, everyHours: 2, minute: 0 }
          : kind === "daily"
            ? { kind, hour: 9, minute: 0 }
            : kind === "weekly"
              ? { kind, weekday: 1, hour: 9, minute: 0 }
              : { kind, cron: carriedCron };
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    onSubmit({
      name: name.trim(),
      cronExpr: intervalToCron(interval),
      targetType,
      targetValue: targetValue.trim(),
      backend,
    });
  }
</script>

<form onsubmit={submit} class="routine-form">
  <input type="text" placeholder="name" bind:value={name} required />

  <div class="interval-kind">
    <select value={interval.kind} onchange={(e) => setKind((e.currentTarget as HTMLSelectElement).value as RoutineInterval["kind"])}>
      {#each Object.entries(KIND_LABELS) as [kind, label] (kind)}
        <option value={kind}>{label}</option>
      {/each}
    </select>
  </div>

  {#if interval.kind === "minutes"}
    <div class="row">
      <span>every</span>
      <input type="number" min="1" max="59" bind:value={interval.everyMinutes} />
      <span>min</span>
    </div>
  {:else if interval.kind === "hourly"}
    <div class="row">
      <span>every</span>
      <input type="number" min="1" max="23" bind:value={interval.everyHours} />
      <span>h at :</span>
      <input type="number" min="0" max="59" bind:value={interval.minute} />
    </div>
  {:else if interval.kind === "daily"}
    <div class="row">
      <span>at</span>
      <input type="number" min="0" max="23" bind:value={interval.hour} />
      <span>:</span>
      <input type="number" min="0" max="59" bind:value={interval.minute} />
    </div>
  {:else if interval.kind === "weekly"}
    <div class="row">
      <select bind:value={interval.weekday}>
        {#each WEEKDAY_NAMES as wd, i (wd)}<option value={i}>{wd}</option>{/each}
      </select>
      <span>at</span>
      <input type="number" min="0" max="23" bind:value={interval.hour} />
      <span>:</span>
      <input type="number" min="0" max="59" bind:value={interval.minute} />
    </div>
  {:else}
    <input type="text" class="mono" placeholder="0 0 9 * * *  (sec min hour dom mon dow)" bind:value={interval.cron} />
  {/if}
  <p class="preview">{describeInterval(interval)}</p>

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
      <option value="opencode">opencode</option>
      <option value="ollama">ollama</option>
    </select>
    <button type="submit" disabled={busy}>{busy ? "…" : submitLabel}</button>
    {#if onCancel}
      <button type="button" class="cancel" onclick={onCancel}>Cancel</button>
    {/if}
  </div>
</form>

<style>
  .routine-form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }
  .interval-kind select {
    width: 100%;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }
  .row input[type="number"] {
    width: 4em;
  }
  .preview {
    margin: 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
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
  .cancel {
    color: var(--ax-text-muted);
  }
</style>
