<!--
  One node of the dock tree, drawn recursively: a split becomes a flex row or
  column with a divider between each pair of children, a tab group becomes a
  group (drawn by the owner's `group` snippet, so the IDE and the file app share this one).

  A child's `flex-grow` is its stored fraction and its `flex-basis` is zero, so
  the fractions *are* the proportions — there is no second place where a size
  lives and no pixel arithmetic to keep in step with the model. The dividers
  take their own few pixels off the top, which is why the panes end up a hair
  smaller than their exact fraction; nothing depends on the difference.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import DockNode from "./DockNode.svelte";
  import { isSplit, type LayoutNode, type TabGroup } from "./layout";

  interface Props {
    node: LayoutNode;
    /** How a tab group is drawn — the IDE's `PaneGroup`, the file app's `FileGroup`. */
    group: Snippet<[TabGroup]>;
    /** A divider was pressed: boundary `boundary` of split `splitId` (`DockDrag.startDivider`). */
    onDividerDown: (splitId: string, boundary: number, event: PointerEvent) => void;
  }

  let { node, group, onDividerDown }: Props = $props();
</script>

{#if isSplit(node)}
  <div class="split {node.dir}" data-ide-split={node.id}>
    {#each node.children as child, i (child.id)}
      <div class="child" style="flex: {node.sizes[i]} 1 0">
        <DockNode node={child} {group} {onDividerDown} />
      </div>
      {#if i < node.children.length - 1}
        <div
          class="divider"
          role="separator"
          aria-orientation={node.dir === "row" ? "vertical" : "horizontal"}
          onpointerdown={(event) => onDividerDown(node.id, i, event)}
        ></div>
      {/if}
    {/each}
  </div>
{:else}
  {@render group(node)}
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
