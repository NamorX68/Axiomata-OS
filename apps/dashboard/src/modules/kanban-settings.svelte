<script lang="ts">
  /**
   * kanban — flip side: which board this tile shows, and managing the boards
   * themselves.
   *
   * Board management lives here rather than on the board because you switch
   * boards rarely — that is a setting. Columns are the opposite and are shaped
   * directly on the board: you rearrange them while looking at them. The
   * asymmetry is deliberate; the handling follows the use, not the symmetry.
   *
   * The card treatment is here for the same reason — it is set once and then
   * left alone — but unlike the board, it applies to the whole application
   * rather than to this tile (`kanbanPrefs.ts`).
   */
  import { onMount } from "svelte";

  import { invokeBackend as invoke, type Board } from "../core/backend";
  import { forgetBoard, refreshBoard } from "../core/boardStore";
  import type { ModuleContext } from "../core/types";
  import { cardStyle, rememberLastBoard, setCardStyle, type CardStyle } from "./kanbanPrefs";

  /** The card treatments, in the order they are offered. `auto` first because
   *  it is the default and the right answer unless the eye says otherwise. */
  const CARD_STYLE_CHOICES: { id: CardStyle; label: string; note: string }[] = [
    { id: "auto", label: "Automatisch", note: "das Thema entscheidet" },
    { id: "flat", label: "Flach", note: "nur der Tonwert trennt" },
    { id: "edge", label: "Kante", note: "eine Haarlinie ringsum" },
    { id: "raised", label: "Schwebend", note: "ein leichter Schatten" },
  ];

  let { ctx }: { ctx: ModuleContext } = $props();
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let boards = $state<Board[]>([]);
  let error = $state("");
  let busy = $state(false);
  let newName = $state("");
  /** Board id awaiting a confirmed delete, with its card count. */
  let confirming = $state<{ id: number; cards: number } | null>(null);

  const selected = $derived(typeof $config.boardId === "number" ? $config.boardId : null);

  async function load() {
    try {
      boards = await invoke<Board[]>("list_boards");
      error = "";
      // A tile pointed at a board that no longer exists would render an
      // error forever; fall back to whatever is there.
      if (selected !== null && !boards.some((b) => b.id === selected)) {
        config.update((c) => ({ ...c, boardId: boards[0]?.id ?? undefined }));
      }
    } catch (err) {
      error = String(err);
    }
  }

  onMount(load);

  function choose(id: number) {
    config.update((c) => ({ ...c, boardId: id }));
    // Also the application-wide "last used", so the next tile placed opens
    // this board rather than whichever happens to be first in the list.
    rememberLastBoard(id);
  }

  async function create() {
    const name = newName.trim();
    if (!name || busy) return;
    busy = true;
    try {
      const created = await invoke<Board>("create_board", { name });
      newName = "";
      choose(created.id);
      await load();
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
      if (selected === id) config.update((c) => ({ ...c, boardId: undefined }));
      await load();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="settings">
  <h3>Brett</h3>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if boards.length === 0}
    <p class="hint">Noch kein Brett angelegt.</p>
  {/if}

  <ul>
    {#each boards as board (board.id)}
      <li class:on={selected === board.id}>
        <label>
          <input
            type="radio"
            name="board-{ctx.instanceId}"
            checked={selected === board.id}
            onchange={() => choose(board.id)}
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
  <ul class="styles">
    {#each CARD_STYLE_CHOICES as choice (choice.id)}
      <li>
        <label>
          <input
            type="radio"
            name="card-style-{ctx.instanceId}"
            checked={$cardStyle === choice.id}
            onchange={() => setCardStyle(choice.id)}
          />
          <span class="label">{choice.label}</span>
          <span class="note">{choice.note}</span>
        </label>
      </li>
    {/each}
  </ul>
  <p class="hint">Gilt für alle Bretter.</p>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    overflow-y: auto;
    height: 100%;
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
  ul.styles {
    gap: 0;
  }
  ul.styles label {
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) 0;
    cursor: pointer;
  }
  ul.styles .label {
    flex: 0 0 auto;
  }
  ul.styles .note {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
</style>
