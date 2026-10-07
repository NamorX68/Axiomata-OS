<script lang="ts">
  /**
   * kanban — a board of columns and cards.
   *
   * One module, two shapes, chosen by the panel config rather than by two
   * registrations: the board (`openStaged` with `path: "board:<id>"`, opened
   * from the ring by `kanbanApp.ts` — Kanban is an app and has no tile) and a
   * single card's detail (`path: "card:<id>"`). They share one store per
   * board, so a change in one is a change in all of them.
   *
   * Cards are moved by pointer **and** by keyboard, through the same pure
   * targets in `core/kanban.ts` — the keyboard path is not a consolation
   * prize. Columns are shaped here on the board (you rearrange them while
   * looking at them); boards themselves are chosen in the header and managed
   * in the gear's popover (you switch boards rarely, which is a setting).
   *
   * Appearance follows decisions taken by looking at a preview view in every
   * theme, since deleted — `docs/plans/kanban.md` §6a records *why* each came
   * out this way: the column body is `--ax-bg`, the desk the cards lie on, and
   * the card itself asks the theme how it should sit via `--ax-card-*`. Only
   * labels carry colour. Nothing here is framed — the board is frameless,
   * and a column drawn with a border would fight that.
   */
  import { get } from "svelte/store";
  import { SvelteSet } from "svelte/reactivity";

  import { draggable, type DragDelta, type DragPoint } from "../canvas/drag";
  import { onMount } from "svelte";

  import type { BoardCard, BoardColumn, CardEvent, CardFields, CardTier } from "../core/backend";
  import { invokeBackend as invoke } from "../core/backend";
  import CardReviewForm from "./CardReviewForm.svelte";
  import CardStartForm from "./CardStartForm.svelte";
  import CardTakeOverForm from "./CardTakeOverForm.svelte";
  import CardUsage from "./CardUsage.svelte";
  import { canReview, canStart, canTakeOver } from "../ide/cardStart";
  import { boardStore, refreshBoard } from "../core/boardStore";
  import {
    actorLabel,
    applyFilter,
    collectLabels,
    dependencyCandidates,
    dropTarget,
    dueState,
    EVENT_LABEL,
    fieldsOf,
    groupByColumn,
    hideEmptyProposal,
    isNotableState,
    labelColorIndex,
    planLabel,
    showsAssignee,
    STATE_LABEL,
    stateTone,
    stepTarget,
    type CardFilter,
    type ColumnGeometry,
    type DropTarget,
  } from "../core/kanban";
  import { listRoles } from "../core/roster";
  import { closeStaged, staged } from "../core/staging";
  import type { ModuleContext } from "../core/types";
  import { cardStripes, lastBoard, rememberLastBoard } from "./kanbanPrefs";
  import { boardPathPatch, KANBAN_SHOW_CARD } from "./kanbanApp";
  import { on } from "../core/bus";
  import KanbanBoards from "./KanbanBoards.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";

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
  let boardId = $state<number | null>(null);
  /** Every board, for the switcher in the header. */
  let boards = $state<{ id: number; name: string }[]>([]);
  let listError = $state("");
  let filterText = $state("");
  let activeLabels = $state<string[]>([]);
  let showArchived = $state(false);

  // A board id in the config wins; otherwise the last used one, else the first.
  // A board is only created explicitly (the gear's popover), never implicitly
  // by looking.
  //
  // This *follows* the config rather than reading it once at mount: creating
  // or switching a board writes the new id there, and a view that only looked
  // once kept showing the previous board's cards under the new board's name.
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
        // Recorded on the panel too, so it keeps showing this board even after
        // another panel switches the application-wide default.
        if (chosen !== null) config.update((c) => ({ ...c, boardId: chosen }));
      })
      .catch((err) => (listError = String(err)));
  });

  /** The switcher needs every board's name, not just the shown one's. Re-read whenever the chosen board changes and
   *  whenever the boards popover created, renamed or deleted one, so the switcher never lists a board that is gone. */
  function loadBoards(): void {
    void invoke<{ id: number; name: string }[]>("list_boards")
      .then((all) => (boards = all))
      .catch((err) => (listError = String(err)));
  }
  $effect(() => {
    void boardId;
    loadBoards();
  });

  function switchBoard(next: number) {
    boardId = next;
    // The panel's `path` names the board it shows: that is what makes a second click on the ring entry raise this
    // panel instead of opening another one on the same board (`openStaged` compares `path`).
    config.update((c) => ({ ...c, boardId: next, ...boardPathPatch(c, next) }));
    rememberLastBoard(next);
  }

  /** The board panel's boards popover: picking, creating, renaming and deleting boards. */
  let showBoards = $state(false);

  /** The board shown was deleted: forget it and let the first effect pick another (or none). */
  function boardGone(): void {
    showBoards = false;
    boardId = null;
    config.update((c) => ({ ...c, boardId: undefined, ...boardPathPatch(c, null) }));
    loadBoards();
  }

  function chooseBoard(id: number): void {
    showBoards = false;
    switchBoard(id);
  }

  const data = $derived(boardId === null ? null : boardStore(boardId));

  const filter = $derived<CardFilter>({
    text: filterText,
    labels: activeLabels,
    includeArchived: showArchived,
  });

  const visible = $derived($data ? applyFilter($data.cards, filter) : []);
  const grouped = $derived($data ? hideEmptyProposal(groupByColumn($data.columns, visible), $data.cards) : []);
  const plans = $derived($data?.plans ?? []);
  const allLabels = $derived($data ? collectLabels($data.cards) : []);
  const boardEmpty = $derived($data !== null && $data.cards.length === 0);
  /** The card in the large board's side panel (editor-look B4), if one is open. */
  let sideCardId = $state<number | null>(
    // Opened by the spotlight on a card: shown at once (`kanbanApp.openKanban`).
    typeof get(config).focusCard === "number" ? (get(config).focusCard as number) : null,
  );
  /** The card the detail shows: a stand-alone card panel's, else the side panel's. */
  const shownCardId = $derived(cardId ?? sideCardId);
  const detail = $derived<BoardCard | null>(
    shownCardId === null ? null : ($data?.cards.find((card) => card.id === shownCardId) ?? null),
  );

  function showInSide(id: number | null): void {
    confirmingDelete = false;
    sideCardId = id;
  }

  function openCard(card: BoardCard) {
    // A drag ends with a click on the card it just moved. Without this, every
    // successful drag would also fling the detail panel open.
    if (justDragged) {
      justDragged = false;
      return;
    }
    // B4: the card opens in the side panel, the board staying in view.
    showInSide(sideCardId === card.id ? null : card.id);
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
  /** The column whose "…" menu is open (editor-look B2). */
  let colMenu = $state<number | null>(null);
  type Role = BoardColumn["maps_to_status"];
  const ROLES: Role[] = ["open", "doing", "done"];
  const ROLE_NAMES: Record<Role, string> = { open: "offen", doing: "in Arbeit", done: "fertig" };

  /** The menu's "Umbenennen": the column's own name field, selected. */
  function renameColumn(id: number): void {
    colMenu = null;
    const field = rootEl?.querySelector<HTMLInputElement>(`[data-column="${id}"] input.name`);
    field?.focus();
    field?.select();
  }
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

  /* Columns are shaped here, on the board, and not in the boards popover:
     you switch boards rarely, which is a setting, but you shape columns while
     looking at them. */

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
    // The card is fixed when the edit is made, not when its turn in the queue comes: by then the side
    // panel may show another card (editor-look B4), and the edit would land on that one.
    const card = detail;
    savingDetail = savingDetail.then(async () => {
      if (!card) return;
      try {
        detailError = "";
        await invoke("update_card", { id: card.id, fields: { ...fieldsOf(card), ...fields } });
        if (boardId !== null) await refreshBoard(boardId);
      } catch (err) {
        // A refusal of the agent fields ("use lower-case …") belongs next to the field, not in place of the board.
        detailError = String(err);
      }
    });
    return savingDetail;
  }

  /* --------------------------------------------------------- agent flow --- */

  /** The last refusal of an edit in the card's detail, shown under its fields. */
  let detailError = $state("");
  // A refusal belongs to the card as it was when the owner clicked: once the card has moved on (another card is open, or
  // this one changed state) the refusal is history, and would otherwise stand above a card it says nothing about.
  $effect(() => {
    void detail?.id;
    void detail?.state;
    detailError = "";
  });
  /** Role names for the suggestions of the "Rolle" field (a2a.md: the card names a role, the catalog lives in the Studio). */
  let roleNames = $state<string[]>([]);
  onMount(() => {
    // The spotlight opened this panel again for a card: show it.
    const stopShowCard = on(KANBAN_SHOW_CARD, (detail) => {
      const wanted = (detail as { cardId?: number } | undefined)?.cardId;
      if (typeof wanted === "number") sideCardId = wanted;
    });
    listRoles()
      .then((loaded) => (roleNames = loaded.roles.map((role) => role.name)))
      .catch(() => {
        // Suggestions are a convenience; the field works without them.
      });
    return stopShowCard;
  });

  /** The history of the card in the detail. Re-read when the card changes, which includes every flow step. */
  let events = $state<CardEvent[]>([]);
  $effect(() => {
    const id = detail?.id;
    void detail?.updated_at;
    if (id === undefined) {
      events = [];
      return;
    }
    // A slower answer for the card that was open before must not overwrite the one that is open now.
    let stale = false;
    invoke<CardEvent[]>("list_card_events", { cardId: id, limit: 30 })
      .then((list) => {
        if (!stale) events = list;
      })
      .catch(() => {
        if (!stale) events = [];
      });
    return () => {
      stale = true;
    };
  });

  /**
   * After a save, puts a select back to what the card really holds. A refused value (an unknown plan, a dependency
   * rule) leaves the card unchanged, so nothing re-renders and the select would keep showing the refused choice.
   */
  function settle(select: HTMLSelectElement, cardId: number, read: (card: BoardCard) => string): void {
    void savingDetail.then(() => {
      const current = $data?.cards.find((c) => c.id === cardId);
      if (current) select.value = read(current);
    });
  }
  let note = $state("");
  /** The form of a card step that is open — start, review or take-over (A2A CP-A6) — and the card it belongs to. */
  let stepForm = $state<{ card: number; step: "start" | "review" | "take-over"; state: string } | null>(null);
  // The form belongs to the card it was opened on: looking at another card closes it.
  $effect(() => {
    void cardId;
    stepForm = null;
  });

  async function runFlow(step: Promise<unknown>): Promise<void> {
    try {
      detailError = "";
      await step;
      if (boardId !== null) await refreshBoard(boardId);
    } catch (err) {
      detailError = String(err);
    }
  }

  const addDependency = (card: BoardCard, needs: number) =>
    runFlow(invoke("add_card_dependency", { cardId: card.id, needs }));
  const removeDependency = (card: BoardCard, needs: number) =>
    runFlow(invoke("remove_card_dependency", { cardId: card.id, needs }));
  /** The owner gives a started card back: it waits in Offen again and its session cannot start its server any more. */
  const releaseCard = (card: BoardCard) => runFlow(invoke("release_card", { cardId: card.id }));
  const approveProposal = (card: BoardCard) => runFlow(invoke("approve_card_proposal", { cardId: card.id }));
  const markCard = (card: BoardCard, mark: "cancel" | "reopen") =>
    runFlow(invoke("mark_card", { cardId: card.id, mark, reason: null }));

  async function writeNote(card: BoardCard): Promise<void> {
    const text = note.trim();
    if (!text) return;
    note = "";
    await runFlow(invoke("add_card_note", { cardId: card.id, text }));
    events = await invoke<CardEvent[]>("list_card_events", { cardId: card.id, limit: 30 });
  }

  function titleOf(id: number): string {
    return $data?.cards.find((c) => c.id === id)?.title ?? "gelöscht";
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
        <span class="due" class:over={due.overdue} class:soon={due.soon}>
          <Icon name="calendar" size="sm" />{due.label}
        </span>
      {/if}
    </div>
  {/if}
  {#if showsAssignee(card.assignee)}
    <p class="who">
      <Icon name={card.assignee.startsWith("agent:") ? "bot" : "user"} size="sm" />{actorLabel(card.assignee)}
    </p>
  {/if}
  <!-- The agent flow (a2a.md CP-A2): what the column cannot say — waiting for another card, a question, a failure. -->
  {#if isNotableState(card.state) || card.kind || card.agent}
    <p class="flow-line">
      {#if isNotableState(card.state)}
        <span class="state-badge" data-state-tone={stateTone(card.state)} title={card.input_required ?? undefined}>
          {STATE_LABEL[card.state]}{#if card.state === "blocked"}&nbsp;auf&nbsp;{card.waiting_on.map((id) => `#${id}`).join(", ")}{/if}
        </span>
      {/if}
      {#if card.kind}<span class="flow-tag">{card.kind}</span>{/if}
      {#if card.agent}<span class="flow-tag" title="Rolle">{card.agent}</span>{/if}
      {#if card.returned_count > 0}<span class="flow-tag" title="vom Reviewer zurückgegeben">↩ {card.returned_count}</span>{/if}
    </p>
  {/if}
{/snippet}

<svelte:window
  onfocus={() => {
    // The CLI, the chat and (from M7.5) agents change a board without this window knowing; coming
    // back to the app is the moment to look again. Views of one board share a single load.
    if (boardId !== null) void refreshBoard(boardId);
  }}
  onclick={(event) => {
    if (colMenu !== null && !(event.target as Element | null)?.closest?.(".col-menu, .col-tools")) colMenu = null;
  }}
  onkeydown={(event) => {
    if (event.key !== "Escape") return;
    if (colMenu !== null) colMenu = null;
    else if (sideCardId !== null && !(event.target as Element | null)?.closest?.("input, textarea, select")) {
      showInSide(null);
    }
  }}
/>

<!-- A card's detail: in a stand-alone card panel, and in the large board's side panel (editor-look B4). -->
{#snippet cardDetail(detail: BoardCard)}
    <article class="detail">
    <span class="detail-num">Karte #{detail.id}</span>
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

    <!-- The agent flow (a2a.md CP-A2): what kind of work this is, for which role and how strong, and when it counts as done. -->
    <h4 class="flow-head">Ablauf</h4>
    <div class="fields">
      <label>
        <span>Art</span>
        <input
          value={detail.kind ?? ""}
          placeholder="implement, review, test, doc"
          onblur={(event) => saveDetail({ kind: event.currentTarget.value.trim() || null })}
        />
      </label>
      <label>
        <span>Stufe</span>
        <select
          value={detail.tier ?? ""}
          onchange={(event) => {
            const select = event.currentTarget;
            void saveDetail({ tier: (select.value || null) as CardTier | null });
            settle(select, detail.id, (card) => card.tier ?? "");
          }}
        >
          <option value="">—</option>
          <option value="light">leicht</option>
          <option value="medium">mittel</option>
          <option value="heavy">schwer</option>
        </select>
      </label>
      <label>
        <span>Rolle</span>
        <input
          list="kanban-roles"
          value={detail.agent ?? ""}
          placeholder="z. B. allrounder"
          onblur={(event) => saveDetail({ agent: event.currentTarget.value.trim() || null })}
        />
      </label>
      <label>
        <span>Begründung</span>
        <input
          value={detail.agent_reason ?? ""}
          placeholder="warum diese Rolle"
          onblur={(event) => saveDetail({ agent_reason: event.currentTarget.value.trim() || null })}
        />
      </label>
      <label>
        <span>Plan</span>
        <select
          value={detail.plan_id ?? ""}
          onchange={(event) => {
            const select = event.currentTarget;
            void saveDetail({ plan_id: select.value ? Number(select.value) : null });
            settle(select, detail.id, (card) => String(card.plan_id ?? ""));
          }}
        >
          <option value="">kein Plan</option>
          {#each plans as plan (plan.id)}
            <option value={plan.id}>{planLabel(plan)}</option>
          {/each}
        </select>
      </label>
    </div>
    <datalist id="kanban-roles">
      {#each roleNames as name (name)}<option value={name}></option>{/each}
    </datalist>
    <textarea
      class="detail-body"
      value={detail.acceptance}
      placeholder="Abnahmekriterien — woran der Reviewer erkennt, dass die Karte fertig ist."
      aria-label="Abnahmekriterien"
      onblur={(event) => saveDetail({ acceptance: event.currentTarget.value })}
    ></textarea>

    {#if detail.plan_id !== null}
      {@const candidates = dependencyCandidates(detail, $data?.cards ?? [])}
      <div class="deps">
        <span class="deps-label">Braucht zuerst</span>
        {#each detail.depends_on as needs (needs)}
          <span class="dep-chip" class:waiting={detail.waiting_on.includes(needs)} title={titleOf(needs)}>
            #{needs} {titleOf(needs)}
            <button type="button" aria-label="Abhängigkeit von #{needs} entfernen" onclick={() => removeDependency(detail, needs)}>
              ×
            </button>
          </span>
        {/each}
        {#if candidates.length > 0}
          <select
            class="dep-add"
            aria-label="Abhängigkeit hinzufügen"
            onchange={(event) => {
              const picked = Number(event.currentTarget.value);
              event.currentTarget.value = "";
              if (picked) void addDependency(detail, picked);
            }}
          >
            <option value="">+ Abhängigkeit …</option>
            {#each candidates as other (other.id)}<option value={other.id}>#{other.id} {other.title}</option>{/each}
          </select>
        {:else if detail.depends_on.length === 0}
          <span class="deps-none">keine</span>
        {/if}
      </div>
    {/if}
    {#if detailError}<p class="detail-error" role="alert">{detailError}</p>{/if}

    <dl>
      <dt>Zustand</dt>
      <dd>
        {STATE_LABEL[detail.state]}{#if detail.returned_count > 0} · {detail.returned_count}× zurückgegeben{/if}
      </dd>
      {#if detail.input_required}
        <dt>Rückfrage</dt>
        <dd>{detail.input_required}</dd>
      {/if}
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
    {#if stepForm?.card === detail.id && stepForm.state === detail.state}
      {@const finish = () => {
        stepForm = null;
        if (boardId !== null) void refreshBoard(boardId);
      }}
      {#if stepForm.step === "start"}
        <CardStartForm card={detail} onDone={finish} />
      {:else if stepForm.step === "review"}
        <CardReviewForm card={detail} onDone={finish} />
      {:else}
        <CardTakeOverForm card={detail} onDone={finish} />
      {/if}
    {/if}
    <div class="detail-actions">
      {#if !(stepForm?.card === detail.id && stepForm.state === detail.state)}
        {#if canStart(detail)}
          <button class="ax-btn primary" onclick={() => (stepForm = { card: detail.id, step: "start", state: detail.state })}>Starten …</button>
        {:else if canReview(detail)}
          <button class="ax-btn" title="Der Reviewer startet von selbst; hier mit einer Engine deiner Wahl" onclick={() => (stepForm = { card: detail.id, step: "review", state: detail.state })}>Review starten …</button>
        {:else if canTakeOver(detail)}
          <button class="ax-btn primary" onclick={() => (stepForm = { card: detail.id, step: "take-over", state: detail.state })}>Übernehmen …</button>
        {/if}
      {/if}
      {#if detail.state === "working" && detail.claimed_by}
        <button class="ax-btn" title="Die Karte wartet wieder in Offen; das Geheimnis der Sitzung wird zurückgenommen" onclick={() => releaseCard(detail)}>Freigeben</button>
      {/if}
      {#if detail.state === "proposed"}
        <button class="ax-btn primary" onclick={() => approveProposal(detail)}>Vorschlag annehmen</button>
      {/if}
      {#if detail.state === "failed" || detail.state === "canceled"}
        <button class="ax-btn" onclick={() => markCard(detail, "reopen")}>Wieder öffnen</button>
      {:else if detail.state !== "taken_over" && detail.state !== "integrated" && detail.state !== "verified"}
        <button class="ax-btn" onclick={() => markCard(detail, "cancel")}>Absagen</button>
      {/if}
      <button class="ax-btn" onclick={() => setArchived(detail, detail.archived_at === null)}>
        {detail.archived_at === null ? "Archivieren" : "Zurückholen"}
      </button>
      {#if confirmingDelete}
        <button class="ax-btn danger" onclick={() => removeCard(detail)}>Wirklich löschen</button>
        <button class="ax-btn" onclick={() => (confirmingDelete = false)}>Abbrechen</button>
      {:else}
        <button class="ax-btn danger" onclick={() => (confirmingDelete = true)}>Löschen</button>
      {/if}
    </div>

    <CardUsage card={detail} />

    <section class="history" aria-label="Verlauf">
      <h4 class="flow-head">Verlauf</h4>
      <form
        class="note-form"
        onsubmit={(event) => {
          event.preventDefault();
          void writeNote(detail);
        }}
      >
        <input bind:value={note} placeholder="Notiz schreiben …" aria-label="Notiz zum Verlauf" />
      </form>
      {#if events.length === 0}
        <p class="history-empty">Noch nichts passiert.</p>
      {:else}
        <ul>
          {#each [...events].reverse() as event (event.id)}
            <li>
              <span class="h-kind">{EVENT_LABEL[event.kind]}</span>
              <span class="h-who">{actorLabel(event.actor)}</span>
              <time class="h-when" datetime={event.at}>{new Date(event.at).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" })}</time>
              {#if event.text}<p class="h-text">{event.text}</p>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </article>
{/snippet}

{#if listError}
  <p class="notice danger">{listError}</p>
{:else if boardId === null}
  <!-- The app opens even with no board at all: it has to offer to create the first one. -->
  <div class="first-board">
    <p class="notice">Noch kein Brett.</p>
    <KanbanBoards current={null} onChoose={switchBoard} onDeleted={boardGone} />
  </div>
{:else if $data === null || ($data.loading && $data.board === null)}
  <p class="notice">Lädt …</p>
{:else if $data.error}
  <p class="notice danger">{$data.error}</p>
{:else if cardId !== null}
  <!-- Detail of a single card. -->
  {#if detail === null}
    <p class="notice">Diese Karte gibt es nicht mehr.</p>
  {:else}
    {@render cardDetail(detail)}
  {/if}
{:else}
  <div class="kanban" class:stripes={$cardStripes} bind:this={rootEl}>
    <!-- The board says which board it is. Switching between boards belongs here, where you can see which one you
         are looking at; managing them (create, rename, delete) is the gear's popover. -->
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
      <!-- One toolbar (editor-look B5). -->
      <label class="filter">
          <Icon name="search" size="sm" />
          <input type="search" placeholder="Karten filtern …" bind:value={filterText} aria-label="Karten filtern" />
        </label>
        <span class="label-filters">
          {#each allLabels as label (label)}
            <button
              class="chip"
              data-tone={labelColorIndex(label, LABEL_TOKENS)}
              class:on={activeLabels.includes(label)}
              aria-pressed={activeLabels.includes(label)}
              onclick={() => toggleLabel(label)}
            >
              {label}
            </button>
          {/each}
        </span>
        <IconButton
          icon="archive"
          label={showArchived ? "Archivierte Karten ausblenden" : "Archivierte Karten zeigen"}
          pressed={showArchived}
          onclick={() => (showArchived = !showArchived)}
        />
      <IconButton
        icon="refresh-cw"
        label="Brett neu laden"
        disabled={$data.loading}
        onclick={() => boardId !== null && void refreshBoard(boardId)}
      />
      <IconButton icon="columns-3" label="Spalte hinzufügen" onclick={addColumn} />
      <IconButton
        icon="settings"
        label="Bretter & Ansicht"
        pressed={showBoards}
        onclick={() => (showBoards = !showBoards)}
      />
    </div>
    {#if showBoards}
      <div class="boards-pop" role="dialog" aria-label="Bretter & Ansicht">
        <KanbanBoards current={boardId} onChoose={chooseBoard} onDeleted={boardGone} onChanged={loadBoards} />
      </div>
    {/if}

    <div class="work">
    <div class="board" style="--min-col: calc({MIN_COL_PX}px * var(--ax-ui-scale))" bind:this={boardEl}>
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
                 nothing to take hold of. Same idea as the panel's own grip. -->
            <span class="col-grip" aria-hidden="true"><Icon name="grip-vertical" size="sm" /></span>
            <span class="role" data-role={column.maps_to_status} title={ROLE_NAMES[column.maps_to_status]}></span>
            <input
              class="name"
              value={column.name}
              data-no-drag
              aria-label="Name der Spalte"
              onblur={(event) => saveColumn(column, { name: event.currentTarget.value })}
            />
            <span class="count">{cards.length}</span>
            <!-- D4: the controls stay out of sight until the pointer or the keyboard comes near — now one
                 "…" menu (editor-look B2) instead of a select and a ×. -->
            <span class="col-tools" class:menu-shown={colMenu === column.id} data-no-drag>
              <IconButton
                icon="ellipsis"
                size="sm"
                label="Spalte {column.name}: Aktionen"
                pressed={colMenu === column.id}
                onclick={() => (colMenu = colMenu === column.id ? null : column.id)}
              />
            </span>
            {#if colMenu === column.id}
              <div class="col-menu" role="menu" data-no-drag>
                <p class="menu-label">Rolle</p>
                {#each column.stage ? [column.maps_to_status] : ROLES as role (role)}
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={column.maps_to_status === role}
                    disabled={column.stage !== null}
                    onclick={() => {
                      colMenu = null;
                      void saveColumn(column, { maps_to_status: role });
                    }}
                  >
                    <span class="role" data-role={role}></span>{ROLE_NAMES[role]}
                    {#if column.maps_to_status === role}<Icon name="check" size="sm" />{/if}
                  </button>
                {/each}
                <hr />
                <button type="button" role="menuitem" onclick={() => renameColumn(column.id)}>
                  <Icon name="pencil" size="sm" />Umbenennen
                </button>
                {#if column.stage === null}
                  <button
                    type="button"
                    role="menuitem"
                    class="danger"
                    onclick={() => {
                      colMenu = null;
                      removingColumn = { id: column.id, held: cards.length };
                    }}
                  >
                    <Icon name="trash-2" size="sm" />Entfernen
                  </button>
                {:else}
                  <p class="menu-label">
                    {column.stage === "review" ? "Review-Spalte des Ablaufs" : "Vorschlag-Spalte des Ablaufs"} — bleibt bei jedem Brett
                  </p>
                {/if}
              </div>
            {/if}
          </header>

          {#if removingColumn?.id === column.id}
            <div class="col-remove">
              {#if removingColumn.held === 0}
                <span>Spalte entfernen?</span>
                <button class="ax-btn danger" onclick={() => removeColumn(column, null)}>Entfernen</button>
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
              <button class="ax-btn" onclick={() => (removingColumn = null)}>Abbrechen</button>
            </div>
          {/if}
          <div class="cards">
            {#each cards as card, index (card.id)}
              {#if target?.columnId === column.id && target.index === index && (dragging || carrying)}
                <div class="marker" aria-hidden="true"></div>
              {/if}
              <article
                class="card"
                data-tone={card.labels.length > 0 ? labelColorIndex(card.labels[0], LABEL_TOKENS) : undefined}
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
                  onclick={() => openCard(card)}
                  onkeydown={(event) => onCardKey(event, card, column.id, index)}
                >
                  <span class="title"><span class="num">#{card.id}</span> {card.title}</span>
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
              <Icon name="plus" size="sm" /> Karte
            </button>
          {/if}
        </section>
      {/each}

    </div>
    {#if sideCardId !== null}
      <!-- B4: the card beside the board, which stays in view; another card's click swaps it. -->
      <aside class="side-detail" aria-label="Karte">
        <header class="side-head">
          <span>Karte</span>
          <IconButton icon="x" size="sm" label="Karte schließen (Esc)" onclick={() => showInSide(null)} />
        </header>
        {#if detail}
          {@render cardDetail(detail)}
        {:else}
          <p class="notice">Diese Karte gibt es nicht mehr.</p>
        {/if}
      </aside>
    {/if}
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
    min-height: calc(22px * var(--ax-ui-scale));
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

  /* The large board's toolbar (B5): the filter field takes what width is left. */
  .filter {
    display: flex;
    flex: 1 1 calc(160px * var(--ax-ui-scale));
    align-items: center;
    gap: var(--ax-space-2);
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
  }
  .filter:focus-within {
    border-color: var(--ax-accent);
  }
  .kanban .filter input[type="search"] {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .label-filters {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }

  /* The board and, in the large board, the card beside it (B4). */
  .work {
    display: flex;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    min-height: 0;
  }
  .side-detail {
    display: flex;
    flex-direction: column;
    flex: 0 0 min(calc(400px * var(--ax-ui-scale)), 45%);
    min-height: 0;
    padding: var(--ax-space-2) var(--ax-space-3) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: var(--ax-surface-1);
  }
  .side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  /* D1: squeeze to --min-col, then scroll sideways. A board that hides a
     column lies about the state of the work. */
  .board {
    display: flex;
    gap: var(--ax-space-2);
    flex: 1 1 auto;
    /* Shrinks beside the side panel instead of pushing it out (B4). */
    min-width: 0;
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
    /* D3, B2: a lane the cards lie on — a tint, rounded, never a frame. */
    background: color-mix(in srgb, var(--ax-surface-1) 70%, var(--ax-bg));
    border-radius: var(--ax-radius-lg);
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
    align-items: center;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-1) var(--ax-space-2);
  }
  /* The column's role (B2): a dot — open grey, in progress the accent, done green. */
  .role {
    flex: 0 0 auto;
    width: calc(8px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-text-muted);
  }
  /* An attribute, not a class: `open` is already the card button's class here. */
  .role[data-role="doing"] {
    background: var(--ax-accent);
  }
  .role[data-role="done"] {
    background: var(--ax-success);
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
  /* Taken out of the flow: laid out in it, it would keep its width even while invisible and squeeze
     the column name on a narrow column. It overlays the name's tail only while shown. */
  .col-tools {
    position: absolute;
    right: 0;
    top: 50%;
    translate: 0 -60%;
    display: inline-flex;
    padding-left: var(--ax-space-2);
    background: color-mix(in srgb, var(--ax-surface-1) 70%, var(--ax-bg));
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .col:hover .col-tools,
  .col-tools:focus-within,
  .col-tools.menu-shown {
    opacity: 1;
    pointer-events: auto;
  }
  .col-remove select {
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
  }

  /* The column's "…" menu (B2). */
  .col-menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 3;
    display: flex;
    flex-direction: column;
    min-width: calc(180px * var(--ax-ui-scale));
    padding: var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
  }
  .col-menu .menu-label {
    margin: var(--ax-space-1) var(--ax-space-2);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .col-menu button {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 0;
    border-radius: var(--ax-radius-sm);
    background: none;
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }
  .col-menu button:hover {
    background: var(--ax-surface-3);
  }
  .col-menu button.danger {
    color: var(--ax-danger);
  }
  .col-menu hr {
    width: 100%;
    margin: var(--ax-space-1) 0;
    border: 0;
    border-top: 1px solid var(--ax-border);
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
  /* The count as a small pill (B2). */
  .count {
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: 1.6;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  /* One card look (editor-look B1, B7): the theme's card surface with a hairline, lifting a little
     under the pointer. A light theme may add its own resting shadow (`--ax-card-shadow`). */
  .card {
    background: var(--ax-card-bg);
    border: 1px solid var(--ax-border);
    box-shadow: var(--ax-card-shadow);
    border-radius: var(--ax-radius-md);
    padding: var(--ax-space-2) var(--ax-space-3);
    transition:
      box-shadow var(--ax-dur-fast) var(--ax-ease),
      border-color var(--ax-dur-fast) var(--ax-ease),
      translate var(--ax-dur-fast) var(--ax-ease);
  }
  .card:hover {
    border-color: var(--ax-border-strong);
    box-shadow: var(--ax-card-shadow-raised);
    translate: 0 -1px;
  }
  /* B6: a stripe in the first label's colour, so cards of one topic belong together at a glance. */
  .stripes .card[data-tone] {
    border-left: 3px solid var(--tone);
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

  /* "+ Karte": a quiet row at the column's foot (B3). */
  .add {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    margin-top: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
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
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--ax-text-muted);
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
  /* The card's number: what the CLI, the history and a planner's proposals call it. */
  .num,
  .detail-num {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-variant-numeric: tabular-nums;
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
    gap: calc(3px * var(--ax-ui-scale));
  }
  .dot {
    width: calc(7px * var(--ax-ui-scale));
    height: calc(7px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--tone);
  }
  .chips {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  /* D2: labels are the only thing on the board that carries colour — filled, softly tinted (B1). */
  .chip {
    padding: 0 calc(7px * var(--ax-ui-scale));
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--tone) 18%, transparent);
    color: color-mix(in srgb, var(--tone) 85%, var(--ax-text));
    font: inherit;
    font-size: var(--ax-font-size-xs);
    line-height: 1.7;
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
  /* The due date with its calendar icon (B1): quiet, the warning colour today and tomorrow, red once past. */
  .due {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    margin-left: auto;
    color: var(--ax-text-muted);
    white-space: nowrap;
  }
  .due.soon {
    color: var(--ax-warning);
  }
  .due.over {
    color: var(--ax-danger);
  }
  /* Who holds the card (B1): a small badge — a bot for an agent, a person for a human. */
  .who {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    max-width: 100%;
    margin: var(--ax-space-2) 0 0;
    padding: 0 var(--ax-space-2) 0 var(--ax-space-1);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: 1.7;
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

  /* ----------------------------------------------------------- agent flow --- */
  .flow-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1);
    margin: var(--ax-space-1) 0 0;
    font-size: var(--ax-font-size-xs);
  }
  .state-badge {
    padding: 0 calc(7px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--ax-warning) 18%, transparent);
    color: var(--ax-warning);
    line-height: 1.7;
  }
  .state-badge[data-state-tone="bad"] {
    background: color-mix(in srgb, var(--ax-danger) 18%, transparent);
    color: var(--ax-danger);
  }
  .state-badge[data-state-tone="muted"] {
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
  }
  .flow-tag {
    color: var(--ax-text-muted);
  }
  .flow-head {
    margin: var(--ax-space-3) 0 var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .fields select {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .deps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1) var(--ax-space-2);
    margin-bottom: var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }
  .deps-label,
  .deps-none {
    color: var(--ax-text-muted);
  }
  .dep-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    max-width: 100%;
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dep-chip.waiting {
    border-color: var(--ax-warning);
  }
  .dep-chip button {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ax-text-muted);
    cursor: pointer;
  }
  .dep-add {
    padding: 0 var(--ax-space-1);
    border: 1px dashed var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: none;
    color: var(--ax-text-muted);
    font: inherit;
    font-size: var(--ax-font-size-xs);
  }
  .detail-error {
    margin: 0 0 var(--ax-space-3);
    color: var(--ax-warning);
    font-size: var(--ax-font-size-sm);
    word-break: break-word;
  }
  .history {
    margin-top: var(--ax-space-3);
  }
  .note-form input {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-2);
    color: inherit;
    font: inherit;
    font-size: var(--ax-font-size-sm);
  }
  .history ul {
    margin: var(--ax-space-2) 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .history-empty {
    margin: var(--ax-space-2) 0 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .h-kind {
    font-weight: 600;
  }
  .h-who,
  .h-when {
    margin-left: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .h-text {
    margin: 2px 0 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .boards-pop {
    position: absolute;
    top: calc(44px * var(--ax-ui-scale));
    right: var(--ax-space-3);
    z-index: 5;
    width: min(calc(320px * var(--ax-ui-scale)), 90%);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-pop);
  }
  .first-board {
    max-width: calc(360px * var(--ax-ui-scale));
    margin: var(--ax-space-5) auto;
  }
</style>
