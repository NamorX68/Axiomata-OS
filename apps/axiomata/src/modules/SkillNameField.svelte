<!--
  SkillNameField — the "which skill does this tile call" override, shared by
  every skill-backed connector module's settings face (`mail-settings`,
  `calendar-settings`, `reminders-settings`). Each connector already has a
  hardcoded default skill name (`mail-digest`, `calendar-digest`,
  `reminders-digest`) that both its own SOP and its refresh button agree
  on — this field doesn't replace that, it just lets one instance repoint
  itself at a differently-named skill (a renamed copy, an experiment)
  without a code change. Empty means "use the default"; `resolveSkillName`
  (`core/skillRun.ts`) is the read side of this same contract.

  Also carries the shared explanation of *when* a connector module refreshes
  itself at all: once on mount (app start, or the moment it's newly placed),
  on ↻, or via a Routine — never on a repeating timer.
-->
<script lang="ts">
  import type { ModuleContext } from "../core/types";

  let { ctx, defaultName }: { ctx: ModuleContext; defaultName: string } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function setSkillName(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    config.update((c) => ({ ...c, skillName: value || undefined }));
  }
</script>

<label class="field" for="skill-name-{ctx.instanceId}">Skill name</label>
<input
  id="skill-name-{ctx.instanceId}"
  type="text"
  class="mono"
  placeholder={defaultName}
  value={typeof $config.skillName === "string" ? $config.skillName : ""}
  oninput={setSkillName}
/>
<p class="muted small">
  Runs once when this tile first mounts (app start, or right after you place it) and whenever you
  hit ↻ — never on a repeating timer. For a recurring refresh, schedule <code>{defaultName}</code>
  (or whatever you've renamed it to here) as a Routine instead.
</p>

<style>
  .field {
    display: block;
    margin-top: var(--ax-space-1);
  }
  input.mono {
    width: 100%;
    box-sizing: border-box;
    font-family: var(--ax-font-mono);
  }
  .muted {
    color: var(--ax-text-muted);
    margin: 0;
  }
  .small {
    font-size: var(--ax-font-size-sm);
  }
  code {
    font-family: var(--ax-font-mono);
  }
</style>
