<!--
  The project search (`docs/plans/editor.md`, ED5, T13, T14): a query over the
  files of one root, results grouped by file. One component for both places it
  lives — the file app's Search tab (with a choice of root) and the IDE's
  Search pane (fixed to the project).

  * **The rules are `projectSearch.ts`'s**; the backend is `file_search`
    (Rust, the `regex` crate — no backtracking, so no time limit).
  * **It searches as you type** (a short pause first); ⏎ searches at once.
    A newer query stops the one before.
  * **Files on disk, not open tabs**: a file with unsaved changes in any
    editor (`dirtyFiles.ts`) is marked ●, since what is listed is the saved
    state (T14).
  * **A click opens the file at the match** (`onOpen`); a file's heading folds
    its matches away.
  * **A language server's list takes the same place** (ED6.3,
    `locationList.ts`): the uses of a symbol, or its implementations, shown
    with `showLocations` until the owner searches again or closes it.
-->
<script lang="ts">
  import { onDestroy, tick } from "svelte";

  import type { FileRootInfo } from "../core/backend";
  import { cancelSearch, searchFiles, type FileMatches, type LineMatch } from "./backend";
  import { dirtyFiles, isDirty } from "./dirtyFiles";
  import type { LocationList } from "./locationList";
  import {
    DEFAULT_OPTIONS,
    ProjectSearchModel,
    statusText,
    type SearchOptions,
    type SearchView,
  } from "./projectSearch";

  interface Props {
    /** The roots to choose from; with one, there is no choice. */
    roots: Pick<FileRootInfo, "id" | "label">[];
    /** The root chosen first (the active file's, say). */
    initialRoot?: string | null;
    /** Opens `rel` of `root` with the cursor at the match. */
    onOpen: (root: string, rel: string, line: number, col: number) => void;
  }

  let { roots, initialRoot = null, onOpen }: Props = $props();

  /** The three switches beside the field. */
  const OPTION_BUTTONS: { key: "caseSensitive" | "wholeWord" | "regex"; label: string; title: string }[] = [
    { key: "caseSensitive", label: "Aa", title: "Match case" },
    { key: "wholeWord", label: "ab", title: "Whole word" },
    { key: "regex", label: ".*", title: "Regular expression" },
  ];

  /** Quiet time after the last key before the search starts. */
  const TYPING_PAUSE_MS = 250;

  let input = $state<HTMLInputElement | null>(null);
  let pattern = $state("");
  let options = $state<SearchOptions>({ ...DEFAULT_OPTIONS });
  let showGlobs = $state(false);
  let chosenRoot = $state<string | null>(null);
  let view = $state.raw<SearchView>({ status: "idle", files: [], matches: 0, truncated: false, error: null });
  let collapsed = $state(new Set<string>());
  /** A language server's list shown instead of the search's results. */
  let list = $state.raw<LocationList | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const root = $derived(
    roots.find((r) => r.id === chosenRoot)?.id ?? roots.find((r) => r.id === initialRoot)?.id ?? roots[0]?.id ?? null,
  );

  const model = new ProjectSearchModel({ search: searchFiles, cancel: cancelSearch }, crypto.randomUUID(), () => {
    view = model.view;
  });

  function run(): void {
    clearTimeout(timer);
    collapsed = new Set();
    list = null;
    if (root) void model.start(root, pattern, options);
  }

  function schedule(): void {
    clearTimeout(timer);
    timer = setTimeout(run, TYPING_PAUSE_MS);
  }

  function toggle(key: "regex" | "caseSensitive" | "wholeWord"): void {
    options = { ...options, [key]: !options[key] };
    run();
  }

  function toggleFile(key: string): void {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  /** The snippet in three parts: before, the match, after. */
  function parts(m: LineMatch): [string, string, string] {
    const a = Math.max(0, m.col - m.from);
    const b = Math.max(a, m.end - m.from);
    return [m.text.slice(0, a), m.text.slice(a, b), m.text.slice(b)];
  }

  function fileName(rel: string): string {
    return rel.split("/").pop() ?? rel;
  }

  function folderOf(rel: string): string {
    const i = rel.lastIndexOf("/");
    return i < 0 ? "" : rel.slice(0, i);
  }

  /** Shows a language server's list (ED6.3) in place of the search's results. */
  export function showLocations(next: LocationList): void {
    clearTimeout(timer);
    collapsed = new Set();
    list = next;
  }

  /** What the list shows: the language server's places, or the search's files (in the chosen root). */
  const shown = $derived(
    list
      ? list.files
      : root
        ? view.files.map((f: FileMatches) => ({ root, rel: f.rel, matches: f.matches }))
        : [],
  );

  /** Focuses the query field (⇧⌘F), taking `text` as the query if given. */
  export async function focus(text?: string): Promise<void> {
    if (text && text !== pattern) {
      pattern = text;
      run();
    }
    await tick();
    input?.focus();
    input?.select();
  }

  onDestroy(() => {
    clearTimeout(timer);
    model.stop();
  });
</script>

<div class="search">
  <div class="query">
    <input
      bind:this={input}
      bind:value={pattern}
      type="text"
      placeholder="Search"
      aria-label="Search in files"
      spellcheck="false"
      autocomplete="off"
      oninput={schedule}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          run();
        }
      }}
    />
    {#each OPTION_BUTTONS as b (b.key)}
      <button
        type="button"
        class="opt"
        class:word={b.key === "wholeWord"}
        aria-pressed={options[b.key]}
        title={b.title}
        onclick={() => toggle(b.key)}>{b.label}</button
      >
    {/each}
  </div>
  <div class="row">
    <button type="button" class="link" aria-expanded={showGlobs} onclick={() => (showGlobs = !showGlobs)}>
      {showGlobs ? "▾" : "▸"} include / exclude
    </button>
    {#if roots.length > 1}
      <select aria-label="Where to search" value={root} onchange={(e) => ((chosenRoot = e.currentTarget.value), run())}>
        {#each roots as r (r.id)}
          <option value={r.id}>{r.label}</option>
        {/each}
      </select>
    {/if}
  </div>
  {#if showGlobs}
    <input
      class="glob"
      type="text"
      placeholder="Include, e.g. *.rs, src/**"
      aria-label="Files to include"
      spellcheck="false"
      bind:value={options.include}
      oninput={schedule}
    />
    <input
      class="glob"
      type="text"
      placeholder="Exclude, e.g. **/*.test.ts"
      aria-label="Files to exclude"
      spellcheck="false"
      bind:value={options.exclude}
      oninput={schedule}
    />
  {/if}

  {#if list}
    <div class="list-head">
      <p class="status" role="status">{list.title} — {list.count} {list.count === 1 ? "place" : "places"}</p>
      <button type="button" class="link" aria-label="Close the list" onclick={() => (list = null)}>✕</button>
    </div>
  {:else if view.status !== "idle"}
    <p class="status" class:error={view.status === "error"} role="status">{statusText(view)}</p>
  {/if}

  <div class="results">
    {#each shown as file (`${file.root}\0${file.rel}`)}
      {@const key = `${file.root}\0${file.rel}`}
      <div class="file">
        <button type="button" class="file-head" aria-expanded={!collapsed.has(key)} onclick={() => toggleFile(key)}>
          <span class="chevron">{collapsed.has(key) ? "▸" : "▾"}</span>
          <span class="name">{fileName(file.rel)}</span>
          {#if isDirty($dirtyFiles, file.root, file.rel)}
            <span class="unsaved" title="Unsaved changes in an editor — these results are the file on disk"
              >●</span
            >
          {/if}
          <span class="folder">{folderOf(file.rel)}</span>
          <span class="count">{file.matches.length}</span>
        </button>
        {#if !collapsed.has(key)}
          {#each file.matches as m, i (i)}
            {@const [before, hit, after] = parts(m)}
            <button
              type="button"
              class="hit"
              title="{file.rel}:{m.line + 1}"
              onclick={() => onOpen(file.root, file.rel, m.line, m.col)}
            >
              <span class="line">{m.line + 1}</span>
              <span class="text">{before.trimStart()}<mark>{hit}</mark>{after}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    min-height: 0;
    height: 100%;
    padding: var(--ax-space-2);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  .query {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: 0 var(--ax-space-1) 0 0;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }

  .query:focus-within {
    border-color: var(--ax-accent);
  }

  input {
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: transparent;
    border: 0;
    color: var(--ax-text);
    font: inherit;
    outline: none;
  }

  .query input {
    flex: 1;
  }

  .glob {
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }

  .glob:focus {
    border-color: var(--ax-accent);
  }

  .opt {
    padding: 0 var(--ax-space-1);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .opt.word {
    text-decoration: underline;
  }

  .opt[aria-pressed="true"] {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
  }

  .link {
    padding: 0;
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  select {
    min-width: 0;
    max-width: 50%;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-xs);
  }

  .status {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
  }

  .status.error {
    color: var(--ax-danger);
  }

  .results {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .file-head,
  .hit {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    width: 100%;
    padding: 1px var(--ax-space-1);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font: inherit;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
  }

  .file-head:hover,
  .hit:hover {
    background: var(--ax-surface-2);
  }

  .chevron {
    width: 1em;
    color: var(--ax-text-muted);
  }

  .name {
    font-weight: 600;
  }

  .folder {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .count {
    padding: 0 var(--ax-space-1);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .unsaved {
    color: var(--ax-accent);
  }

  .hit {
    padding-left: calc(1em + var(--ax-space-3));
  }

  .line {
    min-width: 2.5em;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: pre;
  }

  mark {
    background: var(--ax-search-current);
    color: inherit;
    border-radius: var(--ax-radius-sm);
  }
</style>
