<!--
  The editor's settings (`docs/plans/editor.md`, D6, F12, F13), the Settings tab
  of the file app's inspector (`Inspector.svelte`, editor-look LK1). Every change
  applies at once and is saved; the preview at the top is a real, read-only
  editor surface on sample code, so what it shows is exactly what the editor
  will do.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import { EditorDocument } from "../editor/document";
  import type { SyntaxHighlighter } from "../editor/syntax/highlighter";
  import EditorSurface from "./EditorSurface.svelte";
  import {
    editorSettings,
    languageList,
    updateEditorSettings,
    type Autosave,
    type CursorAnimation,
    type EditorSettings,
  } from "./editorSettings";
  import { ensureInstalledFonts, installedFonts } from "../core/installedFonts";
  import { DEFAULT_FONT_FAMILY, drawnFamily, EDITOR_FONTS, nearestWeight, realWeights, weightName } from "./fonts";
  import { highlightFor } from "./highlighting";
  import { FILE_ICON_STYLE_NAMES, FILE_ICON_STYLES, type FileIconStyle } from "../core/fileIcons";
  import type { SurfaceSettings } from "./surfaceSettings";

  interface Props {
    /** The surface settings the editor uses right now (font already loaded). */
    surface: SurfaceSettings;
  }

  let { surface }: Props = $props();

  const SAMPLE = [
    "// Live preview — every change shows here first.",
    "function greet(name: string): string {",
    "    const message = `Hallo, ${name}! Größe: äöü ß`;",
    "    if (message.length >= 10 && name !== \"\") return message; // -> => != <= >=",
    "\treturn \"a tab-indented line\";",
    "}",
    "",
    "A longer line of prose that wraps when wrapping is on, so you can see how continuation rows keep their indent.",
  ].join("\n");

  const preview = new EditorDocument(SAMPLE, { indentFallback: { kind: "spaces", size: 4 } });

  // The preview is coloured like a real TypeScript file, so theme colours show too.
  let highlighter = $state.raw<SyntaxHighlighter | null>(null);
  let destroyed = false;
  void highlightFor(preview, { language: "typescript", stale: () => destroyed }).then((h) => {
    if (h) highlighter = h;
  });
  onDestroy(() => {
    destroyed = true;
    highlighter?.dispose();
  });

  const s = $derived($editorSettings);
  // Both depend on the installed list too (an installed family's weights), which arrives later.
  const weights = $derived((void $installedFonts, realWeights(s.fontFamily)));
  const shownWeight = $derived((void $installedFonts, nearestWeight(s.fontFamily, s.fontWeight)));

  void ensureInstalledFonts();
  /** The installed fonts offered: monospaced ones only unless this is unticked (T10). */
  let monospaceOnly = $state(true);
  const bundledNames = new Set(EDITOR_FONTS.map((f) => f.family));
  const installedChoices = $derived(
    $installedFonts.fonts.filter(
      (f) => !bundledNames.has(f.family) && (!monospaceOnly || f.monospace || f.family === s.fontFamily),
    ),
  );
  /** The chosen family is not on this Mac (any more): the editor draws the default. */
  const missing = $derived(drawnFamily(s.fontFamily, $installedFonts.loaded) !== s.fontFamily);

  const CURSOR: { value: CursorAnimation; label: string }[] = [
    { value: "strong", label: "Strong (smear, longer on long jumps)" },
    { value: "subtle", label: "Subtle" },
    { value: "off", label: "Off" },
  ];

  /** The on/off effects of G7, each its own switch. */
  const EFFECT_SWITCHES: { key: keyof EditorSettings; label: string }[] = [
    { key: "smoothScroll", label: "Smooth scrolling" },
    { key: "currentLine", label: "Highlight current line" },
    { key: "indentGuides", label: "Indentation guides" },
    { key: "bracketColors", label: "Coloured bracket pairs" },
    { key: "glow", label: "Accent glow" },
    { key: "foldAnimation", label: "Animated folding" },
    { key: "minimap", label: "Minimap" },
    { key: "stickyScroll", label: "Sticky scroll" },
    { key: "diagnosticsInline", label: "Problem message at line end" },
    { key: "tabColors", label: "Tabs coloured by language" },
  ];

  const AUTOSAVE: { value: Autosave; label: string }[] = [
    { value: "off", label: "Off" },
    { value: "delay", label: "After 1 s pause" },
    { value: "leave", label: "When leaving the editor" },
  ];
</script>

<div class="panel" aria-label="Editor settings">

  <div class="preview">
    <EditorSurface
      doc={preview}
      settings={{ ...surface, wrap: s.wrapCode, minimap: false, stickyScroll: false }}
      fileName="preview.ts"
      readOnly
      {highlighter}
    />
  </div>

  <div class="fields">
    <label>
      <span>Keys</span>
      <select
        value={s.mode}
        onchange={(e) => updateEditorSettings({ mode: e.currentTarget.value as "normal" | "vi" })}
      >
        <option value="normal">Normal (Mac)</option>
        <option value="vi">Vi</option>
      </select>
    </label>

    {#if s.mode === "vi"}
      <label>
        <span>Vi clipboard</span>
        <select
          value={s.viClipboard}
          onchange={(e) => updateEditorSettings({ viClipboard: e.currentTarget.value as "shared" | "separate" })}
        >
          <option value="shared">Shared with the Mac (y and p use ⌘C/⌘V's clipboard)</option>
          <option value="separate">Separate ("+y and "+p reach the Mac)</option>
        </select>
      </label>
    {/if}

    <label>
      <span>Font</span>
      <select value={s.fontFamily} onchange={(e) => updateEditorSettings({ fontFamily: e.currentTarget.value })}>
        <optgroup label="Bundled">
          {#each EDITOR_FONTS as font (font.family)}
            <option value={font.family}>{font.family}</option>
          {/each}
        </optgroup>
        {#if installedChoices.length > 0}
          <optgroup label="Installed on this Mac">
            {#each installedChoices as font (font.family)}
              <option value={font.family}>{font.family}</option>
            {/each}
          </optgroup>
        {/if}
        {#if missing}
          <option value={s.fontFamily}>{s.fontFamily} (not installed)</option>
        {/if}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={monospaceOnly} />
      <span>Only monospaced fonts</span>
    </label>
    {#if missing}
      <p class="note">{s.fontFamily} is not installed on this Mac — the editor shows {DEFAULT_FONT_FAMILY}.</p>
    {/if}

    <label>
      <span>Weight</span>
      <input
        type="range"
        min="100"
        max="900"
        step="100"
        value={s.fontWeight}
        oninput={(e) => updateEditorSettings({ fontWeight: Number(e.currentTarget.value) })}
      />
      <output>{weightName(s.fontWeight)}</output>
    </label>
    {#if shownWeight !== s.fontWeight}
      <p class="note">
        {s.fontFamily} has no {weightName(s.fontWeight).split(" ")[0]} — shows {weightName(shownWeight)}. It has
        {weights.map((w) => weightName(w).split(" ")[0]).join(", ")}.
      </p>
    {/if}

    <label>
      <span>Size</span>
      <input
        type="number"
        min="9"
        max="32"
        value={s.fontSize}
        onchange={(e) => updateEditorSettings({ fontSize: Number(e.currentTarget.value) })}
      />
      <output>px</output>
    </label>

    <label>
      <span>Line height</span>
      <input
        type="range"
        min="1"
        max="2.5"
        step="0.05"
        value={s.lineHeight}
        oninput={(e) => updateEditorSettings({ lineHeight: Number(e.currentTarget.value) })}
      />
      <output>{s.lineHeight.toFixed(2)}</output>
    </label>

    <label class="check">
      <input
        type="checkbox"
        checked={s.ligatures}
        onchange={(e) => updateEditorSettings({ ligatures: e.currentTarget.checked })}
      />
      <span>Ligatures</span>
    </label>

    <fieldset>
      <legend>Line numbers</legend>
      {#each ["absolute", "relative", "hybrid"] as const as mode (mode)}
        <label class="check">
          <input
            type="radio"
            name="editor-line-numbers"
            checked={s.lineNumbers === mode}
            onchange={() => updateEditorSettings({ lineNumbers: mode })}
          />
          <span>{mode[0].toUpperCase() + mode.slice(1)}</span>
        </label>
      {/each}
    </fieldset>

    <fieldset>
      <legend>Soft wrap (⌥Z toggles per file)</legend>
      <label class="check">
        <input
          type="checkbox"
          checked={s.wrapProse}
          onchange={(e) => updateEditorSettings({ wrapProse: e.currentTarget.checked })}
        />
        <span>Prose (.md, .txt)</span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={s.wrapCode}
          onchange={(e) => updateEditorSettings({ wrapCode: e.currentTarget.checked })}
        />
        <span>Code</span>
      </label>
    </fieldset>

    <label>
      <span>Indent</span>
      <select
        value={s.indentKind}
        onchange={(e) => updateEditorSettings({ indentKind: e.currentTarget.value as "spaces" | "tabs" })}
      >
        <option value="spaces">Spaces</option>
        <option value="tabs">Tabs</option>
      </select>
      {#if s.indentKind === "spaces"}
        <input
          type="number"
          min="1"
          max="8"
          value={s.indentSize}
          onchange={(e) => updateEditorSettings({ indentSize: Number(e.currentTarget.value) })}
        />
      {/if}
    </label>
    <p class="note">Used only when a file shows no indentation of its own.</p>

    <label>
      <span>Tab width</span>
      <input
        type="number"
        min="1"
        max="8"
        value={s.tabSize}
        onchange={(e) => updateEditorSettings({ tabSize: Number(e.currentTarget.value) })}
      />
    </label>

    <fieldset>
      <legend>Effects</legend>
      <label>
        <span>Cursor</span>
        <select
          value={s.cursorAnimation}
          onchange={(e) => updateEditorSettings({ cursorAnimation: e.currentTarget.value as CursorAnimation })}
        >
          {#each CURSOR as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>File icons</span>
        <select
          value={s.fileIcons}
          onchange={(e) => updateEditorSettings({ fileIcons: e.currentTarget.value as FileIconStyle })}
        >
          {#each FILE_ICON_STYLES as style (style)}
            <option value={style}>{FILE_ICON_STYLE_NAMES[style]}</option>
          {/each}
        </select>
      </label>
      {#each EFFECT_SWITCHES as effect (effect.key)}
        <label class="check">
          <input
            type="checkbox"
            checked={s[effect.key] === true}
            onchange={(e) => updateEditorSettings({ [effect.key]: e.currentTarget.checked })}
          />
          <span>{effect.label}</span>
        </label>
      {/each}
    </fieldset>
    <p class="note">With macOS "Reduce motion" on, the cursor does not glide and nothing scrolls smoothly.</p>

    <label>
      <span>Autosave</span>
      <select
        value={s.autosave}
        onchange={(e) => updateEditorSettings({ autosave: e.currentTarget.value as Autosave })}
      >
        {#each AUTOSAVE as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
    <p class="note">Autosave never overwrites a change made on disk; it stops and asks instead.</p>

    <label class="check">
      <input
        type="checkbox"
        checked={s.formatOnSave}
        onchange={(e) => updateEditorSettings({ formatOnSave: e.currentTarget.checked })}
      />
      <span>Format on save (⌘S, <code>:w</code>)</span>
    </label>
    {#if s.formatOnSave}
      <label>
        <span>Except</span>
        <input
          type="text"
          placeholder="e.g. markdown, python"
          spellcheck="false"
          value={s.formatOnSaveExcept.join(", ")}
          onchange={(e) => updateEditorSettings({ formatOnSaveExcept: languageList(e.currentTarget.value.split(",")) })}
        />
      </label>
    {/if}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .preview {
    height: clamp(calc(280px * var(--ax-ui-scale)), 38vh, calc(440px * var(--ax-ui-scale)));
    flex: 0 0 auto;
    border-bottom: 1px solid var(--ax-border);
  }

  .fields {
    flex: 1;
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }

  label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
  }

  label > span:first-child {
    width: calc(96px * var(--ax-ui-scale));
    color: var(--ax-text-muted);
  }

  label.check > span:first-child {
    width: auto;
    color: var(--ax-text);
  }

  select,
  input[type="number"],
  input[type="text"] {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
  }

  input[type="number"] {
    width: calc(64px * var(--ax-ui-scale));
  }

  input[type="text"] {
    flex: 1;
    min-width: 0;
  }

  input[type="range"] {
    flex: 1;
    accent-color: var(--ax-accent);
  }

  output {
    min-width: calc(96px * var(--ax-ui-scale));
    color: var(--ax-text-muted);
  }

  fieldset {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-3);
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }

  legend {
    color: var(--ax-text-muted);
    padding: 0 var(--ax-space-1);
  }

  .note {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
