<!--
  The Run view of the activity rail (Run/Tasks, #50): the project's tasks by what they are for, a ▶ on each.

  Three kinds, told apart by a small tag: the ones **detected** from the project's files (cargo, package.json,
  pytest), the owner's **personal** `~/.axiomata/tasks.json`, and the **project**'s own `.axiomata/tasks.json`.
  The last is somebody else's code once a repository is cloned, so its tasks are greyed out behind a
  "Review" that lists exactly what they run; confirming it holds for that exact file content only.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import Notice from "../ui/Notice.svelte";
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
    openTaskIds = new Set<string>(),
    onRun,
    onError,
  }: {
    /** `project:<id>`, or `null` with no project. */
    root: string | null;
    /** The view is on screen (the list is read again each time it is shown). */
    active: boolean;
    /** The tasks that have a pane open: a click there starts that pane again, so the row says so. */
    openTaskIds?: ReadonlySet<string>;
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
  <SectionHead label="Run" rule level={2}>
    {#snippet actions()}
      <IconButton icon="refresh-cw" label="Read the tasks again" size="sm" onclick={() => void refresh()} />
    {/snippet}
  </SectionHead>
  <div class="body">
    {#if !root}
      <p class="note">Open a project to see what it can run.</p>
    {:else if listed}
      {#if listed.project_file && !listed.project_file.trusted}
        {#snippet review()}
          <ul class="review">
            {#each projectTasks as task (task.id)}<li><code>{describeTask(task)}</code></li>{/each}
          </ul>
          <div class="actions">
            <button type="button" class="ax-btn primary" onclick={() => void allow()}>Allow these tasks</button>
            <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
          </div>
        {/snippet}
        <div class="notice-slot">
          <Notice title="Project tasks need your review" alert children={reviewing ? review : undefined}>
            {#snippet details()}
              <p>
                This project has its own <code>.axiomata/tasks.json</code>. It runs commands from the repository, so it
                stays off until you have looked at them.
              </p>
            {/snippet}
            {#snippet action()}
              {#if !reviewing}
                <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
              {/if}
            {/snippet}
          </Notice>
        </div>
      {/if}
      {#each groups as g (g.group)}
        <section class="group g-{g.group}">
          <h3><span class="swatch" aria-hidden="true"></span>{GROUP_LABEL[g.group]}<span class="n">{g.tasks.length}</span></h3>
          <ul>
            {#each g.tasks as task (task.id)}
              {@const inPane = openTaskIds.has(task.id)}
              <li class:locked={locked(task)} class:in-pane={inPane}>
                <button
                  type="button"
                  class="run"
                  disabled={locked(task)}
                  title={locked(task)
                    ? "Review the project's tasks.json first"
                    : inPane
                      ? `Start again in its pane: ${describeTask(task)}`
                      : describeTask(task)}
                  onclick={() => onRun(task)}
                >
                  <span class="play" aria-hidden="true"><Icon name={inPane ? "rotate-ccw" : "play"} size="sm" /></span>
                  <span class="text">
                    <span class="label">{task.label}</span>
                    {#if task.command !== task.label}<span class="cmd">{describeTask(task)}</span>{/if}
                  </span>
                  {#if locked(task)}
                    <span class="tag"><Icon name="lock" size="sm" /> project</span>
                  {:else if task.source !== "detected"}
                    <span class="tag">{task.source}</span>
                  {/if}
                  {#if inPane}<span class="pane-mark">in a pane</span>{/if}
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
        <EmptyState
          compact
          title="Nothing to run yet"
          text="Tasks are found in Cargo.toml, package.json, pyproject.toml and a Makefile, or written in .axiomata/tasks.json."
        />
      {/each}
      {#each listed.problems as problem (problem)}
        <p class="problem">{problem}</p>
      {/each}
    {/if}
    {#if root}
      {#if editing === null}
        <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> New task</button>
      {:else}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <p class="form-title">{editing === "" ? "New task" : "Edit task"}</p>
          <label class="field">
            <span>Name</span>
            <input type="text" bind:value={form.label} placeholder="Start the server" spellcheck="false" />
          </label>
          <label class="field">
            <span>Command</span>
            <input type="text" class="mono" bind:value={form.command} placeholder="uv run uvicorn app:app --reload" spellcheck="false" />
          </label>
          <label class="field">
            <span>Folder <small>inside the project, optional</small></span>
            <input type="text" class="mono" bind:value={form.cwd} placeholder="." spellcheck="false" />
          </label>
          <div class="field">
            <span>What it is for</span>
            <div class="kinds" role="radiogroup" aria-label="What it is for">
              {#each GROUP_CHOICES as choice (choice.id)}
                <label class="kind g-{choice.id}" class:on={form.group === choice.id}>
                  <input type="radio" bind:group={form.group} value={choice.id} />
                  <span class="swatch" aria-hidden="true"></span>{choice.label}
                </label>
              {/each}
            </div>
          </div>
          <div class="field">
            <span>Keep it</span>
            <div class="scopes" role="radiogroup" aria-label="Keep it">
              <label class="scope" class:on={scope === "project"}>
                <input type="radio" bind:group={scope} value="project" />
                <span>In this project</span>
                <small>.axiomata/tasks.json</small>
              </label>
              <label class="scope" class:on={scope === "personal"}>
                <input type="radio" bind:group={scope} value="personal" />
                <span>For all my projects</span>
                <small>~/.axiomata/tasks.json</small>
              </label>
            </div>
          </div>
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

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
  }

  /* Each kind of task is a plate in its colour (the swatch and the edge); a task is a row on it. */
  .group,
  .kind {
    --tone: var(--ax-text-muted);
  }

  .g-run {
    --tone: var(--ax-accent);
  }

  .g-build {
    --tone: var(--ax-harness-opencode);
  }

  .g-test {
    --tone: var(--ax-success);
  }

  .g-lint {
    --tone: var(--ax-warning);
  }

  .group {
    margin-bottom: var(--ax-space-3);
    overflow: hidden;
    border: 1px solid var(--ax-border);
    border-left: calc(3px * var(--ax-ui-scale)) solid color-mix(in srgb, var(--tone) 70%, transparent);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    background: color-mix(in srgb, var(--tone) 6%, var(--ax-elev-3));
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .swatch {
    flex: none;
    width: calc(8px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    clip-path: var(--ax-hex-clip);
    background: var(--tone);
  }

  .n {
    margin-left: auto;
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    font-weight: 400;
    letter-spacing: 0;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
  }

  li + li {
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 60%, transparent);
  }

  .run {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    width: 100%;
    padding: var(--ax-space-2) var(--ax-space-3);
    background: none;
    border: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .run:hover:not(:disabled),
  .run:focus-visible {
    background: var(--ax-elev-3);
  }

  .run:disabled {
    cursor: default;
  }

  .locked .text,
  .locked .play {
    opacity: 0.45;
  }

  /* The play mark is a round button in the group's colour; filled on hover. */
  .play {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: calc(28px * var(--ax-ui-scale));
    height: calc(28px * var(--ax-ui-scale));
    border: 1px solid color-mix(in srgb, var(--tone) 40%, transparent);
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--tone) 14%, transparent);
    color: var(--tone);
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      color var(--ax-dur-fast) var(--ax-ease);
  }

  .run:hover:not(:disabled) .play,
  .run:focus-visible .play {
    background: var(--tone);
    color: var(--ax-bg);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: calc(2px * var(--ax-ui-scale));
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

  .tag,
  .pane-mark {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    font-size: var(--ax-font-size-xs);
  }

  .tag {
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
  }

  /* A pane is open for it: not "running" (the shell may have finished), so no glow — just where it is. */
  .pane-mark {
    border: 1px solid var(--ax-border-strong);
    color: var(--ax-text);
  }

  li {
    position: relative;
  }

  .own {
    position: absolute;
    top: 50%;
    right: var(--ax-space-2);
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
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--ax-space-2);
    width: 100%;
    margin-top: var(--ax-space-1);
    padding: var(--ax-space-2);
    background: none;
    border: 1px dashed var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .add:hover,
  .add:focus-visible {
    border-style: solid;
    background: var(--ax-elev-2);
    color: var(--ax-text);
  }

  /* The form is a plate like the groups, its fields labelled above. */
  form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    margin-top: var(--ax-space-1);
    padding: var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  .form-title {
    margin: 0;
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .field small {
    opacity: 0.8;
  }

  input[type="text"] {
    padding: var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  input.mono {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  input[type="text"]:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }

  .kind {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .kind input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .kind.on {
    border-color: var(--tone);
    background: color-mix(in srgb, var(--tone) 14%, transparent);
    color: var(--ax-text);
  }

  .kind:focus-within {
    outline: var(--ax-focus-ring);
  }

  .scopes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(calc(110px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-1);
  }

  .scope {
    display: flex;
    flex-direction: column;
    gap: calc(2px * var(--ax-ui-scale));
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .scope input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .scope small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .scope.on {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-3);
    color: var(--ax-text);
  }

  .scope:focus-within {
    outline: var(--ax-focus-ring);
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

  .notice-slot {
    margin-bottom: var(--ax-space-3);
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
