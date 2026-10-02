<!--
  The IDE's Git pane (`docs/plans/editor-projekt-werkzeuge.md`, #48): the same panel as the file
  app's Git tab, on the open IDE project's folder. A click on a file opens its change over the
  pane (`GitDiffView`) — as wide as the pane is, so a pane of its own is best split or widened —
  and "open the file" from there opens it in a file pane beside, at the line.
-->
<script lang="ts">
  import type { IdeProject } from "../../core/backend";
  import GitDiffView from "../../fileapp/GitDiffView.svelte";
  import GitPanel from "../../fileapp/GitPanel.svelte";
  import type { Side } from "../../fileapp/gitBackend";
  import { getDock } from "../dockContext";
  import { fileTab, projectRoot, showsFile } from "../paneKinds";

  let { project, tabId, visible }: { project: IdeProject; tabId: string; visible: boolean } = $props();

  const dock = getDock();
  const root = $derived(projectRoot(project.id));

  let change = $state<{ path: string; old_path: string | null; side: Side } | null>(null);
  let nudge = $state(0);
  let version = $state(0);
  let signature = "";

  // Another project: its change is not this one's.
  $effect(() => {
    void project.id;
    change = null;
  });
</script>

<div class="git-pane">
  <GitPanel
    {root}
    active={visible}
    selected={change ? { path: change.path, side: change.side } : null}
    refresh={nudge}
    onOpen={(row) => (change = { path: row.entry.path, old_path: row.entry.old_path, side: row.side })}
    onStatus={(_count, status) => {
      // The open change follows the status, but only when the list of changes really changed.
      const next = JSON.stringify(status?.entries ?? []);
      if (next !== signature) {
        signature = next;
        version++;
      }
    }}
  />
  {#if change}
    <GitDiffView
      {root}
      entry={change}
      side={change.side}
      {version}
      onClose={() => (change = null)}
      onChanged={() => nudge++}
      onOpenFile={(line) => {
        if (!change) return;
        const path = change.path;
        change = null;
        dock.open(fileTab(root, path, line), (t) => showsFile(t, root, path), tabId);
      }}
    />
  {/if}
</div>

<style>
  .git-pane {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--ax-surface-1);
  }
</style>
