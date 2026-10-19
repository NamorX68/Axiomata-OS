<!--
  The git panel (`docs/plans/editor-projekt-werkzeuge.md`, #48): the branch, the changes in two
  groups — *Staged* (what the next commit contains) and *Changes* (everything else, new files
  included) — and a commit box. A click on a file opens its change as a diff over the editor
  (`GitDiffView`), where single hunks can be staged too.

  * **The status is read again** when the panel comes into view, after everything done here or in
    the diff, when the window is focused again, and every few seconds while the panel is showing.
  * **Only *Push* and *Commit & Push* publish**, and only the checked-out branch to its upstream (or
    `origin`, for a branch that has none) — never forced; a rejected push shows git's own message and
    changes nothing. *Fetch* only updates the remote-tracking branches.
  * It works on the open project's folder; without a project, or in a folder that is no
    repository, it says so instead.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import { messageOf } from "../core/errors";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import { gitApi, type Branch, type OutgoingCommit, type RepoStatus, type Side } from "./gitBackend";
  import { canCommit, groupEntries, markOf, pushState, splitPath, type GitRow } from "./gitModel";

  interface Props {
    /** The open project's root (`project:<id>`), or `null`. */
    root: string | null;
    /** The panel is on screen: it keeps itself up to date. */
    active: boolean;
    selected: { path: string; side: Side } | null;
    /** Bumped from outside (the diff changed something): read the status again. */
    refresh?: number;
    onOpen: (row: GitRow) => void;
    /** After every read: how many files are changed (the tab's badge), and the status itself. */
    onStatus?: (changes: number, status: RepoStatus | null) => void;
  }

  let { root, active, selected, refresh = 0, onOpen, onStatus }: Props = $props();

  const POLL_MS = 5000;

  let status = $state.raw<RepoStatus | null>(null);
  let notARepo = $state(false);
  let error = $state("");
  let message = $state("");
  let busy = $state(false);
  let fetching = $state(false);
  let pushing = $state(false);
  /** The branch menu: open, the branches it lists, and the name being typed for a new one. */
  let branchMenu = $state(false);
  let branchList = $state<Branch[]>([]);
  let newBranch = $state("");
  let branchBox = $state<HTMLElement | undefined>();
  /** What a push would publish: the commits after the upstream, read with the status while there are any. */
  let outgoing = $state.raw<OutgoingCommit[]>([]);
  let outgoingOpen = $state(false);
  /** The commits whose files are shown. */
  let openCommits = $state<Record<string, boolean>>({});
  let timer: ReturnType<typeof setInterval> | undefined;
  let generation = 0;

  const groups = $derived(groupEntries(status?.entries ?? []));
  const commitReady = $derived(canCommit(groups, message));
  const push = $derived(status ? pushState(status) : null);

  async function read(): Promise<void> {
    const mine = ++generation;
    if (!root) {
      status = null;
      notARepo = false;
      onStatus?.(0, null);
      return;
    }
    try {
      const state = await gitApi.status(root);
      if (mine !== generation) return;
      notARepo = state.state === "not_a_repo";
      status = state.state === "ready" ? state.status : null;
      error = "";
      outgoing = status && status.ahead > 0 ? await gitApi.outgoing(root).catch(() => []) : [];
      if (mine !== generation) return;
      onStatus?.(status?.entries.length ?? 0, status);
    } catch (err) {
      if (mine === generation) error = messageOf(err);
    }
  }

  async function act(work: () => Promise<unknown>): Promise<void> {
    busy = true;
    try {
      await work();
      error = "";
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
      await read();
    }
  }

  const stage = (row: GitRow) => act(() => gitApi.stage(root!, [row.entry.path]));
  const unstage = (row: GitRow) =>
    act(() => gitApi.unstage(root!, [row.entry.path, ...(row.entry.old_path ? [row.entry.old_path] : [])]));
  const stageAll = () => act(() => gitApi.stageAll(root!));
  const unstageAll = () => act(() => gitApi.unstageAll(root!));

  /** Whether the model is writing a message right now. */
  let suggesting = $state(false);

  /** Fills the message box with a suggestion for what is staged (board card #96). Replaces what is there: the owner
   *  asked for it, and it stays editable before the commit. */
  async function suggestMessage(): Promise<void> {
    if (!root || suggesting || groups.staged.length === 0) return;
    suggesting = true;
    error = "";
    try {
      message = await gitApi.suggestMessage(root);
    } catch (err) {
      error = messageOf(err);
    } finally {
      suggesting = false;
    }
  }

  async function commit(): Promise<void> {
    if (!commitReady || !root) return;
    const text = message;
    await act(async () => {
      await gitApi.commit(root!, text);
      message = "";
    });
  }

  /** Throws a file's unstaged changes away — or deletes an untracked file — after asking. */
  async function revert(row: GitRow): Promise<void> {
    const name = row.entry.path;
    const question = row.untracked
      ? `Delete ${name}? It is not in git, so it cannot be brought back.`
      : `Throw away the unstaged changes in ${name}? They are not saved anywhere and cannot be brought back.`;
    if (!confirm(question)) return;
    await act(() => gitApi.discard(root!, [name]));
  }

  async function createRepo(): Promise<void> {
    if (!root) return;
    await act(() => gitApi.init(root!));
  }

  async function openBranches(): Promise<void> {
    if (!root) return;
    branchMenu = !branchMenu;
    if (!branchMenu) return;
    try {
      branchList = await gitApi.branches(root);
      error = "";
    } catch (err) {
      error = messageOf(err);
      branchMenu = false;
    }
  }

  async function switchTo(name: string): Promise<void> {
    branchMenu = false;
    await act(() => gitApi.switchBranch(root!, name));
  }

  async function makeBranch(): Promise<void> {
    const name = newBranch.trim();
    if (!name) return;
    branchMenu = false;
    newBranch = "";
    await act(() => gitApi.createBranch(root!, name));
  }

  async function pushBranch(): Promise<void> {
    if (!root) return;
    pushing = true;
    try {
      await gitApi.push(root);
      error = "";
    } catch (err) {
      error = messageOf(err);
    } finally {
      pushing = false;
      await read();
    }
  }

  /** Commits what is staged, then pushes; if the push fails the commit stays and the message says so. */
  async function commitAndPush(): Promise<void> {
    if (!commitReady || !root) return;
    const text = message;
    busy = true;
    try {
      await gitApi.commit(root, text);
      message = "";
    } catch (err) {
      error = messageOf(err);
      busy = false;
      await read();
      return;
    }
    pushing = true;
    try {
      await gitApi.push(root);
      error = "";
    } catch (err) {
      error = `Committed, but not pushed: ${messageOf(err)}`;
    } finally {
      busy = false;
      pushing = false;
      await read();
    }
  }

  async function fetchRemote(): Promise<void> {
    if (!root) return;
    fetching = true;
    try {
      await gitApi.fetch(root);
      error = "";
    } catch (err) {
      error = messageOf(err);
    } finally {
      fetching = false;
      await read();
    }
  }

  // A new project, coming into view, or a change made in the diff.
  $effect(() => {
    void root;
    void refresh;
    if (active) void read();
  });

  $effect(() => {
    clearInterval(timer);
    if (!active || !root) return;
    timer = setInterval(() => {
      if (document.visibilityState === "visible") void read();
    }, POLL_MS);
    return () => clearInterval(timer);
  });

  onDestroy(() => clearInterval(timer));
</script>

<svelte:window
  onfocus={() => active && void read()}
  onclick={(event) => {
    // composedPath, not contains(target): a click that swaps the menu's own rows has detached its target.
    if (branchMenu && branchBox && !event.composedPath().includes(branchBox)) branchMenu = false;
  }}
/>

{#snippet rows(list: GitRow[], side: Side)}
  {#each list as row (side + row.entry.path)}
    {@const parts = splitPath(row.entry.path)}
    <div
      class="row"
      class:selected={selected?.path === row.entry.path && selected?.side === side}
      class:conflict={row.entry.conflicted}
    >
      <button type="button" class="open" title={row.entry.path} onclick={() => onOpen(row)}>
        <span class="mark mark-{markOf(row)}" aria-hidden="true">{markOf(row)}</span>
        <span class="name">{parts.name}</span>
        {#if parts.dir}<span class="dir">{parts.dir}</span>{/if}
      </button>
      {#if side === "unstaged" && !row.entry.conflicted}
        <button
          type="button"
          class="toggle revert"
          disabled={busy}
          aria-label={row.untracked ? `Delete ${row.entry.path}` : `Discard changes in ${row.entry.path}`}
          title={row.untracked ? "Delete this untracked file" : "Discard the unstaged changes"}
          onclick={() => void revert(row)}><Icon name="undo-2" size="sm" /></button
        >
      {/if}
      <button
        type="button"
        class="toggle"
        disabled={busy}
        aria-label={side === "staged" ? `Unstage ${row.entry.path}` : `Stage ${row.entry.path}`}
        title={side === "staged" ? "Unstage" : "Stage"}
        onclick={() => void (side === "staged" ? unstage(row) : stage(row))}
        ><Icon name={side === "staged" ? "x" : "plus"} size="sm" /></button
      >
    </div>
  {/each}
{/snippet}

<section class="git" aria-label="Git">
  {#if !root}
    <EmptyState compact title="No project open" text="Open one from the project bar above." />
  {:else if notARepo}
    <EmptyState compact title="Not a git repository" text="This project folder has no history yet.">
      {#snippet action()}
        <button type="button" class="ax-btn primary" disabled={busy} onclick={() => void createRepo()}>Create repository</button>
      {/snippet}
    </EmptyState>
    {#if error}<p class="problem" role="alert">{error}</p>{/if}
  {:else if !status}
    <p class="note">{error || "Reading the repository…"}</p>
  {:else}
    <header>
      <span class="branch" bind:this={branchBox}>
        <button
          type="button"
          class="branch-button"
          aria-expanded={branchMenu}
          title={status.upstream ? `Follows ${status.upstream} — switch branch` : "No upstream — switch branch"}
          onclick={() => void openBranches()}
        >
          <Icon name="git-branch" size="sm" />
          <span class="name">{status.branch ?? `detached at ${status.head ?? "?"}`}</span>
          <span class="caret" class:turned={branchMenu}><Icon name="chevron-down" size="sm" /></span>
        </button>
        <!-- With a push button the ahead count is on it; without one (no remote) it stands here. -->
        {#if status.ahead > 0 && !push}<span class="count" title="Commits not on the upstream">↑ {status.ahead}</span>{/if}
        {#if status.behind > 0}<span class="count" title="Commits on the upstream you do not have">↓ {status.behind}</span>{/if}
        {#if branchMenu}
          <div class="branch-menu" role="menu">
            <div class="new-branch">
              <Icon name="plus" size="sm" />
              <input
                type="text"
                spellcheck="false"
                placeholder="New branch from here"
                bind:value={newBranch}
                onkeydown={(e) => {
                  if (e.key === "Enter") void makeBranch();
                }}
              />
              <button type="button" class="ax-btn" disabled={!newBranch.trim()} onclick={() => void makeBranch()}>Create</button>
            </div>
            {#each branchList as branch (branch.name)}
              <button
                type="button"
                role="menuitem"
                class:current={branch.current}
                disabled={branch.current}
                onclick={() => void switchTo(branch.name)}
              >
                <span class="check" aria-hidden="true">{#if branch.current}<Icon name="check" size="sm" />{/if}</span>
                <span class="name">{branch.name}</span>
                {#if branch.upstream}<small>{branch.upstream}</small>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </span>
      <span class="remote">
        <IconButton
          icon="refresh-cw"
          size="sm"
          label="Fetch from the remote (only reads)"
          disabled={fetching}
          onclick={() => void fetchRemote()}
        />
        {#if push}
          <button
            type="button"
            class="ax-btn push"
            disabled={!push.enabled || pushing || busy}
            title={push.title}
            onclick={() => void pushBranch()}>{pushing ? "Pushing…" : push.label}</button
          >
        {/if}
      </span>
    </header>

    <div class="scroll">
      {#if outgoing.length > 0}
        <div class="plate outgoing-plate">
        <div class="group-head">
          <button
            type="button"
            class="fold"
            aria-expanded={outgoingOpen}
            title="What a push would publish"
            onclick={() => (outgoingOpen = !outgoingOpen)}
          >
            <span class="caret" class:turned={outgoingOpen}><Icon name="chevron-right" size="sm" /></span>
            To push <span class="n">{outgoing.length}</span>
          </button>
        </div>
        {#if outgoingOpen}
          <ul class="outgoing">
            {#each outgoing as commit (commit.id)}
              <li>
                <button
                  type="button"
                  class="commit-line"
                  aria-expanded={openCommits[commit.id] === true}
                  onclick={() => (openCommits = { ...openCommits, [commit.id]: !openCommits[commit.id] })}
                >
                  <span class="sha">{commit.id}</span>
                  <span class="subject">{commit.subject}</span>
                  <small>{commit.when} · {commit.files.length} {commit.files.length === 1 ? "file" : "files"}</small>
                </button>
                {#if openCommits[commit.id]}
                  <ul class="files">
                    {#if commit.files.length === 0}<li class="muted">No file changes (a merge).</li>{/if}
                    {#each commit.files as file (file.path)}
                      <li><span class="mark">{file.status}</span> {file.path}</li>
                    {/each}
                  </ul>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        </div>
      {/if}
      <div class="plate staged-plate">
        <div class="group-head">
          <span class="title">Staged <span class="n">{groups.staged.length}</span></span>
          {#if groups.staged.length > 0}
            <button type="button" class="link" disabled={busy} onclick={() => void unstageAll()}>Unstage all</button>
          {/if}
        </div>
        {#if groups.staged.length === 0}<p class="empty">Nothing staged yet — the + on a change adds it.</p>{/if}
        {@render rows(groups.staged, "staged")}
      </div>

      <div class="plate">
        <div class="group-head">
          <span class="title">Changes <span class="n">{groups.changes.length}</span></span>
          {#if groups.changes.length > 0}
            <button type="button" class="link" disabled={busy} onclick={() => void stageAll()}>Stage all</button>
          {/if}
        </div>
        {#if groups.changes.length === 0}
          <p class="empty clean"><Icon name="circle-check" size="sm" /> The working tree is clean.</p>
        {/if}
        {@render rows(groups.changes, "unstaged")}
      </div>
    </div>

    <div class="commit">
      <textarea
        rows="3"
        placeholder={suggesting ? "Writing a message from the staged diff…" : "Commit message  (⌘↩ commits)"}
        spellcheck="true"
        bind:value={message}
        disabled={suggesting}
        onkeydown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void commit();
          }
        }}
      ></textarea>
      <div class="commit-buttons">
        <IconButton
          icon="sparkles"
          label={groups.staged.length === 0
            ? "Stage changes to have a message written"
            : "Write a message: sends the staged diff (not .env or key files) to the skill provider's model — one paid call"}
          disabled={suggesting || busy || groups.staged.length === 0}
          pressed={suggesting}
          onclick={() => void suggestMessage()}
        />
        <button type="button" class="ax-btn primary" disabled={!commitReady || busy || suggesting} onclick={() => void commit()}>
          <Icon name="git-commit-horizontal" size="sm" /> Commit {groups.staged.length > 0 ? `(${groups.staged.length})` : ""}
        </button>
        <button
          type="button"
          class="ax-btn"
          disabled={!commitReady || busy || pushing || suggesting || status.branch === null}
          title={status.branch === null ? "HEAD is detached; switch to a branch to push" : "Commit what is staged, then push the branch (never forced)"}
          onclick={() => void commitAndPush()}
        >
          <Icon name="arrow-up-from-line" size="sm" /> Commit &amp; Push
        </button>
      </div>
    </div>
    {#if error}<p class="problem" role="alert">{error}</p>{/if}
  {/if}
</section>

<style>
  .git {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  /* The branch as a pill that says it opens a menu, its distance to the upstream beside it, the remote on the right. */
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
  }

  .branch {
    position: relative;
    flex: 1 1 auto;
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    min-width: 0;
  }

  .branch-button {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-elev-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .branch-button:hover,
  .branch-button[aria-expanded="true"] {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-3);
  }

  .branch-button .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caret {
    display: inline-flex;
    color: var(--ax-text-muted);
    transition: transform var(--ax-dur-fast) var(--ax-ease);
  }

  .branch-button .caret.turned {
    transform: rotate(180deg);
  }

  .fold .caret.turned {
    transform: rotate(90deg);
  }

  .count {
    flex: none;
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .remote {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--ax-space-1);
  }

  .push {
    padding: var(--ax-space-1) var(--ax-space-3);
    white-space: nowrap;
  }

  .branch-menu {
    position: absolute;
    left: 0;
    top: calc(100% + var(--ax-space-1));
    z-index: 10;
    min-width: calc(240px * var(--ax-ui-scale));
    max-height: calc(320px * var(--ax-ui-scale));
    overflow: auto;
    padding: var(--ax-space-1);
    background: var(--ax-elev-3);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }

  .branch-menu > button {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .branch-menu > button:hover:not(:disabled) {
    background: var(--ax-selected-bg);
  }

  .branch-menu > button.current {
    color: var(--ax-text);
    font-weight: 600;
    cursor: default;
  }

  .branch-menu small {
    margin-left: auto;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .check {
    display: inline-flex;
    width: calc(14px * var(--ax-ui-scale));
    color: var(--ax-success);
  }

  /* Creating a branch leads the menu and stays in view while a long list scrolls under it. */
  .new-branch {
    position: sticky;
    top: calc(-1 * var(--ax-space-1));
    z-index: 1;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin: calc(-1 * var(--ax-space-1)) calc(-1 * var(--ax-space-1)) var(--ax-space-1);
    padding: var(--ax-space-2);
    border-bottom: 1px solid var(--ax-border);
    background: var(--ax-elev-3);
    color: var(--ax-text-muted);
  }

  .new-branch input {
    flex: 1;
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font: inherit;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3);
  }

  /* A group of files is a plate, like the Run view's groups; the files are rows on it. */
  .plate {
    flex: none;
    overflow: hidden;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  .staged-plate {
    border-left: calc(3px * var(--ax-ui-scale)) solid color-mix(in srgb, var(--ax-success) 70%, transparent);
  }

  .outgoing-plate {
    border-left: calc(3px * var(--ax-ui-scale)) solid color-mix(in srgb, var(--ax-harness-opencode) 70%, transparent);
  }

  .group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    background: var(--ax-elev-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .title,
  .fold {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .fold {
    padding: 0;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
    cursor: pointer;
  }

  .n {
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    font-weight: 400;
    letter-spacing: 0;
  }

  /* Quiet until pointed at: these are not the panel's one primary action (the accent rule). */
  .link {
    padding: 0 var(--ax-space-2);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font: inherit;
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    cursor: pointer;
  }

  .link:hover:not(:disabled) {
    border-color: var(--ax-border-strong);
    color: var(--ax-text);
  }

  .outgoing,
  .files {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .outgoing > li + li {
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 60%, transparent);
  }

  .commit-line {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-2) var(--ax-space-3);
    background: none;
    border: 0;
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .commit-line:hover {
    background: var(--ax-elev-3);
  }

  .commit-line .sha {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-harness-opencode);
  }

  .commit-line .subject {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .commit-line small {
    flex-basis: 100%;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .files {
    padding-bottom: var(--ax-space-2);
  }

  .files li {
    padding: 0 var(--ax-space-3) 0 calc(28px * var(--ax-ui-scale));
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    overflow-wrap: anywhere;
  }

  .files .mark {
    display: inline-block;
    width: 1.4em;
    background: none;
  }

  .files .muted {
    color: var(--ax-text-muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding-right: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .row + .row {
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 40%, transparent);
  }

  .row:hover {
    background: var(--ax-elev-3);
  }

  .row.selected {
    background: var(--ax-selected-bg);
  }

  .open {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-3);
    background: none;
    border: 0;
    color: var(--ax-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* The change's letter as a small badge in its colour: modified, new, deleted at a glance. */
  .mark {
    --mark: var(--ax-text-muted);
    flex-shrink: 0;
    display: inline-grid;
    place-items: center;
    width: calc(18px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-sm);
    background: color-mix(in srgb, var(--mark) 16%, transparent);
    color: var(--mark);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
  }

  .mark-M,
  .mark-R {
    --mark: var(--ax-harness-opencode);
  }

  .mark-A,
  .mark-U {
    --mark: var(--ax-success);
  }

  .mark-D,
  .mark-\! {
    --mark: var(--ax-danger);
  }

  .conflict .name {
    color: var(--ax-danger);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .toggle {
    flex-shrink: 0;
    display: inline-grid;
    place-items: center;
    width: calc(24px * var(--ax-ui-scale));
    height: calc(24px * var(--ax-ui-scale));
    padding: 0;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    cursor: pointer;
    opacity: 0.55;
  }

  .row:hover .toggle,
  .row:focus-within .toggle,
  .row.selected .toggle {
    opacity: 1;
  }

  .toggle:hover:not(:disabled) {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-2);
    color: var(--ax-text);
  }

  .revert:hover:not(:disabled) {
    color: var(--ax-danger);
  }

  .note,
  .empty,
  .problem {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }

  .note,
  .empty {
    color: var(--ax-text-muted);
  }

  .clean {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }

  .empty.clean {
    color: var(--ax-success);
  }

  .problem {
    color: var(--ax-warning);
  }

  /* The commit box sits on the panel's floor: its message, the primary Commit, and Commit & Push beside it. */
  .commit {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
    background: var(--ax-elev-2);
  }

  textarea {
    resize: vertical;
    min-height: calc(64px * var(--ax-ui-scale));
    padding: var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  textarea:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .commit-buttons {
    display: flex;
    gap: var(--ax-space-2);
  }

  .commit-buttons .ax-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--ax-space-1);
    white-space: nowrap;
  }
</style>
