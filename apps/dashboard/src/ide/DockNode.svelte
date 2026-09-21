<!--
  One node of the dock tree, drawn recursively: a split becomes a flex row or
  column with a divider between each pair of children, a tab group becomes a
  `PaneGroup`.

  A child's `flex-grow` is its stored fraction and its `flex-basis` is zero, so
  the fractions *are* the proportions — there is no second place where a size
  lives and no pixel arithmetic to keep in step with the model. The dividers
  take their own few pixels off the top, which is why the panes end up a hair
  smaller than their exact fraction; nothing depends on the difference.
-->
<script lang="ts">
  import DockNode from "./DockNode.svelte";
  import { getDock } from "./dockContext";
  import { isSplit, type LayoutNode } from "./layout";
  import PaneGroup from "./PaneGroup.svelte";

  let { node }: { node: LayoutNode } = $props();

  const dock = getDock();
</script>

{#if isSplit(node)}
  <div class="split {node.dir}" data-ide-split={node.id}>
    {#each node.children as child, i (child.id)}
      <div class="child" style="flex: {node.sizes[i]} 1 0">
        <DockNode node={child} />
      </div>
      {#if i < node.children.length - 1}
        <div
          class="divider"
          role="separator"
          aria-orientation={node.dir === "row" ? "vertical" : "horizontal"}
          onpointerdown={(event) => dock.startDividerDrag(node.id, i, event)}
        ></div>
      {/if}
    {/each}
  </div>
{:else}
  <PaneGroup group={node} />
{/if}

<style>
  .split {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    gap: 0;
  }

  .split.row {
    flex-direction: row;
  }

  .split.col {
    flex-direction: column;
  }

  .child {
    min-width: 0;
    min-height: 0;
  }

  .divider {
    flex: 0 0 var(--ax-space-2);
    background: transparent;
    /* The grabbable strip is wider than the line the user sees, the way every
       usable splitter is; the line itself is drawn by the panes' own borders. */
    position: relative;
    z-index: 1;
  }

  .divider:hover {
    background: var(--ax-accent-muted);
  }

  .split.row > .divider {
    cursor: col-resize;
  }

  .split.col > .divider {
    cursor: row-resize;
  }
</style>
