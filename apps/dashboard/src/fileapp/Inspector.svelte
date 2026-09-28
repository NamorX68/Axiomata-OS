<!--
  The file app's right-hand column (`docs/plans/editor-look.md`, LK1, K2, K3):
  the editor's settings and the list of shortcuts as two tabs. A column of its
  own beside the editors — never over them, so the minimap stays in view.
-->
<script lang="ts" module>
  export type InspectorTab = "settings" | "shortcuts";
</script>

<script lang="ts">
  import IconButton from "../ui/IconButton.svelte";
  import EditorSettingsPanel from "./EditorSettingsPanel.svelte";
  import ShortcutsPanel from "./ShortcutsPanel.svelte";
  import type { SurfaceSettings } from "./surfaceSettings";

  interface Props {
    tab: InspectorTab;
    /** The surface settings for the settings' live preview. */
    surface: SurfaceSettings;
    onTab: (tab: InspectorTab) => void;
    onClose: () => void;
  }

  let { tab, surface, onTab, onClose }: Props = $props();

  const TABS: { id: InspectorTab; label: string }[] = [
    { id: "settings", label: "Settings" },
    { id: "shortcuts", label: "Shortcuts" },
  ];
</script>

<aside class="inspector" aria-label="Inspector">
  <header>
    <div class="tabs" role="tablist" aria-label="Inspector">
      {#each TABS as t (t.id)}
        <button type="button" role="tab" class:active={tab === t.id} aria-selected={tab === t.id} onclick={() => onTab(t.id)}>
          {t.label}
        </button>
      {/each}
    </div>
    <IconButton icon="x" label="Close the inspector" onclick={onClose} />
  </header>
  {#if tab === "settings"}
    <EditorSettingsPanel {surface} />
  {:else}
    <ShortcutsPanel />
  {/if}
</aside>

<style>
  .inspector {
    flex: 0 0 auto;
    width: min(calc(440px * var(--ax-ui-scale)), 40%);
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-2);
    border-left: 1px solid var(--ax-border);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
  }

  .tabs {
    display: flex;
    gap: var(--ax-space-1);
    padding: 2px;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
  }

  .tabs button {
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 0;
    border-radius: var(--ax-radius-pill);
    background: transparent;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .tabs button:hover {
    color: var(--ax-text);
  }

  .tabs button.active {
    background: var(--ax-surface-3);
    color: var(--ax-text);
  }
</style>
