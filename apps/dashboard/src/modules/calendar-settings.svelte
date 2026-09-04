<script lang="ts">
  import { CALENDAR_SKILL_NAME } from "../core/calendar";
  import type { ModuleContext } from "../core/types";

  // No per-instance settings yet (nothing here reads/writes config) — `ctx`
  // is required by `ModuleDefinition.settings`'s signature regardless.
  let { ctx: _ctx }: { ctx: ModuleContext } = $props();
</script>

<div class="settings">
  <p class="muted">
    Backed by the <code>{CALENDAR_SKILL_NAME}</code> skill — this tile only ever reads back that
    skill's last run, it never triggers one on a timer.
  </p>
  <p class="muted">
    Run it by hand from the Skills Deck, hit ↻ on this tile's front, or schedule it as a Routine
    for a periodic background refresh.
  </p>
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
