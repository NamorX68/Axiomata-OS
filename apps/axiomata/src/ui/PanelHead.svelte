<!--
  The one head of every floating panel (`docs/plans/ui-ux-round.md` UX2): the Kanban, the file window, the chat, the
  settings, the pickers. Same height, the title in the title role, a quiet icon, the panel's own actions, then the
  close button with its key (⌘W, or Ctrl+Shift+W off the Mac — Escape closes as well).

  The head says nothing about where its panel sits or how it moves: a movable panel makes it its drag handle by the
  `panel-head` class (`StagingPanel.svelte`), which is why that class name is part of the contract.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { keyHints } from "../core/keyHints";
  import { platform } from "../core/keys";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import type { IconName } from "./icons/lucide";

  interface Props {
    title: string;
    /** A UI icon by name … */
    icon?: IconName;
    /** … or a module's own drawing (`ModuleDefinition.icon`, static and checked in). */
    iconSvg?: string;
    /** A caption after the title: a count, a session, a state. */
    subtitle?: string;
    /** The panel's own controls, between the title and the close button. */
    actions?: Snippet;
    onClose?: () => void;
    /** The head is the handle the panel is moved by. */
    movable?: boolean;
    /** The heading level, for the document outline. */
    level?: 2 | 3;
  }

  let { title, icon, iconSvg, subtitle, actions, onClose, movable = false, level = 2 }: Props = $props();

  const closeKey = $derived(keyHints("⌘W", $platform));
</script>

<header class="panel-head" class:movable>
  {#if icon}
    <span class="icon"><Icon name={icon} /></span>
  {:else if iconSvg}
    <span class="icon svg" aria-hidden="true">{@html iconSvg}</span>
  {/if}
  <svelte:element this={`h${level}`} class="title">{title}</svelte:element>
  {#if subtitle}<span class="subtitle">{subtitle}</span>{/if}
  <span class="spacer"></span>
  {#if actions}{@render actions()}{/if}
  {#if onClose}
    <kbd class="key" aria-hidden="true">{closeKey}</kbd>
    <IconButton icon="x" label="Close ({closeKey})" onclick={onClose} />
  {/if}
</header>

<style>
  .panel-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
    min-height: var(--ax-panel-head-h);
    padding: 0 var(--ax-space-2) 0 var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    /* A title bar's text is never selected — a drag on it would start a selection before it moves the panel. */
    user-select: none;
  }

  .movable {
    cursor: grab;
  }

  .movable:active {
    cursor: grabbing;
  }

  .icon {
    display: inline-flex;
    color: var(--ax-text-muted);
  }

  .icon.svg {
    width: var(--ax-icon-md);
    height: var(--ax-icon-md);
  }

  .icon.svg :global(svg) {
    width: 100%;
    height: 100%;
  }

  .title {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--ax-type-title);
    letter-spacing: var(--ax-tracking-tight);
    color: var(--ax-text);
  }

  .subtitle {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  .spacer {
    flex: 1;
  }

  .key {
    padding: 0 var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-sm);
    font: var(--ax-type-caption);
    line-height: 1.5;
    color: var(--ax-text-muted);
    opacity: 0.7;
  }
</style>
