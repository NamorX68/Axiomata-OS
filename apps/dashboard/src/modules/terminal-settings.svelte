<!--
  terminal-settings — Checkpoint 5's per-instance config, on the tile's flip
  side (Tile.svelte mounts this lazily on first flip, same as every other
  module's `settings` component). Two fields, per docs/plans/terminal.md's
  own Checkpoint 5 scope: font size and shell choice.

  Font size applies live — `terminal.svelte` watches `$config.fontSizePx`
  and re-measures/redraws immediately, no reopen needed. Shell choice only
  takes effect for the *next* spawned session (there's no way to swap the
  shell under an already-running process), which the hint below says
  outright rather than leaving it to be discovered by trying it.
-->
<script lang="ts">
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  const MIN_FONT_PX = 8;
  const MAX_FONT_PX = 32;

  function setFontSize(e: Event) {
    const raw = (e.currentTarget as HTMLInputElement).value.trim();
    // Empty means "unset" - `terminal.svelte` falls back to the theme's own
    // `--ax-font-size-sm` whenever `fontSizePx` isn't a number, so storing
    // `undefined` here (dropped by JSON.stringify, so it's simply absent
    // from the persisted dashboard.json) is enough, no separate sentinel.
    const px = raw ? Math.min(MAX_FONT_PX, Math.max(MIN_FONT_PX, Number(raw))) : undefined;
    config.update((c) => ({ ...c, fontSizePx: px }));
  }

  function setShell(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    config.update((c) => ({ ...c, shell: value || undefined }));
  }
</script>

<div class="settings">
  <label>
    Font size (px)
    <input
      type="number"
      min={MIN_FONT_PX}
      max={MAX_FONT_PX}
      placeholder="theme default"
      value={typeof $config.fontSizePx === "number" ? $config.fontSizePx : ""}
      onchange={setFontSize}
    />
  </label>
  <label>
    Shell
    <input type="text" placeholder="$SHELL" value={typeof $config.shell === "string" ? $config.shell : ""} onchange={setShell} />
  </label>
  <p class="hint">A shell change only applies the next time this terminal is opened (the current session keeps running as-is).</p>
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
  }
  input {
    width: 9em;
  }
  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
