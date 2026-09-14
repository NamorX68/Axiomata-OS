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

  Checkpoint 5b added three more next-session-only settings, all read the
  same way `terminal.svelte`'s `spawn()` already reads `config.shell`:
  scrollback size, start directory, and extra environment variables (a
  plain `KEY=value`-per-line textarea, parsed by `terminalEnv.parseEnvLines`
  — no structured table, not worth the effort for one Terminal module).
  The start-directory field's placeholder is the workspace root (fetched
  once via `get_app_info` on mount), shown as a *suggestion* only — leaving
  the field empty does not implicitly send the workspace root as `cwd`, it
  sends nothing at all and the backend falls back to its own default (see
  `PtySession::spawn`'s own doc comment). Owner decision (`docs/plans/terminal.md`,
  Checkpoint 5b): suggest, don't force.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import type { AppInfo } from "../core/backend";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  const MIN_FONT_PX = 8;
  const MAX_FONT_PX = 32;

  /** The workspace root, fetched once on mount purely to *suggest* a start
   *  directory (see the component doc comment) — never written into
   *  `config.cwd` itself. */
  let workspaceRootHint = $state("");

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

  function setCwd(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    config.update((c) => ({ ...c, cwd: value || undefined }));
  }

  function setEnv(e: Event) {
    const value = (e.currentTarget as HTMLTextAreaElement).value;
    config.update((c) => ({ ...c, env: value || undefined }));
  }

  function setScrollbackLimit(e: Event) {
    const raw = (e.currentTarget as HTMLInputElement).value.trim();
    const limit = raw ? Math.max(0, Math.floor(Number(raw))) : undefined;
    config.update((c) => ({ ...c, scrollbackLimit: limit }));
  }

  onMount(() => {
    ctx
      .invoke<AppInfo>("get_app_info")
      .then((info) => {
        workspaceRootHint = info.workspace_root;
      })
      .catch(() => {
        // No suggestion shown if this fails — the field just falls back to
        // its own generic placeholder below, nothing else depends on this.
      });
  });
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
  <label>
    Scrollback (lines)
    <input
      type="number"
      min="0"
      placeholder="2000"
      value={typeof $config.scrollbackLimit === "number" ? $config.scrollbackLimit : ""}
      onchange={setScrollbackLimit}
    />
  </label>
  <label>
    Start directory
    <input
      type="text"
      placeholder={workspaceRootHint || "app default"}
      value={typeof $config.cwd === "string" ? $config.cwd : ""}
      onchange={setCwd}
    />
  </label>
  <label class="stacked">
    Environment variables
    <textarea
      rows="3"
      placeholder={"KEY=value\nANOTHER=value"}
      value={typeof $config.env === "string" ? $config.env : ""}
      onchange={setEnv}
    ></textarea>
  </label>
  <p class="hint">
    None of these apply to the terminal that's currently running — they take effect the next time this tile spawns a
    new session.
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
  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
  }
  /* The env-vars textarea needs its own line, not squeezed next to its
   *  label the way every single-line input above it is. */
  label.stacked {
    flex-direction: column;
    align-items: stretch;
  }
  input {
    width: 9em;
  }
  textarea {
    width: 100%;
    resize: vertical;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    box-sizing: border-box;
  }
  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
