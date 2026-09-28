<!--
  Quick open (`docs/plans/editor.md`, ED4, W8, W14): ⌘P, a name typed, a file
  opened. Matches every file of the given roots (the file app: workspace,
  projects, picked folders; the IDE: its project) by `quickOpen.ts`'s fuzzy
  ranking, recently opened files first.

  * ⏎ opens a tab of its own, ⌥⏎ the preview tab; `name:12` jumps to line 12.
  * ↑/↓ move, Esc closes.
  * **Indexes are kept per root** for the session: served at once, read again
    in the background once they are older than 30 s — no watcher over whole
    trees (W14).
-->
<script lang="ts" module>
  import { indexFiles } from "./backend";
  import { isFresh, type CachedIndex } from "./quickOpen";

  /**
   * One index per root, shared by every quick open in the app. Plain maps, not
   * Svelte state: a `$derived` that reads them must also read the instance's
   * `arrived` counter, or it never sees an index land.
   */
  const indexes = new Map<string, CachedIndex>();
  const loading = new Map<string, Promise<void>>();

  function refreshIndex(root: string): Promise<void> {
    const running = loading.get(root);
    if (running) return running;
    const next = indexFiles(root)
      .then((index) => {
        indexes.set(root, { ...index, at: Date.now() });
      })
      .catch(() => undefined)
      .finally(() => loading.delete(root));
    loading.set(root, next);
    return next;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";

  import type { FileRootInfo } from "../core/backend";
  import { parseQuery, rankFiles, type Candidate, type Ranked } from "./quickOpen";
  import type { FileRef } from "./tabs";

  interface Props {
    roots: FileRootInfo[];
    /** Recently opened files, most recent first. */
    recent: FileRef[];
    onOpen: (file: FileRef, preview: boolean, line: number | null) => void;
    onClose: () => void;
  }

  let { roots, recent, onOpen, onClose }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  /** Bumped when an index arrives: the results are ranked again. */
  let arrived = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  const candidates = $derived.by((): Candidate[] => {
    void arrived;
    return roots.flatMap((r) => (indexes.get(r.id)?.files ?? []).map((rel) => ({ root: r.id, rel })));
  });
  const parsed = $derived(parseQuery(query));
  const results = $derived<Ranked[]>(
    rankFiles(
      parsed.text,
      candidates,
      recent.map((f) => `${f.root}\0${f.rel}`),
    ),
  );
  const loadingAny = $derived.by(() => {
    void arrived;
    return roots.some((r) => !indexes.has(r.id));
  });
  const truncated = $derived.by(() => {
    void arrived;
    return roots.some((r) => indexes.get(r.id)?.truncated);
  });

  $effect(() => {
    void results;
    selected = 0;
  });

  function label(root: string): string {
    return roots.find((r) => r.id === root)?.label ?? root;
  }

  function open(result: Ranked | undefined, preview: boolean): void {
    if (!result) return;
    onOpen({ root: result.root, rel: result.rel }, preview, parsed.line);
    onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = results.length;
      if (n > 0) selected = (selected + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if (e.key === "Enter") {
      e.preventDefault();
      open(results[selected], e.altKey);
    }
  }

  /** The matched letters of `rel` marked, for the list. */
  function pieces(result: Ranked): Array<{ text: string; hit: boolean }> {
    const hits = new Set(result.positions);
    const out: Array<{ text: string; hit: boolean }> = [];
    for (let i = 0; i < result.rel.length; i++) {
      const hit = hits.has(i);
      const last = out[out.length - 1];
      if (last && last.hit === hit) last.text += result.rel[i];
      else out.push({ text: result.rel[i], hit });
    }
    return out;
  }

  onMount(() => {
    input?.focus();
    const now = Date.now();
    for (const root of roots) {
      if (!isFresh(indexes.get(root.id), now)) void refreshIndex(root.id).then(() => arrived++);
    }
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="backdrop" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="quick-open" role="dialog" aria-label="Open a file" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <input
      bind:this={input}
      bind:value={query}
      onkeydown={onKeydown}
      placeholder="File name — :12 for a line"
      aria-label="File name"
      spellcheck="false"
      autocomplete="off"
    />
    <ul role="listbox" aria-label="Files">
      {#each results as result, i (result.root + "\0" + result.rel)}
        <li role="option" aria-selected={i === selected}>
          <button
            type="button"
            class:selected={i === selected}
            onmousemove={() => (selected = i)}
            onclick={(e) => open(result, e.altKey)}
          >
            <span class="path">
              {#each pieces(result) as piece, p (p)}<span class:hit={piece.hit}>{piece.text}</span>{/each}
            </span>
            {#if roots.length > 1}<span class="root">{label(result.root)}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="note">{loadingAny ? "Reading the folders…" : "No file matches."}</li>
      {/each}
    </ul>
    {#if truncated}<p class="note">Some folders have more files than quick open reads.</p>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: calc(var(--ax-z-staging) + 2);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }

  .quick-open {
    display: flex;
    flex-direction: column;
    width: min(calc(640px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-5)));
    max-height: 60vh;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    overflow: hidden;
  }

  input {
    padding: var(--ax-space-3) var(--ax-space-4);
    background: var(--ax-surface-1);
    border: 0;
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-base);
    outline: none;
  }

  ul {
    margin: 0;
    padding: var(--ax-space-1);
    list-style: none;
    overflow-y: auto;
  }

  button {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-3);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-3);
    background: none;
    border: 0;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  button.selected {
    background: var(--ax-accent-muted);
    color: var(--ax-text);
  }

  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hit {
    color: var(--ax-accent);
    font-weight: 600;
  }

  .root {
    flex-shrink: 0;
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
  }

  .note {
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
