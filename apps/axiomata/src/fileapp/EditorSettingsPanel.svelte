<!--
  The editor's settings (`docs/plans/editor.md`, D6, F12, F13), the Settings tab
  of the file app's inspector (`Inspector.svelte`, editor-look LK1). Every change
  applies at once and is saved; the preview at the top is a real, read-only
  editor surface on sample code, so what it shows is exactly what the editor
  will do.
-->
<script lang="ts">
  import { keyHints } from "../core/keyHints";
  import { platform } from "../core/keys";
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
  import { EDITOR_THEMES, FOLLOW_APP_THEME } from "../core/editorThemes";
  import { FILE_ICON_STYLE_NAMES, FILE_ICON_STYLES, type FileIconStyle } from "../core/fileIcons";
  import type { SurfaceSettings } from "./surfaceSettings";
  import { tEditor } from "../i18n/editor";

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

  const CURSOR: { value: CursorAnimation; label: () => string }[] = [
    { value: "strong", label: () => tEditor("cursorStrong") },
    { value: "subtle", label: () => tEditor("cursorSubtle") },
    { value: "off", label: () => tEditor("off") },
  ];

  /** The on/off effects of G7, each its own switch. */
  const EFFECT_SWITCHES: { key: keyof EditorSettings; label: () => string }[] = [
    { key: "smoothScroll", label: () => tEditor("smoothScroll") },
    { key: "currentLine", label: () => tEditor("currentLine") },
    { key: "indentGuides", label: () => tEditor("indentGuides") },
    { key: "bracketColors", label: () => tEditor("bracketColors") },
    { key: "glow", label: () => tEditor("accentGlow") },
    { key: "foldAnimation", label: () => tEditor("foldAnimation") },
    { key: "minimap", label: () => tEditor("minimap") },
    { key: "stickyScroll", label: () => tEditor("stickyScroll") },
    { key: "diagnosticsInline", label: () => tEditor("diagnosticsInline") },
  ];

  const AUTOSAVE: { value: Autosave; label: () => string }[] = [
    { value: "off", label: () => tEditor("off") },
    { value: "delay", label: () => tEditor("autosaveDelay") },
    { value: "leave", label: () => tEditor("autosaveLeave") },
  ];

  /** The line-number styles, as the radio buttons name them. */
  const LINE_NUMBERS = { absolute: "lineAbsolute", relative: "lineRelative", hybrid: "lineHybrid" } as const;

  /** A file-icon style's name: the sets keep theirs (Q13), the two plain choices are words. */
  function iconStyleName(style: FileIconStyle): string {
    if (style === "mono") return tEditor("iconsMono");
    if (style === "none") return tEditor("iconsNone");
    return FILE_ICON_STYLE_NAMES[style];
  }
</script>

<div class="panel" aria-label={tEditor("editorSettings")}>

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
      <span>{tEditor("keys")}</span>
      <select
        value={s.mode}
        onchange={(e) => updateEditorSettings({ mode: e.currentTarget.value as "normal" | "vi" })}
      >
        <option value="normal">{tEditor("keysNormal")}</option>
        <option value="vi">{"Vi"}</option>
      </select>
    </label>

    {#if s.mode === "vi"}
      <label>
        <span>{tEditor("viClipboard")}</span>
        <select
          value={s.viClipboard}
          onchange={(e) => updateEditorSettings({ viClipboard: e.currentTarget.value as "shared" | "separate" })}
        >
          <option value="shared">{keyHints(tEditor("viClipboardShared"), $platform)}</option>
          <option value="separate">{tEditor("viClipboardSeparate")}</option>
        </select>
      </label>
    {/if}

    <label>
      <span>{tEditor("font")}</span>
      <select value={s.fontFamily} onchange={(e) => updateEditorSettings({ fontFamily: e.currentTarget.value })}>
        <optgroup label={tEditor("fontsBundled")}>
          {#each EDITOR_FONTS as font (font.family)}
            <option value={font.family}>{font.family}</option>
          {/each}
        </optgroup>
        {#if installedChoices.length > 0}
          <optgroup label={tEditor("fontsInstalled")}>
            {#each installedChoices as font (font.family)}
              <option value={font.family}>{font.family}</option>
            {/each}
          </optgroup>
        {/if}
        {#if missing}
          <option value={s.fontFamily}>{tEditor("fontNotInstalled", { font: s.fontFamily })}</option>
        {/if}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={monospaceOnly} />
      <span>{tEditor("monospaceOnly")}</span>
    </label>
    {#if missing}
      <p class="note">{tEditor("fontMissing", { font: s.fontFamily, fallback: DEFAULT_FONT_FAMILY })}</p>
    {/if}

    <label>
      <span>{tEditor("weight")}</span>
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
        {tEditor("weightMissing", {
          font: s.fontFamily,
          weight: weightName(s.fontWeight).split(" ")[0],
          shown: weightName(shownWeight),
          weights: weights.map((w) => weightName(w).split(" ")[0]).join(", "),
        })}
      </p>
    {/if}

    <label>
      <span>{tEditor("size")}</span>
      <input
        type="number"
        min="9"
        max="32"
        value={s.fontSize}
        onchange={(e) => updateEditorSettings({ fontSize: Number(e.currentTarget.value) })}
      />
      <output>{"px"}</output>
    </label>

    <label>
      <span>{tEditor("lineHeight")}</span>
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
      <span>{tEditor("ligatures")}</span>
    </label>

    <fieldset>
      <legend>{tEditor("lineNumbers")}</legend>
      {#each ["absolute", "relative", "hybrid"] as const as mode (mode)}
        <label class="check">
          <input
            type="radio"
            name="editor-line-numbers"
            checked={s.lineNumbers === mode}
            onchange={() => updateEditorSettings({ lineNumbers: mode })}
          />
          <span>{tEditor(LINE_NUMBERS[mode])}</span>
        </label>
      {/each}
    </fieldset>

    <fieldset>
      <legend>{keyHints(tEditor("softWrap"), $platform)}</legend>
      <label class="check">
        <input
          type="checkbox"
          checked={s.wrapProse}
          onchange={(e) => updateEditorSettings({ wrapProse: e.currentTarget.checked })}
        />
        <span>{tEditor("wrapProse")}</span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={s.wrapCode}
          onchange={(e) => updateEditorSettings({ wrapCode: e.currentTarget.checked })}
        />
        <span>{tEditor("wrapCode")}</span>
      </label>
    </fieldset>

    <label>
      <span>{tEditor("indent")}</span>
      <select
        value={s.indentKind}
        onchange={(e) => updateEditorSettings({ indentKind: e.currentTarget.value as "spaces" | "tabs" })}
      >
        <option value="spaces">{tEditor("spaces")}</option>
        <option value="tabs">{tEditor("tabs")}</option>
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
    <p class="note">{tEditor("indentNote")}</p>

    <label>
      <span>{tEditor("tabWidth")}</span>
      <input
        type="number"
        min="1"
        max="8"
        value={s.tabSize}
        onchange={(e) => updateEditorSettings({ tabSize: Number(e.currentTarget.value) })}
      />
    </label>

    <fieldset>
      <legend>{tEditor("effects")}</legend>
      <label>
        <span>{tEditor("cursor")}</span>
        <select
          value={s.cursorAnimation}
          onchange={(e) => updateEditorSettings({ cursorAnimation: e.currentTarget.value as CursorAnimation })}
        >
          {#each CURSOR as option (option.value)}
            <option value={option.value}>{option.label()}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>{tEditor("editorTheme")}</span>
        <select value={s.editorTheme} onchange={(e) => updateEditorSettings({ editorTheme: e.currentTarget.value })}>
          <option value={FOLLOW_APP_THEME}>{tEditor("followAppTheme")}</option>
          {#each EDITOR_THEMES as theme (theme.id)}
            <option value={theme.id}>{theme.light ? tEditor("themeLight", { name: theme.label }) : theme.label}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>{tEditor("fileIcons")}</span>
        <select
          value={s.fileIcons}
          onchange={(e) => updateEditorSettings({ fileIcons: e.currentTarget.value as FileIconStyle })}
        >
          {#each FILE_ICON_STYLES as style (style)}
            <option value={style}>{iconStyleName(style)}</option>
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
          <span>{effect.label()}</span>
        </label>
      {/each}
    </fieldset>
    <p class="note">{tEditor("reduceMotionNote")}</p>

    <label>
      <span>{tEditor("autosave")}</span>
      <select
        value={s.autosave}
        onchange={(e) => updateEditorSettings({ autosave: e.currentTarget.value as Autosave })}
      >
        {#each AUTOSAVE as option (option.value)}
          <option value={option.value}>{option.label()}</option>
        {/each}
      </select>
    </label>
    <p class="note">{tEditor("autosaveNote")}</p>

    <label class="check">
      <input
        type="checkbox"
        checked={s.formatOnSave}
        onchange={(e) => updateEditorSettings({ formatOnSave: e.currentTarget.checked })}
      />
      <span
        >{keyHints(tEditor("formatOnSaveBefore"), $platform)} <code>{":w"}</code>{tEditor("formatOnSaveAfter")}</span
      >
    </label>
    {#if s.formatOnSave}
      <label>
        <span>{tEditor("except")}</span>
        <input
          type="text"
          placeholder={tEditor("exceptPlaceholder")}
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
