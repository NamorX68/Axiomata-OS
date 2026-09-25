<!--
  todo — a flat checklist backed by one fixed workspace file, `ToDo.md`
  (workspace root). Simple, date-free tasks only; the Apple-Reminders module
  is a separate future thing. All list logic is in `core/todo.ts` (parsed /
  serialised GFM task lists); this shell just loads, renders and writes back.

  - Open tasks live above a `## Done` heading, completed ones below it,
    stamped `(done: YYYY-MM-DD)` for a later cleanup skill.
  - Polls every POLL_MS and reloads; a write is skipped when the serialised
    result is byte-identical to what was last read (so a hand-edit to the
    file is not clobbered by an idle poll). No conflict detection beyond that
    — same trade-off as the old Document viewer had.
  - Config (flip side): `showDone` — whether the Done section starts open.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { WorkspaceFile } from "../core/backend";
  import {
    addTodo,
    completeTodo,
    deleteDone,
    deleteOpen,
    editDone,
    editOpen,
    parseTodoDoc,
    reopenTodo,
    serializeTodoDoc,
    TODO_PATH,
    todayIso,
    type TodoDoc,
  } from "../core/todo";
  import type { ModuleContext } from "../core/types";

  const POLL_MS = 5000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let doc = $state<TodoDoc>({ open: [], done: [] });
  let lastLoaded = $state<string | null>(null);
  let error = $state("");
  let draft = $state("");
  // Inline-edit state: at most one open-list and one done-list item edited
  // at a time (index into the respective array, or null when idle).
  let editingOpen = $state<number | null>(null);
  let editingDone = $state<number | null>(null);
  let editText = $state("");
  // Seeded from config, then re-synced whenever the settings face's
  // checkbox changes it -- `$state(expr)` only evaluates `expr` once at
  // init, so without this effect flipping the checkbox had no visible
  // effect on an already-mounted tile (the front stays mounted across a
  // flip, so this is the common case, not just first load). The front's
  // own toggle button still works independently in between: it only
  // writes this local variable, not `config`, so it doesn't fight the
  // effect until the setting actually changes again.
  let showDone = $state($config.showDone === true);
  $effect(() => {
    showDone = $config.showDone === true;
  });

  function isNotFound(message: string): boolean {
    return /no such file|not found|does not exist/i.test(message);
  }

  async function load() {
    try {
      const file = await ctx.invoke<WorkspaceFile>("read_workspace_file", { rel: TODO_PATH });
      lastLoaded = file.content;
      doc = parseTodoDoc(file.content);
      error = "";
    } catch (err) {
      const message = String(err);
      if (isNotFound(message)) {
        // No ToDo.md yet — that is the normal first-run state, not an error.
        lastLoaded = null;
        doc = { open: [], done: [] };
        error = "";
      } else {
        error = message;
      }
    }
  }

  async function persist(next: TodoDoc) {
    const serialized = serializeTodoDoc(next);
    doc = next;
    if (serialized === lastLoaded) return;
    try {
      await ctx.invoke("write_workspace_file", { rel: TODO_PATH, content: serialized });
      lastLoaded = serialized;
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  function add() {
    const text = draft.trim();
    if (!text) return;
    draft = "";
    void persist(addTodo(doc, text));
  }

  function startEditOpen(i: number) {
    editingDone = null;
    editingOpen = i;
    editText = doc.open[i].text;
  }
  function commitEditOpen() {
    if (editingOpen === null) return;
    const i = editingOpen;
    const text = editText;
    editingOpen = null;
    void persist(editOpen(doc, i, text));
  }

  function startEditDone(i: number) {
    editingOpen = null;
    editingDone = i;
    editText = doc.done[i].text;
  }
  function commitEditDone() {
    if (editingDone === null) return;
    const i = editingDone;
    const text = editText;
    editingDone = null;
    void persist(editDone(doc, i, text));
  }

  function cancelEdit() {
    editingOpen = null;
    editingDone = null;
  }

  // Svelte action: focuses and selects the edit `<input>` the moment it mounts.
  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  onMount(() => {
    void load();
    const id = setInterval(() => void load(), POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="todo">
  <div class="head">
    <span class="count"><strong>{doc.open.length}</strong> open</span>
    <span class="muted">· {doc.done.length} done</span>
    <span class="spacer"></span>
    <button type="button" class="reload" title="Reload" aria-label="Reload" onclick={() => void load()}>↻</button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  <ul class="list open">
    {#each doc.open as item, i (i + " " + item.text)}
      <li>
        {#if editingOpen === i}
          <input
            type="text"
            class="edit-input"
            bind:value={editText}
            use:autofocus
            onblur={commitEditOpen}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitEditOpen();
              } else if (e.key === "Escape") {
                e.preventDefault();
                cancelEdit();
              }
            }}
          />
        {:else}
          <input
            type="checkbox"
            checked={false}
            aria-label={`Mark "${item.text}" done`}
            onchange={() => void persist(completeTodo(doc, i, todayIso()))}
          />
          <span class="text">{item.text}</span>
          <button type="button" class="edit" aria-label={`Edit "${item.text}"`} onclick={() => startEditOpen(i)}>✎</button>
          <button type="button" class="del" aria-label={`Delete "${item.text}"`} onclick={() => void persist(deleteOpen(doc, i))}>✕</button>
        {/if}
      </li>
    {/each}
    {#if doc.open.length === 0}
      <li class="empty muted">Nothing open. Add a task below.</li>
    {/if}
  </ul>

  {#if doc.done.length > 0}
    <button type="button" class="done-toggle" onclick={() => (showDone = !showDone)} aria-expanded={showDone}>
      {showDone ? "▾" : "▸"} Done · {doc.done.length}
    </button>
    {#if showDone}
      <ul class="list done">
        {#each doc.done as item, i (i + " " + item.text)}
          <li>
            {#if editingDone === i}
              <input
                type="text"
                class="edit-input"
                bind:value={editText}
                use:autofocus
                onblur={commitEditDone}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitEditDone();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    cancelEdit();
                  }
                }}
              />
            {:else}
              <input
                type="checkbox"
                checked={true}
                aria-label={`Reopen "${item.text}"`}
                onchange={() => void persist(reopenTodo(doc, i))}
              />
              <span class="text struck">{item.text}</span>
              {#if item.doneOn}<span class="date muted">{item.doneOn}</span>{/if}
              <button type="button" class="edit" aria-label={`Edit "${item.text}"`} onclick={() => startEditDone(i)}>✎</button>
              <button type="button" class="del" aria-label={`Delete "${item.text}"`} onclick={() => void persist(deleteDone(doc, i))}>✕</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  <form class="add" onsubmit={(e) => (e.preventDefault(), add())}>
    <input type="text" placeholder="New task…" bind:value={draft} />
    <button type="submit" aria-label="Add task" disabled={!draft.trim()}>+</button>
  </form>
</div>

<style>
  .todo {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    padding: var(--ax-space-3);
    gap: var(--ax-space-2);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .count strong {
    font-size: var(--ax-font-size-xl);
    color: var(--ax-accent);
    font-weight: 700;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .reload {
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
  }
  .list.open {
    flex: 1 1 auto;
  }
  .list.done {
    flex: 0 1 auto;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) 0;
  }
  .list li.empty {
    padding: var(--ax-space-2) 0;
  }
  .text {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .edit-input {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    color: inherit;
  }
  .struck {
    text-decoration: line-through;
    color: var(--ax-text-muted);
  }
  .date {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    flex: 0 0 auto;
  }
  input[type="checkbox"] {
    accent-color: var(--ax-accent);
    flex: 0 0 auto;
  }
  .del,
  .edit {
    flex: 0 0 auto;
    padding: 0 var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    opacity: 0;
  }
  .list li:hover .del,
  .list li:hover .edit,
  .del:focus-visible,
  .edit:focus-visible {
    opacity: 1;
  }

  .add {
    display: flex;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .add input {
    flex: 1 1 auto;
    min-width: 0;
  }

  .done-toggle {
    align-self: flex-start;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    flex: 0 0 auto;
  }

  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin: 0;
  }
  p {
    margin: 0;
  }
</style>
