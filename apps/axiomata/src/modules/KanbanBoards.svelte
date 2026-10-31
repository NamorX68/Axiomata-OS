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
  import { messageOf } from "../core/errors";
  import { onMount } from "svelte";

  import { invokeBackend as invoke, type Board } from "../core/backend";
  import { emit } from "../core/bus";
  import { forgetBoard, refreshBoard } from "../core/boardStore";
  import { tFlow } from "../i18n/flow";
  import { tKanban as t } from "../i18n/kanban";
  import { refreshAgents } from "../ide/projectSession";
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

  /** A mark, not a word: the same in every language. */
  const MARK_DELETE = "✕";

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
      error = messageOf(err);
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
      error = messageOf(err);
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
      error = messageOf(err);
    }
  }

  /** Two steps on purpose: deleting a board takes every card on it. */
  async function askDelete(board: Board) {
    try {
      const cards = await invoke<number>("count_board_cards", { boardId: board.id });
      confirming = { id: board.id, cards };
    } catch (err) {
      error = messageOf(err);
    }
  }

  async function confirmDelete(id: number) {
    busy = true;
    try {
      // The board's cards take their sessions with them (Rust); the panes of those sessions in the Studio have
      // nothing left to run, and their tiles must not outlive the cards.
      const ended = await invoke<number[]>("delete_board", { id });
      if (ended.length > 0) emit("studio:close-agent-panes", { agentIds: ended });
      void refreshAgents().catch(() => {});
      forgetBoard(id);
      confirming = null;
      await load();
      onChanged();
      if (current === id) onDeleted();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="boards">
  <h3>{t("boards")}</h3>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if boards.length === 0}
    <p class="hint">{t("noBoardCreated")}</p>
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
            aria-label={t("boardName")}
            onblur={(event) => rename(board, event.currentTarget.value)}
          />
        </label>
        <button class="remove" onclick={() => askDelete(board)} aria-label={t("deleteBoard")}>{MARK_DELETE}</button>
      </li>

      {#if confirming?.id === board.id}
        <li class="confirm">
          <span>
            {confirming.cards === 0 ? t("deleteBoardQuestion") : t("cardsDeletedWith", { n: confirming.cards })}
          </span>
          <button class="danger" disabled={busy} onclick={() => confirmDelete(board.id)}>
            {tFlow("delete")}
          </button>
          <button onclick={() => (confirming = null)}>{tFlow("cancel")}</button>
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
    <input placeholder={t("newBoard")} bind:value={newName} aria-label={t("newBoardName")} />
    <button type="submit" disabled={busy || newName.trim() === ""}>{t("create")}</button>
  </form>

  <h3>{t("cards")}</h3>
  <label class="stripes">
    <input type="checkbox" checked={$cardStripes} onchange={(e) => setCardStripes(e.currentTarget.checked)} />
    <span>{t("colourStripe")}</span>
  </label>
  <p class="hint">{t("allBoards")}</p>
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
