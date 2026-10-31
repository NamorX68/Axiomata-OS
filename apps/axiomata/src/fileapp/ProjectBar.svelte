<!--
  The project bar over the file tree: the open project, and the three things to do with projects —
  open a folder, start a new one, close the one that is open. The folder always comes from the
  native dialog (driven from Rust); nothing here types a path.

  Closing only takes the project out of the tree. Open tabs stay (unsaved text included), the
  folder is never touched, and the project stays in the registry the IDE shares.
-->
<script lang="ts">
  import type { IdeProject } from "../core/backend";
  import { tEditor } from "../i18n/editor";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import { folderNameProblem, otherProjects } from "./projectModel";
  import { ACCENT_SLOTS, accentColor, themeAreaSwatches } from "../ide/projectAccent";

  interface Props {
    projects: IdeProject[];
    current: IdeProject | null;
    busy?: boolean;
    onPick: (id: number) => void;
    onOpenFolder: () => void;
    onNew: (name: string, gitInit: boolean) => void;
    onClose: () => void;
    /** "Change folder…": the native dialog picks the new one; the project keeps its id and layout. */
    onChangeFolder: (id: number) => void;
    /** "Remove from list": the registry row goes, the folder stays. */
    onRemove: (id: number) => void;
    /** Gives a project its marker colour (a slot), or none; without it the bar shows markers but cannot change them. */
    onSetAccent?: (id: number, slot: number | null) => void;
  }

  let { projects, current, busy = false, onPick, onOpenFolder, onNew, onClose, onChangeFolder, onRemove, onSetAccent }: Props = $props();

  const swatches = themeAreaSwatches;

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

  function removeWithQuestion(project: IdeProject): void {
    if (!confirm(tEditor("removeProjectConfirm", { name: project.name }))) return;
    open = false;
    onRemove(project.id);
  }

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
      {#if accentColor(current.accent_slot, $swatches)}<span class="dot" style:background={accentColor(current.accent_slot, $swatches)}></span>{/if}
      <span class="name" title={current.repo_root}>{current.name}</span>
      {#if !current.root_exists}<span class="missing">{tEditor("folderMissing")}</span>{/if}
    {:else}
      <span class="name none">{tEditor("noProject")}</span>
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
                <span class="entry-name">
                  {#if accentColor(project.accent_slot, $swatches)}<span class="dot" style:background={accentColor(project.accent_slot, $swatches)}></span>{/if}
                  {project.name}
                </span>
                <small class:missing={!project.root_exists}>{project.repo_root}</small>
              </button>
              <!-- Quiet until the row is hovered or focused, as in the IDE's project list. -->
              <div class="row-actions">
                <IconButton
                  icon="folder-open"
                  label={tEditor("changeFolderOf", { name: project.name })}
                  size="sm"
                  onclick={() => (onChangeFolder(project.id), (open = false))}
                />
                <IconButton
                  icon="trash-2"
                  label={tEditor("removeFromListOf", { name: project.name })}
                  size="sm"
                  onclick={() => removeWithQuestion(project)}
                />
              </div>
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
          }}><Icon name="folder-open" size="sm" /> {tEditor("openProject")}</button
        >
        <button type="button" role="menuitem" class="action" onclick={() => (adding = true)}
          ><Icon name="plus" size="sm" /> {tEditor("newProjectDots")}</button
        >
        {#if current && onSetAccent}
          <div class="accent-row" role="group" aria-label={tEditor("markerColourOf", { name: current.name })}>
            <span class="label">{tEditor("marker")}</span>
            <button
              type="button"
              class="slot none"
              class:chosen={current.accent_slot === null}
              aria-label={tEditor("noMarkerColour")}
              aria-pressed={current.accent_slot === null}
              title={tEditor("noMarker")}
              onclick={() => onSetAccent(current.id, null)}
            ></button>
            {#each { length: ACCENT_SLOTS } as _, slot (slot)}
              <button
                type="button"
                class="slot"
                class:chosen={current.accent_slot === slot}
                style:background={accentColor(slot, $swatches)}
                aria-label={tEditor("markerColourN", { n: slot + 1 })}
                aria-pressed={current.accent_slot === slot}
                onclick={() => onSetAccent(current.id, slot)}
              ></button>
            {/each}
          </div>
        {/if}
        {#if current}
          <button
            type="button"
            role="menuitem"
            class="action"
            onclick={() => {
              open = false;
              onClose();
            }}><Icon name="x" size="sm" /> {tEditor("closeProject")}</button
          >
          <button
            type="button"
            role="menuitem"
            class="action"
            onclick={() => {
              open = false;
              onChangeFolder(current.id);
            }}><Icon name="folder-open" size="sm" /> {tEditor("changeFolder")}</button
          >
          <button type="button" role="menuitem" class="action" onclick={() => removeWithQuestion(current)}
            ><Icon name="trash-2" size="sm" /> {tEditor("removeFromList")}</button
          >
        {/if}
      {:else}
        <!-- No <form>: the buttons act on click, so nothing depends on how the webview treats a submit. -->
        <div class="new-form">
          <p class="label">{tEditor("newProject")}</p>
          <input
            type="text"
            spellcheck="false"
            bind:this={nameField}
            bind:value={name}
            oninput={() => (problem = null)}
            placeholder={tEditor("projectName")}
            onkeydown={(event) => {
              if (event.key === "Enter") submit();
            }}
          />
          {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
          <label class="check"><input type="checkbox" bind:checked={git} /> {tEditor("startGitRepo")}</label>
          <div class="form-actions">
            <button type="button" class="ax-btn primary" onclick={submit}>{tEditor("chooseWhere")}</button>
            <button type="button" class="ax-btn" onclick={() => (adding = false)}>{tEditor("cancel")}</button>
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

  /* A mark for orientation, small on purpose: it must never compete with the accent. */
  /* The project's mark in its colour (G19): a hexagon, the system's shape. */
  .dot {
    flex: none;
    width: calc(9px * var(--ax-ui-scale));
    height: calc(10px * var(--ax-ui-scale));
    clip-path: var(--ax-hex-clip);
  }

  .entry-name {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .accent-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
  }

  /* The menu's own `button` rule makes buttons full-width columns; a swatch is a dot. */
  .menu .accent-row .slot {
    width: calc(14px * var(--ax-ui-scale));
    height: calc(14px * var(--ax-ui-scale));
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
  }

  .menu .accent-row .slot.none {
    background: none;
    border-color: var(--ax-border-strong);
  }

  .menu .accent-row .slot.chosen {
    border-color: var(--ax-text);
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

  li {
    position: relative;
  }

  .row-actions {
    position: absolute;
    top: 50%;
    right: var(--ax-space-1);
    display: none;
    gap: var(--ax-space-1);
    transform: translateY(-50%);
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
    display: flex;
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
