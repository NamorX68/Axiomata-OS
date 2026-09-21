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

  let {
    projects,
    current,
    switching = false,
    onOpen,
    onCreate,
    onSetRoot,
    onRemove,
  }: {
    projects: IdeProject[];
    current: IdeProject | null;
    /** An open is in flight; taking another click now only invites a race. */
    switching?: boolean;
    onOpen: (id: number) => void;
    onCreate: (name: string, repoRoot: string) => void;
    onSetRoot: (id: number, repoRoot: string) => void;
    onRemove: (id: number) => void;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLElement | undefined>();
  let newName = $state("");
  let newRoot = $state("");
  let editingRoot = $state<number | null>(null);
  let editedRoot = $state("");

  function submitNew() {
    if (!newName.trim() || !newRoot.trim()) return;
    onCreate(newName.trim(), newRoot.trim());
    newName = "";
    newRoot = "";
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
    if (open && root && !root.contains(event.target as Node)) open = false;
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
    <span class="caret" aria-hidden="true">▾</span>
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
              <div class="row-actions">
                <button type="button" onclick={() => startEditingRoot(project)}>Change path</button>
                <button
                  type="button"
                  class="danger"
                  onclick={() => {
                    if (confirm(`Remove “${project.name}” from the list? The folder itself is left alone.`)) {
                      onRemove(project.id);
                    }
                  }}>Remove from list</button
                >
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
                  <button type="submit">Save</button>
                </form>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <form
        class="new"
        onsubmit={(event) => {
          event.preventDefault();
          submitNew();
        }}
      >
        <p class="label">New project</p>
        <input type="text" spellcheck="false" bind:value={newName} placeholder="Name" />
        <input type="text" spellcheck="false" bind:value={newRoot} placeholder="/Users/…/repo" />
        <button type="submit" disabled={!newName.trim() || !newRoot.trim()}>Add</button>
      </form>
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
    width: 26rem;
    max-height: 60vh;
    overflow-y: auto;
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  ul {
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }

  li {
    padding: var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
  }

  li.active {
    box-shadow: inset 2px 0 0 var(--ax-accent);
  }

  .pick {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    width: 100%;
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
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    word-break: break-all;
  }

  .path.missing {
    color: var(--ax-warning);
    text-decoration: line-through;
  }

  .row-actions {
    display: flex;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
  }

  .row-actions button,
  .new button,
  .edit-root button {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .row-actions button:hover,
  .new button:hover:not(:disabled),
  .edit-root button:hover {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }

  .row-actions .danger:hover {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }

  .new button:disabled {
    opacity: var(--ax-tile-glass-opacity);
    cursor: default;
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
