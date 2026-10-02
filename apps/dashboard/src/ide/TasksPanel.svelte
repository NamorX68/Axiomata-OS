<!--
  The Run view of the activity rail (Run/Tasks, #50): the project's tasks by what they are for, a ▶ on each.

  Three kinds, told apart by a small tag: the ones **detected** from the project's files (cargo, package.json,
  pytest), the owner's **personal** `~/.axiomata/tasks.json`, and the **project**'s own `.axiomata/tasks.json`.
  The last is somebody else's code once a repository is cloned, so its tasks are greyed out behind a
  "Review" that lists exactly what they run; confirming it holds for that exact file content only.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import IconButton from "../ui/IconButton.svelte";
  import {
    describeTask,
    groupTasks,
    GROUP_LABEL,
    listTasks,
    trustTasks,
    type TaskInfo,
    type TaskListInfo,
  } from "./tasksBackend";

  let {
    root,
    active,
    onRun,
    onError,
  }: {
    /** `project:<id>`, or `null` with no project. */
    root: string | null;
    /** The view is on screen (the list is read again each time it is shown). */
    active: boolean;
    onRun: (task: TaskInfo) => void;
    onError: (message: string) => void;
  } = $props();

  let listed = $state<TaskListInfo | null>(null);
  let reviewing = $state(false);

  async function refresh(): Promise<void> {
    if (!root) {
      listed = null;
      return;
    }
    try {
      listed = await listTasks(root);
    } catch (err) {
      listed = null;
      onError((err as { message?: string }).message ?? String(err));
    }
  }

  // Read again whenever the view is shown or the project changes.
  $effect(() => {
    void root;
    if (active) void refresh();
  });
  onMount(() => void refresh());

  async function confirm(): Promise<void> {
    if (!root || !listed?.project_file) return;
    try {
      await trustTasks(root, listed.project_file.hash);
      reviewing = false;
      await refresh();
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
      await refresh();
    }
  }

  const projectTasks = $derived(listed?.tasks.filter((t) => t.source === "project") ?? []);
  const groups = $derived(groupTasks(listed?.tasks ?? []));
  const locked = (task: TaskInfo) => task.source === "project" && !listed?.project_file?.trusted;
</script>

<div class="panel">
  <div class="head">
    <span class="title">RUN</span>
    <IconButton icon="refresh-cw" label="Read the tasks again" size="sm" onclick={() => void refresh()} />
  </div>
  <div class="body">
    {#if !root}
      <p class="note">Open a project to see what it can run.</p>
    {:else if listed}
      {#if listed.project_file && !listed.project_file.trusted}
        <div class="trust" role="alert">
          <p>
            This project has its own <code>.axiomata/tasks.json</code>. It runs commands from the repository, so it
            stays off until you have looked at them.
          </p>
          {#if reviewing}
            <ul class="review">
              {#each projectTasks as task (task.id)}<li><code>{describeTask(task)}</code></li>{/each}
            </ul>
            <div class="actions">
              <button type="button" class="ax-btn primary" onclick={() => void confirm()}>Allow these tasks</button>
              <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
            </div>
          {:else}
            <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
          {/if}
        </div>
      {/if}
      {#each groups as g (g.group)}
        <section>
          <h3>{GROUP_LABEL[g.group]}</h3>
          <ul>
            {#each g.tasks as task (task.id)}
              <li class:locked={locked(task)}>
                <button
                  type="button"
                  class="run"
                  disabled={locked(task)}
                  title={locked(task) ? "Review the project's tasks.json first" : describeTask(task)}
                  onclick={() => onRun(task)}
                >
                  <span class="play" aria-hidden="true">▶</span>
                  <span class="label">{task.label}</span>
                  {#if task.source !== "detected"}<span class="tag">{task.source}</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <p class="note">
          Nothing to run found. Tasks are detected from <code>Cargo.toml</code>, <code>package.json</code> and
          pytest, or written in <code>.axiomata/tasks.json</code>.
        </p>
      {/each}
      {#each listed.problems as problem (problem)}
        <p class="problem">{problem}</p>
      {/each}
    {/if}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .head {
    display: flex;
    align-items: center;
    padding: var(--ax-space-1) var(--ax-space-3) var(--ax-space-1) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .title {
    flex: 1;
    letter-spacing: var(--ax-tracking-wide);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
  }

  h3 {
    margin: var(--ax-space-3) 0 var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .run {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: none;
    border-radius: var(--ax-radius-md);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .run:hover:not(:disabled) {
    background: var(--ax-surface-2);
  }

  .run:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .play {
    color: var(--ax-accent);
    font-size: var(--ax-font-size-xs);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .note,
  .problem {
    margin: var(--ax-space-3) 0 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .problem {
    color: var(--ax-warning);
  }

  .trust {
    padding: var(--ax-space-3);
    margin-bottom: var(--ax-space-3);
    border: 1px solid var(--ax-warning);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    font-size: var(--ax-font-size-sm);
  }

  .trust p {
    margin: 0 0 var(--ax-space-2);
  }

  .review {
    margin: 0 0 var(--ax-space-2);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    word-break: break-word;
  }

  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }

  code {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }
</style>
