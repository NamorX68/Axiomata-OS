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
  import IconButton from "../ui/IconButton.svelte";
  import { gitApi, type RepoStatus, type Side } from "./gitBackend";
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

  async function commit(): Promise<void> {
    if (!commitReady || !root) return;
    const text = message;
    await act(async () => {
      await gitApi.commit(root!, text);
      message = "";
    });
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

<svelte:window onfocus={() => active && void read()} />

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
      <button
        type="button"
        class="toggle"
        disabled={busy}
        aria-label={side === "staged" ? `Unstage ${row.entry.path}` : `Stage ${row.entry.path}`}
        title={side === "staged" ? "Unstage" : "Stage"}
        onclick={() => void (side === "staged" ? unstage(row) : stage(row))}>{side === "staged" ? "−" : "+"}</button
      >
    </div>
  {/each}
{/snippet}

<section class="git" aria-label="Git">
  {#if !root}
    <p class="note">No project open.</p>
  {:else if notARepo}
    <p class="note">This project folder is not a git repository.</p>
  {:else if !status}
    <p class="note">{error || "Reading the repository…"}</p>
  {:else}
    <header>
      <span class="branch" title={status.upstream ? `Follows ${status.upstream}` : "No upstream"}>
        <span class="name">{status.branch ?? `detached at ${status.head ?? "?"}`}</span>
        {#if status.ahead > 0}<span class="count" title="Commits not on the upstream">↑{status.ahead}</span>{/if}
        {#if status.behind > 0}<span class="count" title="Commits on the upstream you do not have">↓{status.behind}</span>{/if}
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
      <div class="group-head">
        <span>Staged <small>{groups.staged.length}</small></span>
        {#if groups.staged.length > 0}
          <button type="button" class="link" disabled={busy} onclick={() => void unstageAll()}>Unstage all</button>
        {/if}
      </div>
      {#if groups.staged.length === 0}<p class="empty">Nothing staged.</p>{/if}
      {@render rows(groups.staged, "staged")}

      <div class="group-head">
        <span>Changes <small>{groups.changes.length}</small></span>
        {#if groups.changes.length > 0}
          <button type="button" class="link" disabled={busy} onclick={() => void stageAll()}>Stage all</button>
        {/if}
      </div>
      {#if groups.changes.length === 0}<p class="empty">No changes — the working tree is clean.</p>{/if}
      {@render rows(groups.changes, "unstaged")}
    </div>

    <div class="commit">
      <textarea
        rows="3"
        placeholder="Commit message"
        spellcheck="true"
        bind:value={message}
        onkeydown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void commit();
          }
        }}
      ></textarea>
      <div class="commit-buttons">
        <button type="button" class="ax-btn primary" disabled={!commitReady || busy} onclick={() => void commit()}>
          Commit {groups.staged.length > 0 ? `(${groups.staged.length})` : ""}
        </button>
        <button
          type="button"
          class="ax-btn"
          disabled={!commitReady || busy || pushing || status.branch === null}
          title={status.branch === null ? "HEAD is detached; switch to a branch to push" : "Commit what is staged, then push the branch (never forced)"}
          onclick={() => void commitAndPush()}
        >
          Commit &amp; Push
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

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
  }

  .branch {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    min-width: 0;
  }

  .branch .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text);
  }

  .remote {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .push {
    padding: 0 var(--ax-space-2);
  }

  .commit-buttons {
    display: flex;
    gap: var(--ax-space-2);
  }

  .commit-buttons .ax-btn {
    flex: 1;
  }

  .count {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .group-head small {
    margin-left: var(--ax-space-1);
  }

  .link {
    background: none;
    border: 0;
    color: var(--ax-accent);
    font: inherit;
    text-transform: none;
    cursor: pointer;
  }

  .row {
    display: flex;
    align-items: center;
    padding-right: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .row:hover,
  .row.selected {
    background: var(--ax-accent-muted);
  }

  .open {
    display: flex;
    flex: 1;
    align-items: baseline;
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

  .mark {
    flex-shrink: 0;
    width: calc(14px * var(--ax-ui-scale));
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    text-align: center;
    color: var(--ax-text-muted);
  }

  .mark-A,
  .mark-U {
    color: var(--ax-success, var(--ax-accent));
  }

  .mark-D,
  .mark-\!,
  .conflict .name {
    color: var(--ax-danger, var(--ax-warning));
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
    width: calc(22px * var(--ax-ui-scale));
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-md, 1em);
    cursor: pointer;
  }

  .toggle:hover:not(:disabled) {
    background: var(--ax-surface-2);
    color: var(--ax-text);
  }

  .note,
  .empty,
  .problem {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
  }

  .note,
  .empty {
    color: var(--ax-text-muted);
  }

  .problem {
    color: var(--ax-warning);
  }

  .commit {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  textarea {
    resize: vertical;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }
</style>
