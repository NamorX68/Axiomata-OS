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

  Checkpoint 5b, Block B added six purely visual settings, unlike the
  section above all *live*-applied (`terminal.svelte` reads them fresh
  every frame/font-resolution, same as `fontSizePx` already was — no
  reopen or new session needed): cursor style + blink, a named colour
  theme, bold-as-bright, a custom font family, background opacity, and the
  visual bell's on/off toggle.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import type { AppInfo } from "../core/backend";
  import type { ModuleContext } from "../core/types";
  import { DEFAULT_CURSOR_STYLE } from "./TerminalScreen";
  import { DEFAULT_THEME, THEMES } from "./terminalThemes";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  const MIN_FONT_PX = 8;
  const MAX_FONT_PX = 32;

  /** Friendly display names for `THEMES`' keys, in the order the `<select>`
   *  below lists them — driven off `THEMES` itself (not a separately
   *  hand-maintained list) so a palette can't exist in one place but not
   *  the other. */
  const THEME_LABELS: Record<string, string> = {
    xterm: "xterm (default)",
    "solarized-dark": "Solarized Dark",
    dracula: "Dracula",
    nord: "Nord",
    "gruvbox-dark": "Gruvbox Dark",
  };
  const themeNames = Object.keys(THEMES);

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

  function setCursorStyle(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    // `DEFAULT_CURSOR_STYLE` is what the frontend already falls back to
    // (`currentCursorStyle()`) when the field is unset, so storing it
    // explicitly would just be redundant persisted state.
    config.update((c) => ({ ...c, cursorStyle: value === DEFAULT_CURSOR_STYLE ? undefined : value }));
  }

  function setCursorBlink(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, cursorBlink: checked ? undefined : false }));
  }

  function setTheme(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    config.update((c) => ({ ...c, theme: value === DEFAULT_THEME ? undefined : value }));
  }

  function setBoldIsBright(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, boldIsBright: checked ? undefined : false }));
  }

  function setBellEnabled(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    config.update((c) => ({ ...c, bellEnabled: checked ? undefined : false }));
  }

  function setFontFamily(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value.trim();
    config.update((c) => ({ ...c, fontFamily: value || undefined }));
  }

  function setOpacity(e: Event) {
    const raw = Number((e.currentTarget as HTMLInputElement).value);
    const opacity = Math.min(100, Math.max(0, raw));
    config.update((c) => ({ ...c, opacity: opacity === 100 ? undefined : opacity }));
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

  <div class="divider"></div>

  <label>
    Cursor style
    <select
      value={typeof $config.cursorStyle === "string" ? $config.cursorStyle : DEFAULT_CURSOR_STYLE}
      onchange={setCursorStyle}
    >
      <option value="block">Block</option>
      <option value="outline">Outline</option>
      <option value="underline">Underline</option>
      <option value="bar">Bar</option>
    </select>
  </label>
  <label>
    Cursor blink
    <input type="checkbox" checked={$config.cursorBlink !== false} onchange={setCursorBlink} />
  </label>
  <label>
    Theme
    <select value={typeof $config.theme === "string" ? $config.theme : DEFAULT_THEME} onchange={setTheme}>
      {#each themeNames as name (name)}
        <option value={name}>{THEME_LABELS[name] ?? name}</option>
      {/each}
    </select>
  </label>
  <label>
    Bold text in bright colour
    <input type="checkbox" checked={$config.boldIsBright !== false} onchange={setBoldIsBright} />
  </label>
  <label>
    Visual bell
    <input type="checkbox" checked={$config.bellEnabled !== false} onchange={setBellEnabled} />
  </label>
  <label>
    Font family
    <input
      type="text"
      placeholder="theme default"
      value={typeof $config.fontFamily === "string" ? $config.fontFamily : ""}
      onchange={setFontFamily}
    />
  </label>
  <label>
    Background opacity
    <input
      type="range"
      min="0"
      max="100"
      value={typeof $config.opacity === "number" ? $config.opacity : 100}
      onchange={setOpacity}
    />
  </label>
  <p class="hint">
    These apply live to the terminal that's currently running — no reopen or new session needed.
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
  input,
  select {
    width: 9em;
  }
  /* A checkbox/range shouldn't take the same fixed 9em box a text/number/
   *  select field does — that would stretch a checkbox's hit area oddly and
   *  give a slider far more room than it needs next to a short label. */
  input[type="checkbox"] {
    width: auto;
  }
  input[type="range"] {
    width: 8em;
  }
  textarea {
    width: 100%;
    resize: vertical;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    box-sizing: border-box;
  }
  .divider {
    height: 1px;
    background: var(--ax-border);
    margin: var(--ax-space-1) 0;
  }
  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
