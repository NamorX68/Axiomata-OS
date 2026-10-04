<script lang="ts">
  /**
   * Kanban — managing the boards themselves (create, rename, delete, pick) and how cards look. It was the flip
   * side of the Kanban tile; the tile is gone (owner, 2026-10-04), so it is a popover of the board panel now.
   *
   * Board management lives in a popover rather than in the board's header because you switch boards rarely — that
   * is a setting. Columns are the opposite and are shaped directly on the board: you rearrange them while looking
   * at them. The asymmetry is deliberate; the handling follows the use, not the symmetry.
   *
   * The card treatment is here for the same reason — it is set once and then left alone — and applies to the whole
   * application (`kanbanPrefs.ts`).
   */
  import { onMount } from "svelte";

  import { invokeBackend as invoke, type Board } from "../core/backend";
  import { forgetBoard, refreshBoard } from "../core/boardStore";
  import { cardStripes, setCardStripes } from "./kanbanPrefs";

  let {
    current,
    onChoose,
    onDeleted,
    onChanged = () => {},
  }: {
    /** The board the panel shows. */
    current: number | null;
    /** The owner picked or created a board. */
    onChoose: (id: number) => void;
    /** The board the panel shows was deleted. */
    onDeleted: () => void;
    /** A board was created, renamed or deleted — the panel's switcher re-reads the list. */
    onChanged?: () => void;
  } = $props();

  let boards = $state<Board[]>([]);
  let error = $state("");
  let busy = $state(false);
  let newName = $state("");
  /** Board id awaiting a confirmed delete, with its card count. */
  let confirming = $state<{ id: number; cards: number } | null>(null);

  async function load() {
    try {
      boards = await invoke<Board[]>("list_boards");
      error = "";
    } catch (err) {
      error = String(err);
    }
  }

  onMount(load);

  async function create() {
    const name = newName.trim();
    if (!name || busy) return;
    busy = true;
    try {
      const created = await invoke<Board>("create_board", { name });
      newName = "";
      onChoose(created.id);
      await load();
      onChanged();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function rename(board: Board, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === board.name) return;
    try {
      await invoke("rename_board", { id: board.id, name: trimmed });
      await Promise.all([load(), refreshBoard(board.id)]);
      onChanged();
    } catch (err) {
      error = String(err);
    }
  }

  /** Two steps on purpose: deleting a board takes every card on it. */
  async function askDelete(board: Board) {
    try {
      const cards = await invoke<number>("count_board_cards", { boardId: board.id });
      confirming = { id: board.id, cards };
    } catch (err) {
      error = String(err);
    }
  }

  async function confirmDelete(id: number) {
    busy = true;
    try {
      await invoke("delete_board", { id });
      forgetBoard(id);
      confirming = null;
      await load();
      onChanged();
      if (current === id) onDeleted();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="boards">
  <h3>Brett</h3>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if boards.length === 0}
    <p class="hint">Noch kein Brett angelegt.</p>
  {/if}

  <ul>
    {#each boards as board (board.id)}
      <li class:on={current === board.id}>
        <label>
          <input
            type="radio"
            name="kanban-board"
            checked={current === board.id}
            onchange={() => onChoose(board.id)}
          />
          <input
            class="name"
            value={board.name}
            aria-label="Name des Bretts"
            onblur={(event) => rename(board, event.currentTarget.value)}
          />
        </label>
        <button class="remove" onclick={() => askDelete(board)} aria-label="Brett löschen">✕</button>
      </li>

      {#if confirming?.id === board.id}
        <li class="confirm">
          <span>
            {confirming.cards === 0
              ? "Brett löschen?"
              : `${confirming.cards} ${confirming.cards === 1 ? "Karte wird" : "Karten werden"} mitgelöscht.`}
          </span>
          <button class="danger" disabled={busy} onclick={() => confirmDelete(board.id)}>
            Löschen
          </button>
          <button onclick={() => (confirming = null)}>Abbrechen</button>
        </li>
      {/if}
    {/each}
  </ul>

  <form
    onsubmit={(event) => {
      event.preventDefault();
      void create();
    }}
  >
    <input placeholder="Neues Brett …" bind:value={newName} aria-label="Name des neuen Bretts" />
    <button type="submit" disabled={busy || newName.trim() === ""}>Anlegen</button>
  </form>

  <h3>Karten</h3>
  <label class="stripes">
    <input type="checkbox" checked={$cardStripes} onchange={(e) => setCardStripes(e.currentTarget.checked)} />
    <span>Farbstreifen nach dem ersten Label</span>
  </label>
  <p class="hint">Gilt für alle Bretter.</p>
</div>

<style>
  .boards {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    overflow-y: auto;
    max-height: 70vh;
  }
  h3 {
    margin: 0;
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .error {
    margin: 0;
    color: var(--ax-danger);
    font-size: var(--ax-font-size-xs);
  }
  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  li.on .name {
    color: var(--ax-accent);
  }
  label {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    min-width: 0;
  }
  input.name,
  form input {
    flex: 1 1 auto;
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: inherit;
    font: inherit;
  }
  button {
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--ax-text);
    border-color: var(--ax-border-strong);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.remove {
    border: 0;
  }
  button.danger {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }
  li.confirm {
    padding: var(--ax-space-2);
    background: var(--ax-surface-2);
    border-radius: var(--ax-radius-sm);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  li.confirm span {
    flex: 1 1 auto;
  }
  form {
    display: flex;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
  }
  .stripes {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    cursor: pointer;
  }
</style>
