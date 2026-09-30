<!--
  The project menu in the IDE's header: which project is open, which others
  there are, and the three things one can do to a project that is not "work in
  it" — add one, point it somewhere else, remove it from the list.

  The wording is deliberate in one place. Removing a project removes a **row**,
  never a folder (`axiomata_ide::store::delete_project` says so in the same
  words), so the button says "Remove from list" and the confirmation repeats
  it. "Delete project" would be a fair reading of a button that then leaves the
  folder untouched — or worse, a fair expectation that it did not.

  A folder that has gone missing is marked rather than dropped: an unmounted
  disk looks exactly like a deleted folder from in here, and the path can be
  corrected in place so the project keeps its id and its layout.
-->
<script lang="ts">
  import type { IdeProject } from "../core/backend";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

  let {
    projects,
    current,
    switching = false,
    onOpen,
    onOpenFolder,
    onNewFolder,
    onSetRoot,
    onRemove,
  }: {
    projects: IdeProject[];
    current: IdeProject | null;
    /** An open is in flight; taking another click now only invites a race. */
    switching?: boolean;
    onOpen: (id: number) => void;
    /** "Open project…": the native dialog, then the folder is a project. */
    onOpenFolder: () => void;
    /** "New project": a new folder called `name`, in a folder the dialog picks. */
    onNewFolder: (name: string, gitInit: boolean) => void;
    onSetRoot: (id: number, repoRoot: string) => void;
    onRemove: (id: number) => void;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLElement | undefined>();
  let newName = $state("");
  let newGit = $state(true);
  let editingRoot = $state<number | null>(null);
  let editedRoot = $state("");
  /** The "New project" form is folded into a row until asked for (editor-look I4). */
  let adding = $state(false);

  // A menu opens fresh: a form left open when it closed (a click elsewhere) is folded again.
  $effect(() => {
    if (!open) {
      adding = false;
      editingRoot = null;
    }
  });

  function submitNew() {
    if (!newName.trim()) return;
    onNewFolder(newName.trim(), newGit);
    newName = "";
    adding = false;
    open = false;
  }

  function startEditingRoot(project: IdeProject) {
    editingRoot = project.id;
    editedRoot = project.repo_root;
  }

  function submitRoot(id: number) {
    if (editedRoot.trim()) onSetRoot(id, editedRoot.trim());
    editingRoot = null;
  }
</script>

<!-- A click anywhere else closes the menu, the way every menu does; the button
     itself is inside `root`, so opening it does not immediately close it. -->
<svelte:window
  onclick={(event) => {
    // composedPath, not contains(target): a click that swaps the menu's own buttons has already detached its target.
    if (open && root && !event.composedPath().includes(root)) open = false;
  }}
/>

<div class="picker" bind:this={root}>
  <button class="current" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    {#if current}
      <span class="name">{current.name}</span>
      {#if !current.root_exists}<span class="missing" title={current.repo_root}>folder missing</span>{/if}
    {:else}
      <span class="name none">No project</span>
    {/if}
    <span class="caret" aria-hidden="true"><Icon name="chevron-down" size="sm" /></span>
  </button>

  {#if open}
    <div class="menu">
      {#if projects.length > 0}
        <ul>
          {#each projects as project (project.id)}
            <li class:active={project.id === current?.id}>
              <button
                class="pick"
                type="button"
                disabled={switching}
                onclick={() => (onOpen(project.id), (open = false))}
              >
                <span class="name">{project.name}</span>
                <span class="path" class:missing={!project.root_exists}>{project.repo_root}</span>
              </button>
              <!-- Quiet until the row is hovered or focused (editor-look I4). -->
              <div class="row-actions">
                <IconButton
                  icon="folder-open"
                  label="Change the path of {project.name}"
                  size="sm"
                  onclick={() => startEditingRoot(project)}
                />
                <IconButton
                  icon="trash-2"
                  label="Remove {project.name} from the list (the folder is left alone)"
                  size="sm"
                  onclick={() => {
                    if (confirm(`Remove “${project.name}” from the list? The folder itself is left alone.`)) {
                      onRemove(project.id);
                    }
                  }}
                />
              </div>
              {#if editingRoot === project.id}
                <form
                  class="edit-root"
                  onsubmit={(event) => {
                    event.preventDefault();
                    submitRoot(project.id);
                  }}
                >
                  <input type="text" spellcheck="false" bind:value={editedRoot} placeholder="/Users/…/repo" />
                  <button type="submit" class="ax-btn primary">Save</button>
                </form>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if !adding}
        <button
          class="add"
          type="button"
          onclick={() => {
            open = false;
            onOpenFolder();
          }}><Icon name="folder-open" size="sm" /> Open project…</button
        >
        <button class="add" type="button" onclick={() => (adding = true)}><Icon name="plus" size="sm" /> New project…</button>
      {:else}
        <div class="new">
          <p class="label">New project</p>
          <input
            type="text"
            spellcheck="false"
            bind:value={newName}
            placeholder="Folder name"
            onkeydown={(event) => {
              if (event.key === "Enter") submitNew();
            }}
          />
          <label class="check"><input type="checkbox" bind:checked={newGit} /> Start a git repository</label>
          <div class="form-actions">
            <button type="button" class="ax-btn primary" disabled={!newName.trim()} onclick={submitNew}>Choose where…</button>
            <button type="button" class="ax-btn" onclick={() => (adding = false)}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }

  .current {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .current:hover {
    border-color: var(--ax-accent);
  }

  .name.none {
    color: var(--ax-text-muted);
  }

  .missing {
    color: var(--ax-warning);
    font-size: var(--ax-font-size-xs);
  }

  .caret {
    color: var(--ax-text-muted);
  }

  .menu {
    position: absolute;
    top: calc(100% + var(--ax-space-2));
    left: 0;
    z-index: 2;
    width: calc(416px * var(--ax-ui-scale));
    max-height: 60vh;
    overflow-y: auto;
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  ul {
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
  }

  /* A row per project (editor-look I4): the pick, its actions on hover, the path form below when open. */
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-md);
  }

  li:hover,
  li:focus-within {
    background: var(--ax-surface-2);
  }

  li.active {
    box-shadow: inset 2px 0 0 var(--ax-accent);
  }

  .pick {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
    padding: 0;
    background: none;
    border: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .path {
    overflow: hidden;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .path.missing {
    color: var(--ax-warning);
    text-decoration: line-through;
  }

  .row-actions {
    display: flex;
    gap: var(--ax-space-1);
    opacity: 0;
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
    opacity: 1;
  }

  /* "New project…": a row of its own, like the projects above it. */
  .add {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .add:hover {
    background: var(--ax-surface-2);
    color: var(--ax-text);
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

  .edit-root {
    flex-basis: 100%;
  }

  .new,
  .edit-root {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }

  .edit-root {
    margin-top: var(--ax-space-2);
  }

  .new {
    padding-top: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .label {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
  }

  input {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  input:focus-visible {
    outline: var(--ax-focus-ring);
  }
</style>
