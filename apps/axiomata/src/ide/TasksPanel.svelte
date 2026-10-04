<!--
  The Run view of the activity rail (Run/Tasks, #50): the project's tasks by what they are for, a ▶ on each.

  Three kinds, told apart by a small tag: the ones **detected** from the project's files (cargo, package.json,
  pytest), the owner's **personal** `~/.axiomata/tasks.json`, and the **project**'s own `.axiomata/tasks.json`.
  The last is somebody else's code once a repository is cloned, so its tasks are greyed out behind a
  "Review" that lists exactly what they run; confirming it holds for that exact file content only.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import {
    describeTask,
    groupTasks,
    GROUP_LABEL,
    inputOf,
    listTasks,
    removeTask,
    saveTask,
    trustTasks,
    type NewTaskInput,
    type TaskGroup,
    type TaskInfo,
    type TaskListInfo,
    type TaskScope,
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

  async function allow(): Promise<void> {
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

  // ---- the form: a task of your own, kept in the project or for every project ----

  /** `null` = closed, `""` = a new task, else the label of the task being edited. */
  let editing = $state<string | null>(null);
  let form = $state<NewTaskInput & { cwd: string }>({ label: "", command: "", cwd: "", group: "run" });
  let scope = $state<TaskScope>("project");
  let saving = $state(false);

  const GROUP_CHOICES: { id: TaskGroup; label: string }[] = [
    { id: "run", label: "Run" },
    { id: "build", label: "Build" },
    { id: "test", label: "Test" },
    { id: "lint", label: "Check" },
    { id: "other", label: "Other" },
  ];

  function startNew(): void {
    editing = "";
    form = { label: "", command: "", cwd: "", group: "run" };
    scope = "project";
  }

  function startEdit(task: TaskInfo): void {
    editing = task.label;
    form = { ...inputOf(task), cwd: task.cwd ?? "" };
    scope = task.source === "personal" ? "personal" : "project";
  }

  async function submit(): Promise<void> {
    if (!root || editing === null || saving) return;
    if (!form.label.trim() || !form.command.trim()) {
      onError("A task needs a name and a command.");
      return;
    }
    saving = true;
    try {
      await saveTask(root, scope, { ...form, cwd: form.cwd.trim() || null }, editing === "" ? null : editing);
      editing = null;
      await refresh();
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
    } finally {
      saving = false;
    }
  }

  async function remove(task: TaskInfo): Promise<void> {
    if (!root || task.source === "detected") return;
    if (!window.confirm(`Remove the task “${task.label}” from ${task.source === "personal" ? "your tasks file" : "the project's tasks.json"}?`)) return;
    try {
      await removeTask(root, task.source === "personal" ? "personal" : "project", task.label);
      await refresh();
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
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
              <button type="button" class="ax-btn primary" onclick={() => void allow()}>Allow these tasks</button>
              <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
            </div>
          {:else}
            <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
          {/if}
        </div>
      {/if}
      {#each groups as g (g.group)}
        <section class="g-{g.group}">
          <h3><span class="swatch" aria-hidden="true"></span>{GROUP_LABEL[g.group]}<span class="n">{g.tasks.length}</span></h3>
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
                  <span class="play" aria-hidden="true"><Icon name="play" size="sm" /></span>
                  <span class="text">
                    <span class="label">{task.label}</span>
                    {#if task.command !== task.label}<span class="cmd">{describeTask(task)}</span>{/if}
                  </span>
                  {#if task.source !== "detected"}<span class="tag">{task.source}</span>{/if}
                </button>
                {#if task.source !== "detected" && !locked(task)}
                  <span class="own">
                    <IconButton icon="pencil" label="Edit {task.label}" size="sm" onclick={() => startEdit(task)} />
                    <IconButton icon="trash-2" label="Remove {task.label}" size="sm" onclick={() => void remove(task)} />
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <p class="note">
          Nothing to run found. Tasks are detected from <code>Cargo.toml</code>, <code>package.json</code>,
          <code>pyproject.toml</code> and a <code>Makefile</code>, or written in <code>.axiomata/tasks.json</code>.
        </p>
      {/each}
      {#each listed.problems as problem (problem)}
        <p class="problem">{problem}</p>
      {/each}
    {/if}
    {#if root}
      {#if editing === null}
        <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> New task…</button>
      {:else}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <p class="form-title">{editing === "" ? "New task" : "Edit task"}</p>
          <input type="text" bind:value={form.label} placeholder="Name, e.g. Start the server" spellcheck="false" />
          <input
            type="text"
            bind:value={form.command}
            placeholder="Command, e.g. uv run uvicorn app:app --reload"
            spellcheck="false"
          />
          <input type="text" bind:value={form.cwd} placeholder="Folder inside the project (optional)" spellcheck="false" />
          <select bind:value={form.group} aria-label="What it is for">
            {#each GROUP_CHOICES as choice (choice.id)}<option value={choice.id}>{choice.label}</option>{/each}
          </select>
          <fieldset>
            <legend>Keep it</legend>
            <label><input type="radio" bind:group={scope} value="project" /> in this project <small>.axiomata/tasks.json</small></label>
            <label><input type="radio" bind:group={scope} value="personal" /> for all my projects <small>~/.axiomata</small></label>
          </fieldset>
          <div class="actions">
            <button type="submit" class="ax-btn primary" disabled={saving || !form.label.trim() || !form.command.trim()}>Save</button>
            <button type="button" class="ax-btn" onclick={() => (editing = null)}>Cancel</button>
          </div>
        </form>
      {/if}
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

  section {
    --tone: var(--ax-text-muted);
    margin-bottom: var(--ax-space-3);
  }

  section.g-run {
    --tone: var(--ax-accent);
  }

  section.g-build {
    --tone: var(--ax-harness-opencode);
  }

  section.g-test {
    --tone: var(--ax-success);
  }

  section.g-lint {
    --tone: var(--ax-warning);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin: var(--ax-space-3) 0 var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .swatch {
    width: calc(8px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--tone);
  }

  .n {
    margin-left: auto;
    font-weight: 400;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }

  .run {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    width: 100%;
    padding: var(--ax-space-2) var(--ax-space-2);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-md);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .run:hover:not(:disabled),
  .run:focus-visible {
    background: var(--ax-surface-2);
    border-color: var(--ax-border);
  }

  .run:disabled {
    opacity: 0.45;
    cursor: default;
  }

  /* The play mark is a round button in the group's colour; filled on hover. */
  .play {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: calc(26px * var(--ax-ui-scale));
    height: calc(26px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--tone) 18%, transparent);
    color: var(--tone);
  }

  .run:hover:not(:disabled) .play {
    background: var(--tone);
    color: var(--ax-bg);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .cmd {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .tag {
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  li {
    position: relative;
  }

  .own {
    position: absolute;
    top: 50%;
    right: var(--ax-space-1);
    transform: translateY(-50%);
    display: flex;
    gap: var(--ax-space-1);
    opacity: 0;
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-md);
  }

  li:hover .own,
  li:focus-within .own {
    opacity: 1;
  }

  .add {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .add:hover {
    color: var(--ax-text);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-3);
    padding-top: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .form-title {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
  }

  input[type="text"],
  select {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  input[type="text"]:focus-visible,
  select:focus-visible {
    outline: var(--ax-focus-ring);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }

  legend {
    padding: 0;
    margin-bottom: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  fieldset small {
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
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
