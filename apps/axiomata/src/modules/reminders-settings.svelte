<script lang="ts">
  import { REMINDERS_SKILL_NAME } from "../core/reminders";
  import { resolveSkillName } from "../core/skillRun";
  import type { ModuleContext } from "../core/types";
  import SkillNameField from "./SkillNameField.svelte";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;
  const skillName = $derived(resolveSkillName($config, REMINDERS_SKILL_NAME));
</script>

<div class="settings">
  <p class="muted">
    Currently calling <code>{skillName}</code>.
  </p>
  <SkillNameField {ctx} defaultName={REMINDERS_SKILL_NAME} />
  <p class="muted">There's no "all lists" view — pick one list on the front; that choice is remembered.</p>
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
</style>
