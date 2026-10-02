<!--
  The activity rail (`docs/plans/workbench.md`): a narrow column of icons at the Studio's left edge.
  Files and Search sit on top; below the rule come Agents, Terminal and Git as one block. They choose what
  the sidebar column shows (Files, Search, Agents, Git); a click on the one already shown folds the column
  away, as in VS Code. Terminal is an action, not a view: every click opens a new terminal. "Open a file…"
  sits at the foot. Settings and shortcuts stay on the right of the header.
-->
<script lang="ts">
  import IconButton from "../ui/IconButton.svelte";
  import type { SidebarView } from "../fileapp/treeModel";

  let {
    view,
    open,
    gitCount = 0,
    agentsRunning = 0,
    disabled = false,
    onSelect,
    onTerminal,
    onOpenFile,
  }: {
    /** What the sidebar column shows. */
    view: SidebarView;
    /** The column is shown (not folded away). */
    open: boolean;
    /** Changed files, for the Git icon's badge. */
    gitCount?: number;
    /** Agents working right now, for the Agents icon's badge. */
    agentsRunning?: number;
    /** No project open: the terminal has nowhere to start. */
    disabled?: boolean;
    onSelect: (view: SidebarView) => void;
    onTerminal: () => void;
    /** The native file dialog (⌘O). */
    onOpenFile: () => void;
  } = $props();

  const shown = (v: SidebarView) => open && view === v;
</script>

<nav class="rail" aria-label="Views">
  <div class="group" role="tablist" aria-orientation="vertical">
    <IconButton icon="files" size="lg" label="Files" tab pressed={shown("files")} onclick={() => onSelect("files")} />
    <IconButton
      icon="search"
      size="lg"
      label="Search the project (⇧⌘F)"
      tab
      pressed={shown("search")}
      onclick={() => onSelect("search")}
    />
  </div>
  <span class="rule" aria-hidden="true"></span>
  <div class="group" role="tablist" aria-orientation="vertical">
    <span class="slot">
      <IconButton
        icon="bot"
        size="lg"
        label={agentsRunning > 0 ? `Agents — ${agentsRunning} working` : "Agents"}
        tab
        pressed={shown("agents")}
        onclick={() => onSelect("agents")}
      />
      {#if agentsRunning > 0}<span class="badge live" aria-hidden="true">{agentsRunning}</span>{/if}
    </span>
    <IconButton icon="terminal" size="lg" label="New terminal" {disabled} onclick={onTerminal} />
    <span class="slot">
      <IconButton
        icon="git-branch"
        size="lg"
        label={gitCount > 0 ? `Git — ${gitCount} changed` : "Git"}
        tab
        pressed={shown("git")}
        onclick={() => onSelect("git")}
      />
      {#if gitCount > 0}<span class="badge" aria-hidden="true">{gitCount > 99 ? "99+" : gitCount}</span>{/if}
    </span>
  </div>
  <div class="foot">
    <IconButton icon="folder-open" size="lg" label="Open a file… (⌘O)" onclick={onOpenFile} />
  </div>
</nav>

<style>
  .rail {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-4);
    padding: var(--ax-space-4) var(--ax-space-2);
    border-right: 1px solid var(--ax-border);
    background: var(--ax-surface-1);
  }

  .group {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-3);
  }

  .foot {
    margin-top: auto;
  }

  .rule {
    width: calc(28px * var(--ax-ui-scale));
    height: 1px;
    background: var(--ax-border);
  }

  .slot {
    position: relative;
    display: inline-flex;
  }

  .badge {
    position: absolute;
    top: 0;
    right: 0;
    min-width: calc(14px * var(--ax-ui-scale));
    padding: 0 var(--ax-space-1);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
    color: var(--ax-bg);
    font-size: calc(9px * var(--ax-ui-scale));
    line-height: calc(14px * var(--ax-ui-scale));
    text-align: center;
    pointer-events: none;
  }
</style>
