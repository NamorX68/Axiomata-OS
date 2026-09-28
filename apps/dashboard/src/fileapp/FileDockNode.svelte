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

  /* A 1 px line between groups (editor-look I1, as in Zed); the grabbable strip is wider
     than the line, on both sides of it, and lights up on hover. */
  .divider {
    position: relative;
    z-index: 1;
    flex: 0 0 1px;
    background: var(--ax-border);
  }

  .divider::before {
    content: "";
    position: absolute;
  }

  .row > .divider::before {
    inset: 0 calc(-1 * var(--ax-space-1));
    cursor: col-resize;
  }

  .col > .divider::before {
    inset: calc(-1 * var(--ax-space-1)) 0;
    cursor: row-resize;
  }

  .divider:hover,
  .divider:active {
    background: var(--ax-accent);
  }
</style>
