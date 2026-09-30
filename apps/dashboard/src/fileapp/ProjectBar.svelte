<!--
  The project bar over the file tree: the open project, and the three things to do with projects —
  open a folder, start a new one, close the one that is open. The folder always comes from the
  native dialog (driven from Rust); nothing here types a path.

  Closing only takes the project out of the tree. Open tabs stay (unsaved text included), the
  folder is never touched, and the project stays in the registry the IDE shares.
-->
<script lang="ts">
  import type { IdeProject } from "../core/backend";
  import Icon from "../ui/Icon.svelte";
  import { folderNameProblem, otherProjects } from "./projectModel";

  interface Props {
    projects: IdeProject[];
    current: IdeProject | null;
    busy?: boolean;
    onPick: (id: number) => void;
    onOpenFolder: () => void;
    onNew: (name: string, gitInit: boolean) => void;
    onClose: () => void;
  }

  let { projects, current, busy = false, onPick, onOpenFolder, onNew, onClose }: Props = $props();

  let open = $state(false);
  let adding = $state(false);
  let name = $state("");
  let git = $state(true);
  let root = $state<HTMLElement | undefined>();

  const others = $derived(otherProjects(projects, current?.id ?? null));

  $effect(() => {
    if (!open) {
      adding = false;
      problem = null;
    }
  });

  /** Shown under the field once a click or Enter found the name unusable. */
  let problem = $state<string | null>(null);
  let nameField = $state<HTMLInputElement | undefined>();

  function submit(): void {
    const trimmed = name.trim();
    problem = folderNameProblem(trimmed);
    if (problem) {
      nameField?.focus();
      return;
    }
    open = false;
    name = "";
    onNew(trimmed, git);
  }
</script>

<svelte:window
  onclick={(event) => {
    // composedPath, not contains(target): a click that swaps the menu's own buttons has already detached its target.
    if (open && root && !event.composedPath().includes(root)) open = false;
  }}
/>

<div class="project-bar" bind:this={root}>
  <button class="current" type="button" aria-expanded={open} disabled={busy} onclick={() => (open = !open)}>
    <Icon name="folder" size="sm" />
    {#if current}
      <span class="name" title={current.repo_root}>{current.name}</span>
      {#if !current.root_exists}<span class="missing">folder missing</span>{/if}
    {:else}
      <span class="name none">No project</span>
    {/if}
    <span class="caret" aria-hidden="true"><Icon name="chevron-down" size="sm" /></span>
  </button>

  {#if open}
    <div class="menu" role="menu">
      {#if others.length > 0}
        <ul>
          {#each others as project (project.id)}
            <li>
              <button
                type="button"
                role="menuitem"
                onclick={() => {
                  open = false;
                  onPick(project.id);
                }}
              >
                <span>{project.name}</span>
                <small class:missing={!project.root_exists}>{project.repo_root}</small>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if !adding}
        <button
          type="button"
          role="menuitem"
          class="action"
          onclick={() => {
            open = false;
            onOpenFolder();
          }}><Icon name="folder-open" size="sm" /> Open project…</button
        >
        <button type="button" role="menuitem" class="action" onclick={() => (adding = true)}
          ><Icon name="plus" size="sm" /> New project…</button
        >
        {#if current}
          <button
            type="button"
            role="menuitem"
            class="action"
            onclick={() => {
              open = false;
              onClose();
            }}><Icon name="x" size="sm" /> Close project</button
          >
        {/if}
      {:else}
        <!-- No <form>: the buttons act on click, so nothing depends on how the webview treats a submit. -->
        <div class="new-form">
          <p class="label">New project</p>
          <input
            type="text"
            spellcheck="false"
            bind:this={nameField}
            bind:value={name}
            oninput={() => (problem = null)}
            placeholder="Folder name"
            onkeydown={(event) => {
              if (event.key === "Enter") submit();
            }}
          />
          {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
          <label class="check"><input type="checkbox" bind:checked={git} /> Start a git repository</label>
          <div class="form-actions">
            <button type="button" class="ax-btn primary" onclick={submit}>Choose where…</button>
            <button type="button" class="ax-btn" onclick={() => (adding = false)}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .project-bar {
    position: relative;
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
  }

  .current {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .current:hover:not(:disabled) {
    background: var(--ax-accent-muted);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.none {
    color: var(--ax-text-muted);
  }

  .missing {
    color: var(--ax-warning);
    font-size: var(--ax-font-size-xs);
  }

  .menu {
    position: absolute;
    left: var(--ax-space-3);
    right: var(--ax-space-3);
    top: calc(100% + var(--ax-space-1));
    z-index: 10;
    padding: var(--ax-space-1);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  ul {
    margin: 0 0 var(--ax-space-1);
    padding: 0 0 var(--ax-space-1);
    list-style: none;
    border-bottom: 1px solid var(--ax-border);
  }

  .menu button {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .menu button.action {
    flex-direction: row;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .menu button:hover {
    background: var(--ax-accent-muted);
  }

  small {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .new-form {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
  }

  .label {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .problem {
    margin: 0;
    color: var(--ax-warning);
    font-size: var(--ax-font-size-xs);
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .form-actions {
    display: flex;
    gap: var(--ax-space-2);
  }
</style>
