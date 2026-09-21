<script lang="ts">
  /**
   * kanban — a board of columns and cards.
   *
   * One module, three shapes, chosen by the instance config rather than by
   * three registrations: a canvas tile to glance at, the same board as a
   * floating panel to work in (`openStaged` with `path: "board:<id>"`), and a
   * single card's detail (`path: "card:<id>"`). They share one store per
   * board, so a change in one is a change in all of them.
   *
   * Cards are moved by pointer **and** by keyboard, through the same pure
   * targets in `core/kanban.ts` — the keyboard path is not a consolation
   * prize. Columns are shaped here on the board (you rearrange them while
   * looking at them); boards themselves are chosen and managed on the flip
   * side (you switch boards rarely, which is a setting).
   *
   * Appearance follows decisions taken by looking at a preview view in every
   * theme, since deleted — `docs/plans/kanban.md` §6a records *why* each came
   * out this way: the column body is `--ax-bg`, the desk the cards lie on, and
   * the card itself asks the theme how it should sit via `--ax-card-*`. Only
   * labels carry colour. Nothing here is framed — the tile's front face is
   * frameless, and a column drawn with a border would fight that.
   */
  import { SvelteSet } from "svelte/reactivity";

  import { draggable, type DragDelta, type DragPoint } from "../canvas/drag";
  import type { BoardCard, BoardColumn, CardFields } from "../core/backend";
  import { invokeBackend as invoke } from "../core/backend";
  import { boardStore, refreshBoard } from "../core/boardStore";
  import {
    actorLabel,
    applyFilter,
    collectLabels,
    dropTarget,
    dueState,
    groupByColumn,
    labelColorIndex,
    showsAssignee,
    stepTarget,
    type CardFilter,
    type ColumnGeometry,
    type DropTarget,
  } from "../core/kanban";
  import { closeStaged, hostAnchor, openStaged, staged } from "../core/staging";
  import type { ModuleContext } from "../core/types";
  import { cardStyle, lastBoard, rememberLastBoard } from "./kanbanPrefs";

  let { ctx }: { ctx: ModuleContext } = $props();
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  /** Label colours live as tokens so each theme can tune them. */
  const LABEL_TOKENS = 6;
  /** D1: columns squeeze to here, then the board scrolls. Never hidden. */
  const MIN_COL_PX = 140;

  const cardId = $derived(typeof $config.cardId === "number" ? $config.cardId : null);
  /** This panel's own staging id, so a deleted card can close its own panel. */
  const stagedId = $derived(
    $staged.find((panel) => panel.config.cardId === cardId)?.id ?? null,
  );
  /** True when this instance is a floating panel rather than a canvas tile. */
  const isPanel = $derived($config.path !== undefined);

  let boardId = $state<number | null>(null);
  /** Every board, for the switcher in the header. */
  let boards = $state<{ id: number; name: string }[]>([]);
  let listError = $state("");
  let filterText = $state("");
  let activeLabels = $state<string[]>([]);
  let showArchived = $state(false);

  // A board id in the config wins; otherwise the first board there is. A board
  // is only created explicitly (flip side), never implicitly by looking.
  //
  // This *follows* the config rather than reading it once at mount: the flip
  // side writes a new board id there when you create or switch, and a front
  // face that only looked once kept showing the previous board's cards under
  // the new board's name.
  const configuredBoard = $derived(
    typeof $config.boardId === "number" ? $config.boardId : null,
  );

  $effect(() => {
    if (configuredBoard !== null) {
      boardId = configuredBoard;
      return;
    }
    if (boardId !== null) return;
    void invoke<{ id: number; name: string }[]>("list_boards")
      .then((all) => {
        const remembered = lastBoard();
        const chosen =
          (remembered !== undefined && all.some((b) => b.id === remembered)
            ? remembered
            : all[0]?.id) ?? null;
        boardId = chosen;
        // Recorded on the instance too, so this tile keeps showing this board
        // even after somebody else switches the application-wide default.
        if (chosen !== null) config.update((c) => ({ ...c, boardId: chosen }));
      })
      .catch((err) => (listError = String(err)));
  });

  /** The switcher needs every board's name, not just this tile's. Re-read
   *  whenever the chosen board changes, so a board created on the flip side
   *  appears in the switcher without a reload. */
  $effect(() => {
    void boardId;
    void invoke<{ id: number; name: string }[]>("list_boards")
      .then((all) => (boards = all))
      .catch((err) => (listError = String(err)));
  });

  function switchBoard(next: number) {
    boardId = next;
    config.update((c) => ({ ...c, boardId: next }));
    rememberLastBoard(next);
  }

  /** Opens this board as a large floating panel — the place to actually work
   *  in it, as opposed to the tile, which is for glancing at. */
  function openAsPanel() {
    if (boardId === null) return;
    openStaged("kanban", {
      path: `board:${boardId}`,
      boardId,
      // Deliberately no anchor, unlike the card detail. A card is small and
      // belongs where the eye already is; a full-size board is not a reply to
      // the tile you clicked, it is a place to go. Anchored, a tile parked in
      // a corner opened its big view in that corner — the one spot where the
      // panel has least room to be big in.
      //
      // Its own key, so the big board and a single card do not share one
      // remembered size — they are the same module but not the same window.
      sizeKey: "kanban-board",
      // This is *the* place to work in a board, as opposed to the tile, which
      // is for glancing at. Full HD is the floor for that: at 1024 it was no
      // roomier than the tile it was opened from, which made the button
      // pointless. `StagingPanel` clamps this to the window, so a smaller
      // screen simply gets as much of it as it has.
      panelSize: { w: 1920, h: 1080 },
    });
  }

  const data = $derived(boardId === null ? null : boardStore(boardId));

  const filter = $derived<CardFilter>({
    text: filterText,
    labels: activeLabels,
    includeArchived: showArchived,
  });

  const visible = $derived($data ? applyFilter($data.cards, filter) : []);
  const grouped = $derived($data ? groupByColumn($data.columns, visible) : []);
  const allLabels = $derived($data ? collectLabels($data.cards) : []);
  const boardEmpty = $derived($data !== null && $data.cards.length === 0);
  const detail = $derived<BoardCard | null>(
    cardId === null ? null : ($data?.cards.find((card) => card.id === cardId) ?? null),
  );

  function openCard(card: BoardCard, event: MouseEvent) {
    // A drag ends with a click on the card it just moved. Without this, every
    // successful drag would also fling the detail panel open.
    if (justDragged) {
      justDragged = false;
      return;
    }
    // `openStaged` recognises an already-open panel only by `config.path`
    // (core/staging.ts), so the card id goes in as one — without it every
    // click would stack another copy of the same card.
    //
    // The anchor makes the detail appear over the tile the card was clicked
    // in, rather than in the middle of the screen: the eye is already there.
    openStaged("kanban", {
      path: `card:${card.id}`,
      cardId: card.id,
      boardId: card.board_id,
      anchor: hostAnchor(event.currentTarget as Element),
      // A card is a short form, not a board. Without its own key it would
      // inherit the board panel's size, which is far too big for five fields.
      sizeKey: "kanban-card",
      // Five lines more body than the fields strictly need — a card is
      // usually a sentence, but the ones that are not deserve room.
      panelSize: { w: 520, h: 540 },
    });
  }

  function toggleLabel(label: string) {
    activeLabels = activeLabels.includes(label)
      ? activeLabels.filter((l) => l !== label)
      : [...activeLabels, label];
  }

  /* ------------------------------------------------------------ moving --- */

  let rootEl = $state<HTMLElement | null>(null);
  let boardEl = $state<HTMLElement | null>(null);
  let dragging = $state<{ id: number; x: number; y: number; width: number } | null>(null);
  /** Pointer offset inside the card when the drag began, so the ghost keeps
   *  the grip the user actually took instead of jumping to its own corner. */
  let grab = { dx: 0, dy: 0 };
  let target = $state<DropTarget | null>(null);
  /** Set while moving by keyboard; the same target type the pointer produces. */
  let carrying = $state<{ id: number; title: string } | null>(null);
  let announcement = $state("");
  /** A drag ends with a click on the card; this swallows it once. */
  let justDragged = false;
  /** The card whose move is in flight — hidden at its old place until the
   *  board comes back with it in its new one. */
  /**
   * Cards whose move is in flight, hidden at their old position until the
   * board comes back changed.
   *
   * A set rather than one id: two cards can be dragged in quick succession,
   * and with a single value the first move's round trip finishing would
   * un-hide the second one mid-flight — bringing back exactly the flash this
   * exists to prevent, for precisely the case that is hardest to notice
   * while testing.
   */
  const settling = new SvelteSet<number>();
  /** A column being dragged by its header, and where it would land. */
  let draggingColumn = $state<{ id: number; x: number } | null>(null);
  let columnTarget = $state<number | null>(null);
  let columnBoxes: { id: number; x: number; w: number }[] = [];
  let snapshot: ColumnGeometry[] = [];

  /**
   * Measures every column and card once, at the moment a move starts.
   *
   * Never per pointer move: the dragged card is lifted out of the flow, so
   * re-measuring would chase boxes that shift *because* of the drag, and the
   * drop indicator would flicker between two answers.
   */
  function measure(): ColumnGeometry[] {
    if (!boardEl) return [];
    return [...boardEl.querySelectorAll<HTMLElement>("[data-column]")].map((columnEl) => ({
      columnId: Number(columnEl.dataset.column),
      rect: box(columnEl),
      cards: [...columnEl.querySelectorAll<HTMLElement>("[data-card]")].map((cardEl) => ({
        id: Number(cardEl.dataset.card),
        rect: box(cardEl),
      })),
    }));
  }

  /**
   * Does the drop marker belong *after* the last card of this column?
   *
   * The card being moved is excluded from the count: while it is in the air it
   * is still in `cards`, so counting it would put "the end" one place beyond
   * where the user can actually drop.
   */
  function marksEndOf(columnId: number, cards: BoardCard[]): boolean {
    if (!dragging && !carrying) return false;
    if (target?.columnId !== columnId) return false;
    const held = dragging?.id ?? carrying?.id;
    return target.index >= cards.filter((c) => c.id !== held).length;
  }

  function box(el: HTMLElement) {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  }

  async function commitMove(id: number, to: DropTarget) {
    // The card stays out of its old place until the board comes back changed.
    // Without this it snaps back to full opacity at the position it just left
    // and sits there for the length of a round-trip before jumping — a flash
    // that reads as "the move failed" even when it succeeded.
    settling.add(id);
    try {
      await invoke("move_card", { id, columnId: to.columnId, index: to.index });
    } catch (err) {
      listError = String(err);
    } finally {
      // Whether it worked or not, what is on screen has to match the database
      // again before the card is shown anywhere.
      if (boardId !== null) await refreshBoard(boardId);
      settling.delete(id);
    }
  }

  function startDrag(card: BoardCard, point: DragPoint) {
    snapshot = measure();
    const rect = snapshot.flatMap((c) => c.cards).find((c) => c.id === card.id)?.rect;
    grab = rect ? { dx: point.x - rect.x, dy: point.y - rect.y } : { dx: 0, dy: 0 };
    // The ghost takes the card's own width, so it is the card that was picked
    // up rather than a differently-shaped stand-in for it.
    dragging = { id: card.id, width: rect?.w ?? 0, ...ghostAt(point) };
    target = null;
  }

  /** Ghost position, relative to the module root it is rendered in. */
  function ghostAt(point: DragPoint) {
    const origin = rootEl?.getBoundingClientRect();
    return {
      x: point.x - grab.dx - (origin?.left ?? 0),
      y: point.y - grab.dy - (origin?.top ?? 0),
    };
  }

  function moveDrag(card: BoardCard, _delta: DragDelta, point: DragPoint) {
    dragging = { ...dragging!, ...ghostAt(point) };
    target = dropTarget(snapshot, point.x, point.y, card.id);
  }

  function endDrag(card: BoardCard) {
    const to = target;
    dragging = null;
    target = null;
    justDragged = true;
    // Released outside every column: a cancel, not a drop into whichever
    // column happened to be nearest.
    if (to) void commitMove(card.id, to);
  }

  function startColumnDrag(column: BoardColumn, point: DragPoint) {
    columnBoxes = (boardEl
      ? [...boardEl.querySelectorAll<HTMLElement>("[data-column]")]
      : []
    ).map((el) => {
      const r = el.getBoundingClientRect();
      return { id: Number(el.dataset.column), x: r.left, w: r.width };
    });
    draggingColumn = { id: column.id, x: point.x };
    columnTarget = null;
  }

  function moveColumnDrag(column: BoardColumn, point: DragPoint) {
    draggingColumn = { id: column.id, x: point.x };
    // Same rule as for cards: the index counts the columns it will sit among,
    // so the dragged one is out of its own list.
    const others = columnBoxes.filter((box) => box.id !== column.id);
    const at = others.findIndex((box) => point.x < box.x + box.w / 2);
    columnTarget = at === -1 ? others.length : at;
  }

  async function endColumnDrag(column: BoardColumn) {
    const index = columnTarget;
    draggingColumn = null;
    columnTarget = null;
    if (index === null) return;
    try {
      await invoke("move_board_column", { id: column.id, index });
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  /* Keyboard: the same targets, reached with the keys. Equal standing, not a
     consolation prize — the mouse-wheel bug in tiles is still unsolved, and
     a board that can only be rearranged by pointer would be a trap. */

  function pickUp(card: BoardCard, column: number, index: number) {
    snapshot = measure();
    carrying = { id: card.id, title: card.title };
    target = { columnId: column, index };
    announce(card.title);
  }

  function announce(title: string) {
    const where = target
      ? ` — ${$data?.columns.find((c) => c.id === target!.columnId)?.name ?? ""}, Platz ${target.index + 1}`
      : "";
    announcement = `${title} aufgenommen${where}. Pfeiltasten bewegen, Leertaste legt ab, Escape bricht ab.`;
  }

  function onCardKey(event: KeyboardEvent, card: BoardCard, column: number, index: number) {
    const keys: Record<string, "up" | "down" | "left" | "right"> = {
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
    };

    if (event.key === " " || event.key === "Enter") {
      if (!carrying) {
        if (event.key === "Enter") return; // Enter still opens the card.
        event.preventDefault();
        pickUp(card, column, index);
        return;
      }
      event.preventDefault();
      const to = target;
      const held = carrying;
      carrying = null;
      target = null;
      announcement = `${held.title} abgelegt.`;
      if (to) void commitMove(held.id, to);
      return;
    }

    if (event.key === "Escape" && carrying) {
      event.preventDefault();
      announcement = `${carrying.title} bleibt, wo sie war.`;
      carrying = null;
      target = null;
      return;
    }

    const direction = keys[event.key];
    if (direction && carrying && target) {
      event.preventDefault();
      target = stepTarget(snapshot, target, direction, carrying.id);
      announce(carrying.title);
    }
  }

  /* ------------------------------------------------------------ writing --- */

  let addingTo = $state<number | null>(null);
  let newTitle = $state("");

  async function quickAdd(columnId: number) {
    const title = newTitle.trim();
    if (!title) return;
    newTitle = "";
    try {
      await invoke("create_card", {
        new: { column_id: columnId, title, body: "", labels: [], assignee: null, due_at: null },
      });
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  /** Two steps, because unlike archiving this one does not come back. */
  let confirmingDelete = $state(false);

  async function removeCard(card: BoardCard) {
    try {
      await invoke("delete_card", { id: card.id });
      confirmingDelete = false;
      if (boardId !== null) await refreshBoard(boardId);
      // The panel is showing a card that no longer exists; close it.
      if (stagedId) closeStaged(stagedId);
    } catch (err) {
      listError = String(err);
    }
  }

  async function setArchived(card: BoardCard, archived: boolean) {
    try {
      await invoke("set_card_archived", { id: card.id, archived });
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  /* Columns are shaped here, on the board, and not on the flip side where the
     boards live: you switch boards rarely, which is a setting, but you shape
     columns while looking at them. */

  let removingColumn = $state<{ id: number; held: number } | null>(null);

  async function saveColumn(column: BoardColumn, patch: Partial<BoardColumn>) {
    try {
      await invoke("update_board_column", {
        id: column.id,
        name: patch.name ?? column.name,
        mapsToStatus: patch.maps_to_status ?? column.maps_to_status,
      });
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  async function addColumn() {
    if (boardId === null) return;
    try {
      await invoke("create_board_column", {
        boardId,
        new: { name: "Neue Spalte", maps_to_status: "open" },
      });
      await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  /** Removing a column asks where its cards go rather than taking them with it. */
  async function removeColumn(column: BoardColumn, moveTo: number | null) {
    try {
      const gone = await invoke<boolean>("delete_board_column", {
        id: column.id,
        moveCardsTo: moveTo,
      });
      if (!gone) {
        listError = "Die Spalte hält noch Karten — wähle, wohin sie sollen.";
        return;
      }
      removingColumn = null;
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      listError = String(err);
    }
  }

  /** `"rust, design"` ⇄ `["rust", "design"]`. Comma-separated rather than a
   *  chip editor: a label is one word, and a text field is the fastest way to
   *  add, rename and remove several at once. */
  function parseLabels(raw: string): string[] {
    return [...new Set(raw.split(",").map((l) => l.trim()).filter(Boolean))];
  }

  /** An `<input type="date">` value ⇄ the RFC 3339 the backend stores. Empty
   *  clears the date rather than being ignored. */
  function dueFromInput(value: string): string | null {
    if (!value) return null;
    const [year, month, day] = value.split("-").map(Number);
    return new Date(year, month - 1, day, 12).toISOString();
  }

  function dueToInput(iso: string | null): string {
    if (!iso) return "";
    const d = new Date(iso);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  /**
   * Saves one or more of a card's fields.
   *
   * `update_card` is a full replace, so every save has to send the fields it
   * is *not* changing as well — which makes the snapshot it merges into the
   * thing that matters. Taking the card as an argument looked fine and was
   * wrong: tabbing from the labels field to the date field fires two saves in
   * a row, and the second merged into a snapshot taken before the first had
   * come back, silently undoing it. So the current card is read here, at call
   * time, and saves are chained rather than raced.
   */
  let savingDetail: Promise<unknown> = Promise.resolve();

  function saveDetail(fields: Partial<CardFields>) {
    savingDetail = savingDetail.then(async () => {
      const card = detail;
      if (!card) return;
      try {
        await invoke("update_card", {
          id: card.id,
          fields: {
            title: card.title,
            body: card.body,
            labels: card.labels,
            assignee: card.assignee,
            due_at: card.due_at,
            ...fields,
          },
        });
        if (boardId !== null) await refreshBoard(boardId);
      } catch (err) {
        listError = String(err);
      }
    });
    return savingDetail;
  }
</script>

<!-- The card's face, rendered both in its column and on the ghost that
     follows the pointer. One definition, so what you pick up is what you were
     looking at — a ghost showing only the title reads as a tooltip, not as
     the card in your hand. -->
{#snippet cardFace(card: BoardCard)}
  {#if card.body}
    <p class="body">{card.body}</p>
  {/if}
  {#if card.labels.length > 0 || card.due_at || card.verified_by}
    <div class="foot">
      <span class="dots">
        {#each card.labels as label (label)}
          <span class="dot" data-tone={labelColorIndex(label, LABEL_TOKENS)} title={label}></span>
        {/each}
      </span>
      <span class="chips">
        {#each card.labels as label (label)}
          <span class="chip" data-tone={labelColorIndex(label, LABEL_TOKENS)}>{label}</span>
        {/each}
      </span>
      {#if card.verified_by}
        <span class="verified" title="abgenommen von {actorLabel(card.verified_by)}">✓</span>
      {/if}
      {#if card.due_at}
        {@const due = dueState(card.due_at)}
        <span class="due" class:over={due.overdue}>{due.label}</span>
      {/if}
    </div>
  {/if}
  {#if showsAssignee(card.assignee)}
    <p class="who">{actorLabel(card.assignee)}</p>
  {/if}
{/snippet}

{#if listError}
  <p class="notice danger">{listError}</p>
{:else if boardId === null}
  <p class="notice">Noch kein Brett. Leg auf der Rückseite eines an.</p>
{:else if $data === null || ($data.loading && $data.board === null)}
  <p class="notice">Lädt …</p>
{:else if $data.error}
  <p class="notice danger">{$data.error}</p>
{:else if cardId !== null}
  <!-- Detail of a single card. -->
  {#if detail === null}
    <p class="notice">Diese Karte gibt es nicht mehr.</p>
  {:else}
    <article class="detail">
      <!-- Edited in place rather than behind an edit mode: there is no reading
           state worth protecting on a card, and a mode would make the common
           act (fix a typo) cost two extra clicks. Saved on blur, so nothing
           needs confirming either. -->
      <input
        class="detail-title"
        value={detail.title}
        aria-label="Titel"
        onblur={(event) => saveDetail({ title: event.currentTarget.value })}
      />
      {#if detail.labels.length > 0}
        <div class="chips">
          {#each detail.labels as label (label)}
            <span class="chip" data-tone={labelColorIndex(label, LABEL_TOKENS)}>{label}</span>
          {/each}
        </div>
      {/if}
      <textarea
        class="detail-body"
        value={detail.body}
        placeholder="Kein Text."
        aria-label="Text"
        onblur={(event) => saveDetail({ body: event.currentTarget.value })}
      ></textarea>
      <div class="fields">
        <label>
          <span>Labels</span>
          <input
            value={detail.labels.join(", ")}
            placeholder="rust, design"
            onblur={(event) => saveDetail({ labels: parseLabels(event.currentTarget.value) })}
          />
        </label>
        <label>
          <span>Fällig</span>
          <input
            type="date"
            value={dueToInput(detail.due_at)}
            onchange={(event) => saveDetail({ due_at: dueFromInput(event.currentTarget.value) })}
          />
        </label>
        <label>
          <span>Zuständig</span>
          <input
            value={detail.assignee ?? ""}
            placeholder="human:owner oder agent:name"
            onblur={(event) =>
              saveDetail({ assignee: event.currentTarget.value.trim() || null })}
          />
        </label>
      </div>

      <dl>
        {#if detail.due_at}
          {@const due = dueState(detail.due_at)}
          <dt>Fällig in</dt>
          <dd class:over={due.overdue}>{due.label}</dd>
        {/if}
        {#if detail.claimed_by}
          <dt>Übernommen</dt>
          <dd>{actorLabel(detail.claimed_by)}</dd>
        {/if}
        {#if detail.verified_by}
          <dt>Abgenommen</dt>
          <dd>✓ {actorLabel(detail.verified_by)}</dd>
        {/if}
        {#if detail.archived_at}
          <dt>Archiviert</dt>
          <dd>ja</dd>
        {/if}
      </dl>

      <!-- Archiving, not deleting, is how a finished card leaves the board:
           it stays findable behind the archive filter. Deleting is for cards
           that should never have existed and lives in CP-K2b's card menu. -->
      <div class="detail-actions">
        <button onclick={() => setArchived(detail, detail.archived_at === null)}>
          {detail.archived_at === null ? "Archivieren" : "Zurückholen"}
        </button>
        {#if confirmingDelete}
          <button class="danger" onclick={() => removeCard(detail)}>Wirklich löschen</button>
          <button onclick={() => (confirmingDelete = false)}>Abbrechen</button>
        {:else}
          <button onclick={() => (confirmingDelete = true)}>Löschen</button>
        {/if}
      </div>
    </article>
  {/if}
{:else}
  <div class="kanban" data-cards={$cardStyle} bind:this={rootEl}>
    <!-- The board says which board it is, and offers the two things you
         otherwise had to flip the tile to find. Boards are *managed* on the
         flip side (create, rename, delete — settings); switching between them
         belongs here, where you can see which one you are looking at. -->
    <div class="board-head">
      {#if boards.length > 1}
        <select
          class="board-pick"
          aria-label="Brett wählen"
          value={boardId}
          onchange={(event) => switchBoard(Number(event.currentTarget.value))}
        >
          {#each boards as entry (entry.id)}
            <option value={entry.id}>{entry.name}</option>
          {/each}
        </select>
      {:else}
        <span class="board-name">{$data.board?.name ?? ""}</span>
      {/if}
      <button class="head-action add-col" onclick={addColumn}>+ Spalte</button>
      {#if !isPanel}
        <button class="head-action" onclick={openAsPanel} title="Groß öffnen" aria-label="Brett groß öffnen">
          ⤢
        </button>
      {/if}
    </div>

    {#if isPanel}
      <!-- Filters live in the panel only: the tile is for glancing at. -->
      <div class="filters">
        <input
          type="search"
          placeholder="Karten filtern …"
          bind:value={filterText}
          aria-label="Karten filtern"
        />
        {#each allLabels as label (label)}
          <button
            class="chip"
            data-tone={labelColorIndex(label, LABEL_TOKENS)}
            class:on={activeLabels.includes(label)}
            onclick={() => toggleLabel(label)}
          >
            {label}
          </button>
        {/each}
        <label class="archived">
          <input type="checkbox" bind:checked={showArchived} />
          Archiv
        </label>
      </div>
    {/if}

    <div class="board" style="--min-col: {MIN_COL_PX}px" bind:this={boardEl}>
      {#each grouped as { column, cards } (column.id)}
        <section class="col" data-column={column.id} class:col-lifted={draggingColumn?.id === column.id}>
          <header
            use:draggable={{
              handle: ".col-grip",
              onStart: (point) => startColumnDrag(column, point),
              onMove: (_delta, point) => moveColumnDrag(column, point),
              onEnd: () => endColumnDrag(column),
            }}
          >
            <!-- A grip, because the header is almost entirely the name field
                 and form controls never start a drag — without it there is
                 nothing to take hold of. Same idea as the tile's own grip. -->
            <span class="col-grip" aria-hidden="true">⠿</span>
            <input
              class="name"
              value={column.name}
              data-no-drag
              aria-label="Name der Spalte"
              onblur={(event) => saveColumn(column, { name: event.currentTarget.value })}
            />
            <span class="count">{cards.length}</span>
            <!-- D4: the controls stay out of sight until the pointer or the
                 keyboard comes near. A menu on every column head is noise. -->
            <span class="col-tools">
              <select
                data-no-drag
                aria-label="Status der Spalte"
                value={column.maps_to_status}
                onchange={(event) =>
                  saveColumn(column, {
                    maps_to_status: event.currentTarget.value as BoardColumn["maps_to_status"],
                  })}
              >
                <option value="open">offen</option>
                <option value="doing">in Arbeit</option>
                <option value="done">fertig</option>
              </select>
              <button
                data-no-drag
                aria-label="Spalte entfernen"
                onclick={() => (removingColumn = { id: column.id, held: cards.length })}
              >
                ✕
              </button>
            </span>
          </header>

          {#if removingColumn?.id === column.id}
            <div class="col-remove">
              {#if removingColumn.held === 0}
                <span>Spalte entfernen?</span>
                <button onclick={() => removeColumn(column, null)}>Entfernen</button>
              {:else}
                <span>{removingColumn.held} Karten wohin?</span>
                <select
                  aria-label="Zielspalte"
                  onchange={(event) => removeColumn(column, Number(event.currentTarget.value))}
                >
                  <option value="">wählen …</option>
                  {#each grouped.filter((g) => g.column.id !== column.id) as other (other.column.id)}
                    <option value={other.column.id}>{other.column.name}</option>
                  {/each}
                </select>
              {/if}
              <button onclick={() => (removingColumn = null)}>Abbrechen</button>
            </div>
          {/if}
          <div class="cards">
            {#each cards as card, index (card.id)}
              {#if target?.columnId === column.id && target.index === index && (dragging || carrying)}
                <div class="marker" aria-hidden="true"></div>
              {/if}
              <article
                class="card"
                class:archived={card.archived_at !== null}
                class:lifted={dragging?.id === card.id || carrying?.id === card.id}
                class:settling={settling.has(card.id)}
                data-card={card.id}
                use:draggable={{
                  ignore: "[data-no-drag]",
                  onStart: (point) => startDrag(card, point),
                  onMove: (delta, point) => moveDrag(card, delta, point),
                  onEnd: () => endDrag(card),
                }}
              >
                <button
                  class="open"
                  onclick={(event) => openCard(card, event)}
                  onkeydown={(event) => onCardKey(event, card, column.id, index)}
                >
                  <span class="title">{card.title}</span>
                </button>
                {@render cardFace(card)}
              </article>
            {:else}
              <p class="empty">Keine Karten</p>
            {/each}
            {#if marksEndOf(column.id, cards)}
              <div class="marker" aria-hidden="true"></div>
            {/if}
          </div>

          {#if addingTo === column.id}
            <form
              class="quick"
              onsubmit={(event) => {
                event.preventDefault();
                void quickAdd(column.id);
              }}
            >
              <!-- svelte-ignore a11y_autofocus -->
              <input
                autofocus
                data-no-drag
                class="quick-field"
                placeholder="Titel, Enter"
                bind:value={newTitle}
                aria-label="Titel der neuen Karte"
                onblur={() => (addingTo = newTitle.trim() ? addingTo : null)}
                onkeydown={(event) => {
                  if (event.key === "Escape") {
                    newTitle = "";
                    addingTo = null;
                  }
                }}
              />
            </form>
          {:else}
            <button class="add" data-no-drag onclick={() => ((addingTo = column.id), (newTitle = ""))}>
              + Karte
            </button>
          {/if}
        </section>
      {/each}

    </div>

    {#if dragging}
      {@const held = dragging}
      {@const ghostCard = $data.cards.find((c) => c.id === held.id)}
      {#if ghostCard}
        <div
          class="ghost card"
          style="left: {held.x}px; top: {held.y}px; width: {held.width}px"
          aria-hidden="true"
        >
          <span class="title">{ghostCard.title}</span>
          {@render cardFace(ghostCard)}
        </div>
      {/if}
    {/if}

    <!-- What a keyboard move is doing, for anyone not watching the screen. -->
    <p class="sr-only" aria-live="polite">{announcement}</p>

    {#if boardEmpty}
      <p class="notice">Noch nichts hier — leg unten in einer Spalte die erste Karte an.</p>
    {/if}
  </div>
{/if}

<style>
  /* How a card sits on its column, when the user overrides the theme's own
     answer (`modules/kanbanPrefs.ts`). Only the three tokens the card face
     reads are touched, so the override reaches the drag ghost and the card
     detail for free — they are the same face. `auto` sets nothing at all and
     leaves the theme's values standing. */
  .kanban[data-cards="flat"] {
    --ax-card-border: transparent;
    --ax-card-shadow: none;
  }
  .kanban[data-cards="edge"] {
    --ax-card-border: var(--ax-border);
    --ax-card-shadow: none;
  }
  .kanban[data-cards="raised"] {
    --ax-card-border: transparent;
    --ax-card-shadow: var(--ax-card-shadow-raised);
  }

  .kanban {
    /* The drag ghost is positioned against this box. */
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
    font-family: var(--ax-font-sans);
    color: var(--ax-text);
  }

  .notice {
    margin: 0;
    padding: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    text-align: center;
  }
  .notice.danger {
    color: var(--ax-danger);
    text-align: left;
  }

  .board-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-height: 22px;
  }
  .board-name {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .board-head .board-pick {
    max-width: 60%;
    padding: 1px var(--ax-space-1);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }
  .board-head .board-pick:hover {
    border-color: var(--ax-border);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .filters input[type="search"] {
    flex: 1 1 160px;
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .archived {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* D1: squeeze to --min-col, then scroll sideways. A board that hides a
     column lies about the state of the work. */
  .board {
    display: flex;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 0;
    min-width: var(--min-col);
    /* D3: the desk the cards lie on — a tint, never a frame. */
    background: var(--ax-bg);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
    container: col / inline-size;
  }

  header {
    /* The column tools are absolutely positioned against this. Without it
       they fall through to `.kanban` (positioned for the drag ghost) and land
       in the corner of the whole module — outside the column, so hovering
       them ends the `.col:hover` that revealed them and they vanish as you
       reach for them. */
    position: relative;
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-1) var(--ax-space-2);
  }
  /* Quiet until touched: the header should read as a heading, not as a form.
     Selectors are deliberately more specific than a bare class — styles.css's
     global `input:not([type=checkbox]):not([type=radio])` rule outranks a
     plain scoped class, so a quiet field needs to outrank it back. */
  .col header .name {
    flex: 1 1 auto;
    min-width: 0;
    padding: 1px var(--ax-space-1);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    text-overflow: ellipsis;
  }
  .col header .name:hover {
    border-color: var(--ax-border);
  }
  .col header .name:focus {
    border-color: var(--ax-accent);
    background: var(--ax-surface-2);
    outline: none;
  }

  /* Taken out of the flow: laid out in it, these would keep their width even
     while invisible and squeeze the column name down to nothing on a narrow
     column. They overlay the name's tail only while actually shown. */
  .col-tools {
    position: absolute;
    right: 0;
    top: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding-left: var(--ax-space-2);
    background: var(--ax-bg);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .col:hover .col-tools,
  .col-tools:focus-within {
    opacity: 1;
    pointer-events: auto;
  }
  .col-tools select,
  .col-remove select {
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
  }
  .col-tools button {
    border: 0;
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    cursor: pointer;
  }
  .col-tools button:hover {
    color: var(--ax-danger);
  }

  .col-remove {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
    padding: var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .col-remove button {
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  /* Board-wide actions live in the board header, not in the column row: a
     button in that row costs the columns width, and at the default tile size
     that is the difference between labels as chips and labels as bare dots. */
  .head-action {
    flex: 0 0 auto;
    padding: 1px var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    cursor: pointer;
  }
  .head-action:hover {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }
  .add-col {
    margin-left: auto;
  }
  .count {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-family: var(--ax-font-mono);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  /* The theme decides how a card sits on its surface (CP-K2-Design). */
  .card {
    background: var(--ax-card-bg);
    border: 1px solid var(--ax-card-border);
    box-shadow: var(--ax-card-shadow);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2);
  }
  .card.archived {
    opacity: 0.55;
  }
  /* The card being moved, by pointer or by key. It keeps its place in the flow
     so the column does not reshuffle under the drop indicator — it is only
     drawn as lifted. */
  .card.lifted {
    opacity: 0.45;
    position: relative;
    z-index: 1;
    cursor: grabbing;
  }

  /* A 2px line at the insertion point, never a placeholder gap: a gap would
     re-lay out the column and invalidate the snapshot the drop is computed
     against. */
  .marker {
    height: 2px;
    margin: calc(-1 * var(--ax-space-1)) 0;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
  }

  .add {
    margin-top: var(--ax-space-2);
    padding: var(--ax-space-1);
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    text-align: left;
    cursor: pointer;
  }
  .add:hover {
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }

  .col-lifted {
    opacity: 0.5;
  }
  .col-grip {
    flex: 0 0 auto;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    opacity: 0.35;
    cursor: grab;
    user-select: none;
  }
  .col:hover .col-grip {
    opacity: 0.8;
  }
  .col-lifted .col-grip {
    cursor: grabbing;
  }

  /* Gone from its old column while the move is in flight, rather than dimmed:
     the gap closing is what "the card left" looks like. */
  .card.settling {
    display: none;
  }

  /* The card following the pointer. It lives at the module root, not inside a
     column: columns scroll, and a transformed card inside one gets clipped at
     the column edge the moment it is dragged anywhere useful. */
  .ghost {
    position: absolute;
    pointer-events: none;
    z-index: 2;
    box-shadow: var(--ax-shadow-drag);
    opacity: 0.95;
    rotate: -1.5deg;
  }

  .quick input.quick-field {
    width: 100%;
    margin-top: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-accent);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }

  /* Visible to a screen reader, not on screen — the running commentary for a
     keyboard move. */
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .open {
    display: block;
    width: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .open:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 2px;
    border-radius: var(--ax-radius-sm);
  }
  /* One step up the scale rather than a literal 13px: the scale runs
     11/12/14/16/20, and wedging a 13 between 12 and 14 would give four steps
     inside three pixels, which stops being a scale. The meta row below stays
     small on purpose — the gap is what makes the title read as the title. */
  .title {
    font-size: var(--ax-font-size-base);
    line-height: var(--ax-line-height);
  }
  .card .body {
    margin: var(--ax-space-1) 0 0;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    line-height: var(--ax-line-height);
  }

  .foot {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--ax-space-1) var(--ax-space-2);
    margin-top: var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
  }
  .dots {
    display: none;
    gap: 3px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--ax-radius-pill);
    background: var(--tone);
  }
  .chips {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  /* D2: labels are the only thing on the board that carries colour. */
  .chip {
    padding: 0 6px;
    border: 1px solid var(--tone);
    border-radius: var(--ax-radius-pill);
    background: transparent;
    color: var(--tone);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    line-height: 1.6;
  }
  button.chip {
    cursor: pointer;
  }
  button.chip.on {
    background: var(--ax-accent-muted);
  }
  [data-tone="0"] {
    --tone: var(--ax-label-1);
  }
  [data-tone="1"] {
    --tone: var(--ax-label-2);
  }
  [data-tone="2"] {
    --tone: var(--ax-label-3);
  }
  [data-tone="3"] {
    --tone: var(--ax-label-4);
  }
  [data-tone="4"] {
    --tone: var(--ax-label-5);
  }
  [data-tone="5"] {
    --tone: var(--ax-label-6);
  }

  .verified {
    color: var(--ax-success);
  }
  .due {
    margin-left: auto;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    white-space: nowrap;
  }
  .due.over {
    color: var(--ax-danger);
  }
  .who {
    margin: var(--ax-space-1) 0 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    margin: var(--ax-space-2) var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* Density follows the COLUMN's width, not the board's: a wide board with
     eight columns still has narrow columns. */
  @container col (max-width: 200px) {
    .chips,
    .card .body,
    .who {
      display: none;
    }
    .dots {
      display: inline-flex;
    }
  }

  /* ------------------------------------------------------------ detail --- */

  /* A column, so the body field takes whatever height is left instead of
     standing at a fixed size with dead space under it. Shrinking the panel
     shrinks the field; the metadata and the buttons keep their place. */
  .detail {
    display: flex;
    flex-direction: column;
    padding: var(--ax-space-4);
    font-family: var(--ax-font-sans);
    color: var(--ax-text);
    overflow-y: auto;
    height: 100%;
  }
  /* Edited in place: the fields carry no chrome until touched, so the panel
     still reads as a card rather than as a form. */
  .detail .detail-title,
  .detail .detail-body {
    display: block;
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    line-height: var(--ax-line-height);
  }
  .detail .detail-title:hover,
  .detail .detail-body:hover {
    border-color: var(--ax-border);
  }
  .detail .detail-title:focus,
  .detail .detail-body:focus {
    border-color: var(--ax-accent);
    background: var(--ax-surface-2);
    outline: none;
  }
  .detail .detail-title {
    flex: 0 0 auto;
    margin-bottom: var(--ax-space-3);
    font-size: var(--ax-font-size-lg);
  }
  .detail .chips,
  .fields,
  .detail dl,
  .detail-actions {
    flex: 0 0 auto;
  }
  .detail .detail-body {
    flex: 1 1 auto;
    margin-bottom: var(--ax-space-4);
    min-height: 9em;
    resize: none;
    font-size: var(--ax-font-size-sm);
  }
  .detail .chips {
    margin-bottom: var(--ax-space-3);
  }
  .fields {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--ax-space-2) var(--ax-space-3);
    margin-bottom: var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
  }
  .fields label {
    display: contents;
  }
  .fields span {
    color: var(--ax-text-muted);
  }
  .fields input {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }

  .detail-actions {
    display: flex;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-4);
  }
  .detail-actions button {
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: transparent;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }
  .detail-actions button.danger {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }
  .detail-actions button:hover {
    color: var(--ax-text);
    border-color: var(--ax-border-strong);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--ax-space-1) var(--ax-space-3);
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }
  dt {
    color: var(--ax-text-muted);
  }
  dd {
    margin: 0;
  }
  dd.over {
    color: var(--ax-danger);
  }
</style>
