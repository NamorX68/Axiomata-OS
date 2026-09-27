<!--
  The IDE's Search pane (`docs/plans/editor.md`, ED5, T13): the project search
  over the open project's folder, the same component as the file app's Search
  tab. A match opens the file in a file pane beside it, at the match's line;
  ⇧⌘F on an open pane puts the cursor back in its field (`focusAt`).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { IdeProject } from "../../core/backend";
  import ProjectSearch from "../../fileapp/ProjectSearch.svelte";
  import { getDock } from "../dockContext";
  import { fileTab, projectRoot, showsFile } from "../paneKinds";

  let { project, tabId, focusAt }: { project: IdeProject; tabId: string; focusAt: number } = $props();

  const dock = getDock();
  let search = $state<ProjectSearch | null>(null);
  /**
   * The last `focusAt` acted on; a newer one focuses the field. Starts at the
   * pane's own, so mounting is not a "newer" one — `onMount` decides that.
   */
  // svelte-ignore state_referenced_locally
  let focused = focusAt;

  const roots = $derived([{ id: projectRoot(project.id), label: project.name }]);

  function open(root: string, rel: string, line: number): void {
    dock.open(fileTab(root, rel, line), (t) => showsFile(t, root, rel), tabId);
  }

  $effect(() => {
    const at = focusAt;
    if (at === focused) return;
    focused = at;
    void search?.focus();
  });

  /** A pane this new was just opened (⇧⌘F, the `+` menu); an older one is a restored layout. */
  const FRESH_MS = 2000;

  onMount(() => {
    focused = focusAt;
    // A layout restored at start-up must not take the focus from where the owner is.
    if (Date.now() - focusAt < FRESH_MS) void search?.focus();
  });
</script>

<div class="search-pane">
  <ProjectSearch bind:this={search} {roots} onOpen={open} />
</div>

<style>
  .search-pane {
    height: 100%;
    min-height: 0;
    background: var(--ax-surface-1);
  }
</style>
