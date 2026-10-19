<!--
  A model field that offers the provider's catalog (board card #17): type to narrow the list, arrows + Enter or a
  click to pick. A typed name that the catalog does not know is kept (owner, 2026-10-19: the free entry stays as a
  way out) and marked as such. The list opens inline below the field rather than floating, so a scrolling dialog
  never clips it.
-->
<script lang="ts">
  import { type ModelOption, filterOptions } from "../core/modelCatalog";
  import IconButton from "./IconButton.svelte";

  interface Props {
    value: string;
    options: ModelOption[];
    placeholder?: string;
    /** The field's accessible name; the visible label is the caller's. */
    label: string;
    /** Shown with a ↻ button when set: fetch the provider's list again. */
    onRefresh?: () => void;
    refreshing?: boolean;
    /** A line under the field: when the list was fetched, or why it failed. */
    status?: string;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(""),
    options,
    placeholder = "",
    label,
    onRefresh,
    refreshing = false,
    status = "",
    onchange,
  }: Props = $props();

  /** Ties the field to its list for assistive technology; unique per picker on the page. */
  const listId = `model-picker-${Math.random().toString(36).slice(2)}`;
  let open = $state(false);
  /** What narrows the list: empty right after focusing, so the whole catalog shows, then the typed text. */
  let query = $state("");
  let active = $state(0);
  /** Whether the owner chose an entry with the arrows or the pointer; only then does Enter pick it. Otherwise Enter
   *  keeps what was typed — typing `claude-sonnet-5` must not turn into the first hit, `claude-sonnet-5-5`. */
  let chosen = $state(false);

  const hits = $derived(filterOptions(options, query));
  const unknown = $derived(value.trim() !== "" && options.length > 0 && !options.some((o) => o.value === value.trim()));

  function set(next: string) {
    value = next;
    onchange?.(next);
  }

  function pick(option: ModelOption) {
    set(option.value);
    open = false;
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      open = true;
      e.preventDefault();
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") {
      active = chosen ? Math.min(active + 1, hits.length - 1) : 0;
      chosen = true;
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      active = Math.max(active - 1, 0);
      chosen = true;
      e.preventDefault();
    } else if (e.key === "Enter") {
      const option = chosen ? hits[active] : undefined;
      if (option) pick(option);
      else open = false;
      e.preventDefault();
    } else if (e.key === "Escape") {
      // Closes the list only, not the dialog around it.
      open = false;
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<div class="model-picker">
  <div class="row">
    <input
      type="text"
      spellcheck="false"
      autocomplete="off"
      role="combobox"
      aria-label={label}
      aria-controls={listId}
      aria-expanded={open}
      aria-autocomplete="list"
      {placeholder}
      {value}
      onfocus={() => {
        query = "";
        active = 0;
        chosen = false;
        open = true;
      }}
      onblur={() => (open = false)}
      oninput={(e) => {
        set(e.currentTarget.value);
        query = e.currentTarget.value;
        active = 0;
        chosen = false;
        open = true;
      }}
      {onkeydown}
    />
    {#if onRefresh}
      <IconButton icon="refresh-cw" label="Fetch the model list again" size="sm" disabled={refreshing} onclick={onRefresh} />
    {/if}
  </div>
  {#if open && hits.length > 0}
    <ul class="list" id={listId} role="listbox" aria-label={label}>
      {#each hits as option, i (option.value)}
        <!-- mousedown, not click: a click would blur the field first and close the list before it lands. -->
        <li
          role="option"
          aria-selected={chosen && i === active}
          class:active={chosen && i === active}
          onmousedown={(e) => {
            e.preventDefault();
            pick(option);
          }}
          onmouseenter={() => {
            active = i;
            chosen = true;
          }}
        >
          <span class="value">{option.value}</span>
          {#if option.label}<span class="name">{option.label}</span>{/if}
          {#if option.hint}<span class="hint">{option.hint}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if unknown}
    <span class="note">Not in the provider's catalog — kept as typed.</span>
  {:else if refreshing}
    <span class="note">Fetching the model list…</span>
  {:else if status}
    <span class="note">{status}</span>
  {/if}
</div>

<style>
  .model-picker {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
  }
  input {
    flex: 1 1 auto;
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  input:focus {
    outline: none;
    border-color: var(--ax-accent);
  }
  .list {
    margin: 0;
    padding: var(--ax-space-1) 0;
    list-style: none;
    max-height: calc(240px * var(--ax-ui-scale));
    overflow-y: auto;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    cursor: pointer;
  }
  li.active {
    background: var(--ax-selected-bg);
  }
  .value {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
  }
  .name,
  .hint,
  .note {
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .hint {
    margin-left: auto;
  }
</style>
