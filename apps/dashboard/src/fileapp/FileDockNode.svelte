<!--
  One node of the file app's group tree (`docs/plans/editor-look.md`, LK2),
  drawn recursively: a split is a flex row or column with a divider between each
  pair of children, a group is a `FileGroup`. A child's `flex-grow` is its
  stored fraction, as in the IDE (`ide/DockNode.svelte`).
-->
<script lang="ts">
  import { isSplit, type LayoutNode } from "../ide/layout";
  import FileDockNode from "./FileDockNode.svelte";
  import { getFileDock } from "./fileDockContext";
  import FileGroup from "./FileGroup.svelte";

  let { node }: { node: LayoutNode } = $props();

  const view = getFileDock();
</script>

{#if isSplit(node)}
  <div class="split {node.dir}" data-ide-split={node.id}>
    {#each node.children as child, i (child.id)}
      <div class="child" style="flex: {node.sizes[i]} 1 0">
        <FileDockNode node={child} />
      </div>
      {#if i < node.children.length - 1}
        <div
          class="divider"
          role="separator"
          aria-orientation={node.dir === "row" ? "vertical" : "horizontal"}
          onpointerdown={(event) => view.startDividerDrag(node.id, i, event)}
        ></div>
      {/if}
    {/each}
  </div>
{:else}
  <FileGroup group={node} />
{/if}

<style>
  .split {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .split.row {
    flex-direction: row;
  }

  .split.col {
    flex-direction: column;
  }

  .child {
    display: flex;
    min-width: 0;
    min-height: 0;
  }

  .divider {
    flex: 0 0 auto;
    background: var(--ax-border);
  }

  .divider:hover {
    background: var(--ax-accent);
  }

  .row > .divider {
    width: var(--ax-space-1);
    cursor: col-resize;
  }

  .col > .divider {
    height: var(--ax-space-1);
    cursor: row-resize;
  }
</style>
