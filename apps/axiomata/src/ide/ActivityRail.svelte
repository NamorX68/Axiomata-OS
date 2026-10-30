<!--
  The activity rail (`docs/plans/workbench.md`): a narrow column of icons at the Studio's left edge.
  Three kinds of things, set apart: **views of the code** (Files, Search, Git), **views for making things
  run** (Run, Debug, Agents), and at the foot the **actions** (new terminal, open a file). The views choose what
  the sidebar column shows; a click on the one already shown folds the column away, as in VS Code. Actions
  never stay pressed. Settings and shortcuts stay on the right of the header.
-->
<script lang="ts">
  import { keyHints } from "../core/keyHints";
  import { platform } from "../core/keys";
  import IconButton from "../ui/IconButton.svelte";
  import { glow } from "../ui/glow";
  import type { SidebarView } from "../fileapp/treeModel";
  import { tStudio } from "../i18n/studio";

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

<nav class="rail" aria-label={tStudio("railViews")}>
  <!-- Working on the code: where it is, what is in it, what changed. -->
  <div class="group" role="tablist" aria-orientation="vertical">
    <IconButton
      icon="folder-tree"
      size="xl"
      label={tStudio("viewFiles")}
      tab
      pressed={shown("files")}
      onclick={() => onSelect("files")}
    />
    <IconButton
      icon="search"
      size="xl"
      label={keyHints(tStudio("viewSearch"), $platform)}
      tab
      pressed={shown("search")}
      onclick={() => onSelect("search")}
    />
    <span class="slot">
      <IconButton
        icon="git-branch"
        size="xl"
        label={gitCount > 0 ? tStudio("gitChanged", { n: gitCount }) : tStudio("viewGit")}
        tab
        pressed={shown("git")}
        onclick={() => onSelect("git")}
      />
      {#if gitCount > 0}<span class="badge" aria-hidden="true">{gitCount > 99 ? "99+" : gitCount}</span>{/if}
    </span>
  </div>
  <span class="rule" aria-hidden="true"></span>
  <!-- Making things run: tasks and agents. -->
  <div class="group" role="tablist" aria-orientation="vertical">
    <IconButton
      icon="play"
      size="xl"
      label={tStudio("viewRun")}
      tab
      pressed={shown("tasks")}
      onclick={() => onSelect("tasks")}
    />
    <IconButton
      icon="bug"
      size="xl"
      label={tStudio("viewDebug")}
      tab
      pressed={shown("debug")}
      onclick={() => onSelect("debug")}
    />
    <span class="slot">
      <IconButton
        icon="bot"
        size="xl"
        label={agentsRunning > 0 ? tStudio("agentsWorking", { n: agentsRunning }) : tStudio("viewAgents")}
        tab
        pressed={shown("agents")}
        onclick={() => onSelect("agents")}
      />
      {#if agentsRunning > 0}<span class="badge live" aria-hidden="true" use:glow={true}>{agentsRunning}</span>{/if}
    </span>
  </div>
  <!-- Actions, not views: they never stay pressed. -->
  <div class="foot">
    <IconButton icon="terminal" size="xl" label={tStudio("newTerminal")} {disabled} onclick={onTerminal} />
    <IconButton icon="folder-open" size="xl" label={keyHints(tStudio("openFile"), $platform)} onclick={onOpenFile} />
  </div>
</nav>

<style>
  .rail {
    /* Bigger targets for the Studio's main navigation (owner, 2026-10-08: the icons were too small). */
    --ax-hit-min: calc(40px * var(--ax-ui-scale));
    flex: 0 0 auto;
    box-sizing: border-box;
    width: var(--ide-rail-w, auto);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-4);
    /* On the ground, not a plate: the rail is the Studio's frame, the panes float beside it (docs/plans/glass.md). */
    padding: var(--ax-space-4) var(--ax-space-2);
  }

  .group {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-3);
  }

  .foot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-3);
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
    background: var(--ax-text-muted);
    color: var(--ax-bg);
    font-size: calc(9px * var(--ax-ui-scale));
    line-height: calc(14px * var(--ax-ui-scale));
    text-align: center;
    pointer-events: none;
  }

  /* Only the agents at work glow (the accent rule); changed files are a count, not something alive. */
  .badge.live {
    background: var(--ax-glow);
  }
</style>
