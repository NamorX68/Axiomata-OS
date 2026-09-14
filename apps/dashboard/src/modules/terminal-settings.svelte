<!--
  terminal-settings — on the tile's flip side (Tile.svelte mounts this
  lazily on first flip, same as every other module's `settings` component).

  Checkpoint 5d of docs/plans/terminal.md: every field here reads/writes
  `terminalSettings` (`./terminalSettings.ts`), the Terminal module's global
  preferences store — one shared settings blob for every placed Terminal
  tile, persisted in its own `~/.axiomata/terminal-settings.json`, not
  per-instance `ctx.config`/`dashboard.json` (Checkpoints 5/5b's original
  design, since replaced — see that file's own doc comment for why: closing
  a tile used to discard every setting on it). `ctx` is still used for
  `ctx.invoke`, just not `ctx.config` — this module never reads or writes
  per-instance config at all any more.

  Font size, cursor style/blink, theme, bold-as-bright, custom font family,
  and background opacity apply live — `terminal.svelte` reads them fresh
  every frame/font-resolution, no reopen needed. Shell, scrollback size,
  start directory, environment variables, and the visual bell's on/off
  toggle only take effect the *next* time a Terminal tile spawns a session
  (there's no way to swap them under an already-running process) — the hint
  below says so outright. The start-directory field's placeholder is the
  workspace root (fetched once via `get_app_info` on mount), shown as a
  *suggestion* only — leaving the field empty does not implicitly send the
  workspace root as `cwd`, it sends nothing at all and the backend falls
  back to its own default (see `PtySession::spawn`'s own doc comment).
  Owner decision (Checkpoint 5b): suggest, don't force.

  Bug fix (live-tested by the owner, Checkpoint 5d): every free-text field
  (shell, start directory, environment variables, font family — not the two
  numeric fields, unaffected by the reported bug) is backed by its own
  local `$state` variable (seeded once, when `terminalSettings` finishes
  loading — see `onMount` — never resynced afterward, via `bind:value`)
  rather than binding `value={$terminalSettings.X}` directly — typing into
  the "Start directory" field was found to be blocked/reset live in the
  real app. The exact Svelte-reactivity mechanism wasn't confirmed (it
  didn't clearly reproduce against the Chromium dev-mock, only described
  against the real WKWebView — this codebase has a track record of
  WebKit-only rendering/interaction bugs, see `canvas/Tile.svelte`'s own
  `.tile-body` comment), but decoupling "what the field displays while
  being edited" from "the store's current value" removes the whole *class*
  of bug regardless of the precise cause: a store write elsewhere (this
  component has several) can no longer force a re-render that resets an
  in-progress edit. `onchange` (blur/Enter, not every keystroke) still
  pushes the local value into the store exactly as before. The numeric
  fields, checkboxes, `<select>`s, and the range slider keep reading
  `$terminalSettings` directly — a single click/choice/number-stepper nudge
  has no "mid-edit" typed state to protect the same way.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import type { AppInfo } from "../core/backend";
  import type { ModuleContext } from "../core/types";
  import { DEFAULT_CURSOR_STYLE } from "./TerminalScreen";
  import { DEFAULT_THEME, THEMES } from "./terminalThemes";
  import { ensureTerminalSettingsLoaded, terminalSettings } from "./terminalSettings";

  let { ctx }: { ctx: ModuleContext } = $props();

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
    "catppuccin-mocha": "Catppuccin Mocha",
    "tokyo-night": "Tokyo Night",
  };
  const themeNames = Object.keys(THEMES);

  /** Bundled coding-monospace families (Checkpoint 5g, owner request: "wir
   *  sollten einige MonoFonts... bereits mitliefern"). Real `@fontsource`
   *  family names, matching the static weights bundled in `main.ts`
   *  (100/400/700 → Thin/Regular/Bold — see that import's own comment for
   *  why 100 is bundled even though nothing renders at it yet: this page
   *  only picks a font *family* here, never a weight), so picking one of
   *  these always has a real Bold face to render with, not just a
   *  browser-synthesized fake bold. This `<select>` is a one-click shortcut
   *  into the "Font family" free-text field right below, not a separate
   *  setting — picking a locally installed font (or clearing it back to the
   *  theme default) still goes through that same field. */
  const BUNDLED_FONTS = ["JetBrains Mono", "IBM Plex Mono"];

  function pickBundledFont(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    if (!value) return; // "Custom…" placeholder — leave the text field as-is
    fontFamilyText = value;
    setFontFamily();
  }

  /** The workspace root, fetched once on mount purely to *suggest* a start
   *  directory (see the component doc comment) — never written into
   *  `terminalSettings.cwd` itself. */
  let workspaceRootHint = $state("");
  /** The theme's own default font size in px, read once from the resolved
   *  `--ax-font-size-sm` token so the font-size field's placeholder shows a
   *  real number (e.g. "12") instead of the text "theme default" — owner
   *  feedback: the placeholder text read as broken/confusing inside a
   *  `type="number"` field, not as a helpful hint. Falls back to the text
   *  form only if the token can't be resolved for some reason. */
  let fontSizeDefaultHint = $state("theme default");
  /** `true` once `terminalSettings` has actually loaded — gates the form
   *  behind a brief "Loading…" (see the template) so the free-text fields'
   *  local `$state` (below) gets seeded from the *real* loaded values, not
   *  the store's empty pre-load default; without this gate they'd flash
   *  empty and then never pick up the real values at all (the whole point
   *  of not resyncing them from the store after the initial seed — see the
   *  component doc comment's "Bug fix" paragraph). */
  let settingsReady = $state(false);

  // Local editable state for the free-text (not numeric — those two kept
  // their original `value={...} onchange={...}` pattern, unaffected by the
  // reported bug) fields — see the component doc comment's "Bug fix"
  // paragraph. Seeded once in `onMount`, after `terminalSettings` has
  // actually loaded (declared empty here; the template doesn't render
  // these fields until `settingsReady`, so this placeholder value is never
  // actually shown).
  let shellText = $state("");
  let cwdText = $state("");
  let envText = $state("");
  let fontFamilyText = $state("");

  function setFontSize(e: Event) {
    const raw = (e.currentTarget as HTMLInputElement).value.trim();
    // Empty means "unset" - `terminal.svelte` falls back to the theme's own
    // `--ax-font-size-sm` whenever `fontSizePx` isn't a number, so storing
    // `undefined` here (dropped by JSON.stringify, so it's simply absent
    // from the persisted terminal-settings.json) is enough, no separate
    // sentinel.
    const px = raw ? Math.min(MAX_FONT_PX, Math.max(MIN_FONT_PX, Number(raw))) : undefined;
    terminalSettings.update((c) => ({ ...c, fontSizePx: px }));
  }

  function setShell() {
    const value = shellText.trim();
    terminalSettings.update((c) => ({ ...c, shell: value || undefined }));
  }

  function setCwd() {
    const value = cwdText.trim();
    terminalSettings.update((c) => ({ ...c, cwd: value || undefined }));
  }

  function setEnv() {
    terminalSettings.update((c) => ({ ...c, env: envText || undefined }));
  }

  function setScrollbackLimit(e: Event) {
    const raw = (e.currentTarget as HTMLInputElement).value.trim();
    const limit = raw ? Math.max(0, Math.floor(Number(raw))) : undefined;
    terminalSettings.update((c) => ({ ...c, scrollbackLimit: limit }));
  }

  function setCursorStyle(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    // `DEFAULT_CURSOR_STYLE` is what the frontend already falls back to
    // (`currentCursorStyle()`) when the field is unset, so storing it
    // explicitly would just be redundant persisted state.
    terminalSettings.update((c) => ({ ...c, cursorStyle: value === DEFAULT_CURSOR_STYLE ? undefined : value }));
  }

  function setCursorBlink(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    terminalSettings.update((c) => ({ ...c, cursorBlink: checked ? undefined : false }));
  }

  function setTheme(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    terminalSettings.update((c) => ({ ...c, theme: value === DEFAULT_THEME ? undefined : value }));
  }

  function setBoldIsBright(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    terminalSettings.update((c) => ({ ...c, boldIsBright: checked ? undefined : false }));
  }

  function setBellEnabled(e: Event) {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    terminalSettings.update((c) => ({ ...c, bellEnabled: checked ? undefined : false }));
  }

  function setFontFamily() {
    const value = fontFamilyText.trim();
    terminalSettings.update((c) => ({ ...c, fontFamily: value || undefined }));
  }

  function setOpacity(e: Event) {
    const raw = Number((e.currentTarget as HTMLInputElement).value);
    const opacity = Math.min(100, Math.max(0, raw));
    terminalSettings.update((c) => ({ ...c, opacity: opacity === 100 ? undefined : opacity }));
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

    // Read the real resolved default once, off the document root (same
    // technique `graph/model.ts`'s `readPalette()` already uses) — this
    // component's own DOM has no canvas of its own to measure against, but
    // the CSS custom property resolves identically off any element.
    const px = getComputedStyle(document.documentElement).getPropertyValue("--ax-font-size-sm").trim();
    if (px) fontSizeDefaultHint = px;

    void ensureTerminalSettingsLoaded().then(() => {
      const s = get(terminalSettings);
      shellText = typeof s.shell === "string" ? s.shell : "";
      cwdText = typeof s.cwd === "string" ? s.cwd : "";
      envText = typeof s.env === "string" ? s.env : "";
      fontFamilyText = typeof s.fontFamily === "string" ? s.fontFamily : "";
      settingsReady = true;
    });
  });
</script>

{#if !settingsReady}
  <p class="muted">Loading…</p>
{:else}
  <div class="settings">
    <label>
      Font size (px)
      <input
        type="number"
        min={MIN_FONT_PX}
        max={MAX_FONT_PX}
        placeholder={fontSizeDefaultHint}
        value={typeof $terminalSettings.fontSizePx === "number" ? $terminalSettings.fontSizePx : ""}
        onchange={setFontSize}
      />
    </label>
    <label>
      Shell
      <input type="text" placeholder="$SHELL" bind:value={shellText} onchange={setShell} />
    </label>
    <label>
      Scrollback (lines)
      <input
        type="number"
        min="0"
        placeholder="2000"
        value={typeof $terminalSettings.scrollbackLimit === "number" ? $terminalSettings.scrollbackLimit : ""}
        onchange={setScrollbackLimit}
      />
    </label>
    <label>
      Start directory
      <input type="text" placeholder={workspaceRootHint || "app default"} bind:value={cwdText} onchange={setCwd} />
    </label>
    <label class="stacked">
      Environment variables
      <textarea rows="3" placeholder={"KEY=value\nANOTHER=value"} bind:value={envText} onchange={setEnv}></textarea>
    </label>
    <p class="hint">
      Shell/scrollback/start directory/environment variables/visual bell apply to the next terminal spawned in any
      tile — not the one currently running.
    </p>

    <div class="divider"></div>

    <label>
      Cursor style
      <select
        value={typeof $terminalSettings.cursorStyle === "string" ? $terminalSettings.cursorStyle : DEFAULT_CURSOR_STYLE}
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
      <input type="checkbox" checked={$terminalSettings.cursorBlink !== false} onchange={setCursorBlink} />
    </label>
    <label>
      Theme
      <select value={typeof $terminalSettings.theme === "string" ? $terminalSettings.theme : DEFAULT_THEME} onchange={setTheme}>
        {#each themeNames as name (name)}
          <option value={name}>{THEME_LABELS[name] ?? name}</option>
        {/each}
      </select>
    </label>
    <label>
      Bold text in bright colour
      <input type="checkbox" checked={$terminalSettings.boldIsBright !== false} onchange={setBoldIsBright} />
    </label>
    <label>
      Visual bell
      <input type="checkbox" checked={$terminalSettings.bellEnabled !== false} onchange={setBellEnabled} />
    </label>
    <label>
      Bundled font
      <select value={BUNDLED_FONTS.includes(fontFamilyText) ? fontFamilyText : ""} onchange={pickBundledFont}>
        <option value="">Custom…</option>
        {#each BUNDLED_FONTS as name (name)}
          <option value={name}>{name}</option>
        {/each}
      </select>
    </label>
    <label>
      Font family
      <input type="text" placeholder="theme default" bind:value={fontFamilyText} onchange={setFontFamily} />
    </label>
    <label>
      Background opacity
      <input
        type="range"
        min="0"
        max="100"
        value={typeof $terminalSettings.opacity === "number" ? $terminalSettings.opacity : 100}
        onchange={setOpacity}
      />
    </label>
    <p class="hint">
      These apply live to every open terminal — no reopen or new session needed. All settings on this page are
      shared by every Terminal tile and saved to terminal-settings.json, not to this one tile.
    </p>
  </div>
{/if}

<style>
  .muted {
    padding: var(--ax-space-3);
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
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
