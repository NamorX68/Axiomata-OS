<!--
  A task running in a terminal (Run/Tasks, #50): a thin bar with the task's name and Restart, and the
  terminal under it. The command is typed into the shell like an agent's start command, and the shell stays
  when it finishes — the output and the scrollback are still there, and Restart types the line again.

  The line comes from `ide/taskRuns.ts` (memory only), never from the tab — see that file for why. A task
  pane without a run (a stored layout is cleaned of them on load) says so rather than guessing.
-->
<script lang="ts">
  import { createContext } from "../../core/registry";
  import Terminal from "../../modules/terminal.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import { taskRuns } from "../taskRuns";

  let {
    tabId,
    label,
    cwd,
    onRestart,
  }: {
    tabId: string;
    label: string;
    /** The project folder, where the line starts (a task's own folder is a `cd` in the line). */
    cwd: string;
    onRestart: () => void;
  } = $props();

  const run = $derived($taskRuns[tabId]);

  /** One context per run: a restart is a new terminal. Config changes go nowhere (a pane's terminal settings are global). */
  const context = $derived(run ? createContext(`${tabId}:${run.runs}`, { cwd }, () => {}) : null);
</script>

<div class="task-pane">
  <div class="bar">
    <span class="label" title={label}>{label}</span>
    <IconButton icon="rotate-ccw" label="Run again" size="sm" onclick={onRestart} />
  </div>
  <div class="body">
    {#if run && context}
      {#key run.runs}
        <Terminal ctx={context} initialCommand={run.line} />
      {/key}
    {:else}
      <p class="none">This task was started in an earlier session. Run it again from the Run view.</p>
    {/if}
  </div>
</div>

<style>
  .task-pane {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    background: var(--ax-surface-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .body {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .none {
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
