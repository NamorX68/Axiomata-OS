<!--
  The find bar (`docs/plans/editor.md`, ED5, T4, T15): floats at the top right
  of an editor surface. All it does is in `FindModel`; this is the fields, the
  toggles and the keys.

  * **Query field**: ⏎ next, ⇧⏎ previous, ⌥⏎ every match a cursor, ⌘G / ⇧⌘G
    too; ⌥⌘C match case, ⌥⌘W whole word, ⌥⌘R regular expression, ⌥⌘L in
    selection; Esc closes and gives the text back its focus.
  * **Replace field** (⌥⌘F, not in diffs or read-only files): ⏎ replaces the
    current match and goes on, ⌥⌘⏎ replaces all (one undo step), ⌥⌘P keeps the
    case of what it replaces.
  * The ⌥ shortcuts are read from `code`: ⌥ turns the letter into another
    character on a Mac, and these letters sit in the same place on a German
    keyboard.
  * `version` is the surface's redraw counter — the model is a plain class.
-->
<script lang="ts">
  import type { FindModel } from "../editor/search/findModel";

  interface Props {
    model: FindModel;
    version: number;
    /** The replace row may be shown (not in diffs or read-only files). */
    canReplace: boolean;
    showReplace: boolean;
    onToggleReplace: () => void;
    onClose: () => void;
  }

  let { model, version, canReplace, showReplace, onToggleReplace, onClose }: Props = $props();

  let queryInput = $state<HTMLInputElement | null>(null);
  let replaceInput = $state<HTMLInputElement | null>(null);

  const view = $derived.by(() => {
    void version;
    return {
      query: model.query,
      replacement: model.replacement,
      options: model.options,
      scope: model.scope !== null,
      status: model.status(),
      message: model.message,
      valid: model.valid,
    };
  });

  const count = $derived.by(() => {
    const s = view.status;
    if (s.kind === "empty") return { text: "", error: false, title: "" };
    if (s.kind === "pending") return { text: "…", error: false, title: "Searching" };
    if (s.kind === "error") return { text: "Invalid", error: true, title: s.message };
    if (s.count === 0) return { text: "No results", error: true, title: "" };
    const total = `${s.count.toLocaleString("en")}${s.truncated ? "+" : ""}`;
    return { text: s.index === null ? `${total} found` : `${s.index + 1}/${total}`, error: false, title: "" };
  });

  /** Focuses the query field, its text selected — ⌘F again selects it for typing over. */
  export function focusQuery(): void {
    queryInput?.focus();
    queryInput?.select();
  }

  export function focusReplace(): void {
    replaceInput?.focus();
    replaceInput?.select();
  }

  /** Keys both fields share; returns whether it was one. */
  function commonKey(e: KeyboardEvent): boolean {
    if (e.key === "Escape") {
      onClose();
      return true;
    }
    if (e.metaKey && !e.altKey && e.key.toLowerCase() === "g") {
      model.next(e.shiftKey);
      return true;
    }
    if (e.metaKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "f") {
      focusQuery();
      return true;
    }
    if (!e.metaKey || !e.altKey) return false;
    switch (e.code) {
      case "KeyC":
        model.setOptions({ caseSensitive: !model.options.caseSensitive });
        return true;
      case "KeyW":
        model.setOptions({ wholeWord: !model.options.wholeWord });
        return true;
      case "KeyR":
        model.setOptions({ regex: !model.options.regex });
        return true;
      case "KeyL":
        model.toggleScope();
        return true;
      case "KeyP":
        model.setOptions({ preserveCase: !model.options.preserveCase });
        return true;
      case "KeyF":
        if (!canReplace) return false;
        if (!showReplace) onToggleReplace();
        queueMicrotask(focusReplace);
        return true;
    }
    return false;
  }

  function onQueryKey(e: KeyboardEvent): void {
    if (e.isComposing) return;
    let handled = commonKey(e);
    if (!handled && e.key === "Enter") {
      if (e.altKey) model.selectAll();
      else model.next(e.shiftKey);
      handled = true;
    }
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function onReplaceKey(e: KeyboardEvent): void {
    if (e.isComposing) return;
    let handled = commonKey(e);
    if (!handled && e.key === "Enter") {
      if (e.metaKey && e.altKey) model.replaceAll();
      else model.replaceOne();
      handled = true;
    }
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<div class="find-bar" role="search">
  <div class="line">
    {#if canReplace}
      <button
        type="button"
        class="twist"
        class:open={showReplace}
        title="Toggle replace (⌥⌘F)"
        aria-label="Toggle replace"
        aria-expanded={showReplace}
        onclick={onToggleReplace}>›</button
      >
    {/if}
    <div class="field">
      <input
        bind:this={queryInput}
        value={view.query}
        placeholder="Find"
        aria-label="Find"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        oninput={(e) => model.setQuery(e.currentTarget.value)}
        onkeydown={onQueryKey}
      />
      <button
        type="button"
        class="toggle"
        class:on={view.options.caseSensitive}
        title="Match case (⌥⌘C)"
        aria-pressed={view.options.caseSensitive}
        onclick={() => model.setOptions({ caseSensitive: !model.options.caseSensitive })}>Aa</button
      >
      <button
        type="button"
        class="toggle word"
        class:on={view.options.wholeWord}
        title="Whole word (⌥⌘W)"
        aria-pressed={view.options.wholeWord}
        onclick={() => model.setOptions({ wholeWord: !model.options.wholeWord })}>ab</button
      >
      <button
        type="button"
        class="toggle"
        class:on={view.options.regex}
        title="Regular expression (⌥⌘R)"
        aria-pressed={view.options.regex}
        onclick={() => model.setOptions({ regex: !model.options.regex })}>.*</button
      >
    </div>
    <span class="count" class:error={count.error} title={count.title}>{count.text}</span>
    <button type="button" class="icon" title="Previous match (⇧⏎, ⇧⌘G)" aria-label="Previous match" onclick={() => model.next(true)}
      >↑</button
    >
    <button type="button" class="icon" title="Next match (⏎, ⌘G)" aria-label="Next match" onclick={() => model.next()}
      >↓</button
    >
    <button
      type="button"
      class="icon"
      class:on={view.scope}
      title="Find in selection (⌥⌘L)"
      aria-label="Find in selection"
      aria-pressed={view.scope}
      onclick={() => model.toggleScope()}>⌶</button
    >
    <button type="button" class="icon" title="Close (Esc)" aria-label="Close" onclick={onClose}>×</button>
  </div>
  {#if canReplace && showReplace}
    <div class="line replace">
      <div class="field">
        <input
          bind:this={replaceInput}
          value={view.replacement}
          placeholder={view.options.regex ? "Replace ($1, $&)" : "Replace"}
          aria-label="Replace"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          oninput={(e) => (model.replacement = e.currentTarget.value)}
          onkeydown={onReplaceKey}
        />
        <button
          type="button"
          class="toggle"
          class:on={view.options.preserveCase}
          title="Preserve case (⌥⌘P)"
          aria-pressed={view.options.preserveCase}
          onclick={() => model.setOptions({ preserveCase: !model.options.preserveCase })}>AB</button
        >
      </div>
      <button type="button" class="text" disabled={!view.valid} title="Replace (⏎)" onclick={() => model.replaceOne()}
        >Replace</button
      >
      <button type="button" class="text" disabled={!view.valid} title="Replace all (⌥⌘⏎)" onclick={() => model.replaceAll()}
        >All</button
      >
    </div>
  {/if}
  {#if view.message}
    <div class="message">{view.message}</div>
  {/if}
</div>

<style>
  .find-bar {
    position: absolute;
    top: var(--ax-space-1);
    /* Left of the minimap, when the surface has one (ED5.6). */
    right: calc(var(--ax-space-4) + var(--minimap-w, 0px));
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    max-width: calc(100% - 2 * var(--ax-space-4));
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    line-height: normal;
    cursor: default;
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    min-width: 0;
  }

  /* The replace row lines its field up under the query's, past the twist. */
  .line.replace {
    padding-left: calc(var(--ax-font-size-base) + var(--ax-space-1) + 2px);
  }

  .field {
    display: flex;
    align-items: center;
    flex: 1 1 220px;
    min-width: 80px;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }

  .field:focus-within {
    border-color: var(--ax-accent);
  }

  input {
    flex: 1;
    min-width: 0;
    width: 180px;
    padding: 3px var(--ax-space-2);
    background: none;
    border: 0;
    outline: none;
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
  }

  button {
    flex-shrink: 0;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .toggle {
    min-width: 22px;
    padding: 1px 3px;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .toggle.word {
    text-decoration: underline;
  }

  .toggle.on,
  .icon.on {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }

  .icon {
    width: 22px;
    height: 22px;
    padding: 0;
  }

  .twist {
    width: var(--ax-font-size-base);
    padding: 0;
    transition: transform 120ms ease;
  }

  .twist.open {
    transform: rotate(90deg);
  }

  .text {
    padding: 2px var(--ax-space-2);
    border-color: var(--ax-border);
  }

  .count {
    flex-shrink: 0;
    min-width: 64px;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-variant-numeric: tabular-nums;
    text-align: center;
    white-space: nowrap;
  }

  .count.error {
    color: var(--ax-danger);
  }

  .message {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    padding-left: var(--ax-space-1);
  }
</style>
