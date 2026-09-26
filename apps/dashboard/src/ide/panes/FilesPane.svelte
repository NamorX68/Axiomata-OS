<!--
  The IDE's Files pane (`docs/plans/editor.md`, ED4, W9): the file app's tree,
  limited to the open project's folder. A click opens the file in the dock's
  file group (`paneKinds.ts`'s `openOrFocus`), beside this pane when there is
  none yet. The open folders and the hidden toggle live on the tab, so they
  are stored with the project's layout.
-->
<script lang="ts">
  import type { FileRootInfo } from "../../core/backend";
  import { toast } from "../../core/toast";
  import FileTree from "../../fileapp/FileTree.svelte";
  import type { FileRef } from "../../fileapp/tabs";
  import { getDock } from "../dockContext";
  import type { FilesPaneConfig } from "../paneKinds";
  import { fileTab, projectRoot, showsFile } from "../paneKinds";

  interface Props {
    project: { id: number; name: string; repo_root: string };
    config: FilesPaneConfig;
    tabId: string;
    /** The file in the dock's front file pane, highlighted. */
    active: FileRef | null;
    onConfig: (config: FilesPaneConfig) => void;
  }

  let { project, config, tabId, active, onConfig }: Props = $props();

  const dock = getDock();
  let tree = $state<ReturnType<typeof FileTree> | undefined>();

  const roots = $derived<FileRootInfo[]>([
    { id: projectRoot(project.id), label: project.name, path: project.repo_root, kind: "project" },
  ]);

  /** The tree binds `expanded`; every change goes back onto the tab. */
  function setExpanded(next: string[]): void {
    onConfig({ ...config, expanded: next });
  }

  function open(file: FileRef): void {
    dock.open(fileTab(file.root, file.rel, null), (t) => showsFile(t, file.root, file.rel), tabId);
  }
</script>

<div class="files-pane">
  <div class="bar">
    <span>Files</span>
    <label title="Show dotfiles, .git, node_modules, target">
      <input
        type="checkbox"
        checked={config.showHidden}
        onchange={(e) => onConfig({ ...config, showHidden: e.currentTarget.checked })}
      /> hidden
    </label>
    <button type="button" class="icon" aria-label="Read the folders again" onclick={() => tree?.refresh()}>↻</button>
  </div>
  <FileTree
    bind:this={tree}
    {roots}
    bind:expanded={() => config.expanded, setExpanded}
    showHidden={config.showHidden}
    {active}
    onOpen={(file) => open(file)}
    onError={(message) => toast(message, "danger")}
  />
</div>

<style>
  /* The same look as the file app's tree column (`FileAppView.svelte`'s `.side`). */
  .files-pane {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .bar span {
    flex: 1;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .bar label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }

  .icon {
    padding: 0 var(--ax-space-1);
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .icon:hover {
    color: var(--ax-text);
  }
</style>
