<script lang="ts">
  import { CALENDAR_SKILL_NAME } from "../core/calendar";
  import { resolveSkillName } from "../core/skillRun";
  import type { ModuleContext } from "../core/types";
  import SkillNameField from "./SkillNameField.svelte";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;
  const skillName = $derived(resolveSkillName($config, CALENDAR_SKILL_NAME));

  const showClock = $derived($config.showClock === true);
  const clockStyle = $derived($config.clockStyle === "analog" ? "analog" : "digital");
  const agendaDays = $derived(
    Math.min(14, Math.max(1, typeof $config.agendaDays === "number" ? Math.floor($config.agendaDays) : 5)),
  );

  function set(key: string, value: unknown) {
    config.update((c) => ({ ...c, [key]: value }));
  }
</script>

<div class="settings">
  <p class="muted">
    Currently calling <code>{skillName}</code>.
  </p>
  <SkillNameField {ctx} defaultName={CALENDAR_SKILL_NAME} />

  <label class="check">
    Tage in der Agenda
    <input
      type="number"
      min="1"
      max="14"
      value={agendaDays}
      onchange={(e) => set("agendaDays", Math.min(14, Math.max(1, Math.floor(e.currentTarget.valueAsNumber || 5))))}
    />
  </label>

  <label class="check">
    Uhr neben dem Kalender
    <input type="checkbox" checked={showClock} onchange={(e) => set("showClock", e.currentTarget.checked)} />
  </label>
  {#if showClock}
    <div class="radios" role="radiogroup" aria-label="Uhr-Stil">
      <label>
        <input
          type="radio"
          name="clockStyle-{ctx.instanceId}"
          checked={clockStyle === "digital"}
          onchange={() => set("clockStyle", "digital")}
        /> Digital
      </label>
      <label>
        <input
          type="radio"
          name="clockStyle-{ctx.instanceId}"
          checked={clockStyle === "analog"}
          onchange={() => set("clockStyle", "analog")}
        /> Analog
      </label>
    </div>
  {/if}
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .muted {
    color: var(--ax-text-muted);
    margin: 0;
  }
  code {
    font-family: var(--ax-font-mono);
  }
  .check {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
  }
  .check input[type="number"] {
    width: 4rem;
  }
  .radios {
    display: flex;
    gap: var(--ax-space-3);
    padding-left: var(--ax-space-2);
  }
  .radios label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }
  input[type="checkbox"],
  input[type="radio"] {
    accent-color: var(--ax-accent);
  }
</style>
