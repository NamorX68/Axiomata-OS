-- Kanban boards, columns, and cards (milestone M7.0).
--
-- Built human-first but agent-ready from the first line: in M7.5 several AI
-- agents claim cards from the same board concurrently, and concurrency cannot
-- be retrofitted once rows exist without it.
--
-- The load-bearing idea: THE COLUMN CARRIES THE STATUS, the card does not.
-- A card's status is `columns.maps_to_status` of the column it sits in, so the
-- contradiction "card in Done, status says doing" cannot be represented at all.
-- Re-mapping a column instantly re-states every card in it, for free.
--
-- What a card does carry is the two signatures: `claimed_by` (who took it) and
-- `verified_by` (who signed it off). Verification deliberately requires a
-- *different* party — see the CHECK at the bottom of `cards`.

CREATE TABLE IF NOT EXISTS boards (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    -- RFC 3339 bookkeeping, like every other timestamp in this database.
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS board_columns (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id INTEGER NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    -- Sort key within the board. REAL so inserting between two neighbours is
    -- their midpoint — one UPDATE instead of renumbering the whole board.
    position REAL NOT NULL,
    -- 'open' | 'doing' | 'done'. The single source of a card's status.
    maps_to_status TEXT NOT NULL CHECK (maps_to_status IN ('open', 'doing', 'done')),
    -- Target for the composite foreign key on `cards` below. Without it a card
    -- could point at a column belonging to a *different* board.
    UNIQUE (id, board_id)
);

-- Hot query: "the columns of board X, in order".
CREATE INDEX IF NOT EXISTS idx_board_columns_board
    ON board_columns (board_id, position);

CREATE TABLE IF NOT EXISTS cards (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id  INTEGER NOT NULL,
    column_id INTEGER NOT NULL,
    -- Sort key within the column; same midpoint scheme as `board_columns`.
    position  REAL NOT NULL,
    title     TEXT NOT NULL,
    body      TEXT NOT NULL DEFAULT '',
    -- JSON array of strings. Filtering happens in Rust/TS, not in SQL: a board
    -- is a few hundred rows at most, and a JSON1 dependency for that would be
    -- a poor trade.
    labels    TEXT NOT NULL DEFAULT '[]',
    -- Actor strings, all four of them in the same shape: 'human:<name>' or
    -- 'agent:<name>'. One shape matters — `verified_by <> claimed_by` below
    -- compares them directly, so two differently-formed identities would make
    -- that check meaningless.
    assignee    TEXT,
    claimed_by  TEXT,
    claimed_at  TEXT,
    verified_by TEXT,
    verified_at TEXT,
    -- RFC 3339. Optional due date; overdue is derived at read time.
    due_at      TEXT,
    -- Set = archived. Archived cards are hidden by default rather than deleted,
    -- so a Done column cannot grow without bound and nothing is lost. There is
    -- deliberately no automatic archiving.
    archived_at TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,

    -- Composite: the column must belong to the same board as the card.
    -- RESTRICT, never CASCADE — cascading a column delete would silently take
    -- its cards with it. `delete_column` moves them first, in one transaction.
    FOREIGN KEY (column_id, board_id)
        REFERENCES board_columns (id, board_id) ON DELETE RESTRICT,

    -- Each signature is all-or-nothing: who and when travel together.
    CHECK ((claimed_by IS NULL) = (claimed_at IS NULL)),
    CHECK ((verified_by IS NULL) = (verified_at IS NULL)),
    -- The two-party rule as a database invariant rather than a code path:
    -- verification requires an existing claim by somebody *else*. No future
    -- caller, however careless, can sign off its own work.
    CHECK (
        verified_by IS NULL
        OR (claimed_by IS NOT NULL AND verified_by <> claimed_by)
    )
);

-- Hot query: "the cards of column X, in order".
CREATE INDEX IF NOT EXISTS idx_cards_column ON cards (column_id, position);
-- The board-wide read, which is what every render does: filters `board_id` and
-- orders by `column_id, position, id`. All four columns are in the index and in
-- that order, so the ordering is served by the index rather than by a temporary
-- sort. A narrower `(board_id, column_id)` would cover the filter but leave the
-- sort to be redone on every read — cheap at a few hundred cards, wasteful once
-- several agents poll the board. Added while the table is still empty, which is
-- the only moment an index costs nothing.
CREATE INDEX IF NOT EXISTS idx_cards_board_order
    ON cards (board_id, column_id, position, id);
-- Due-date filtering ("what is overdue on this board").
CREATE INDEX IF NOT EXISTS idx_cards_due ON cards (board_id, due_at);
-- "What has this actor claimed" — the agents' own view in M7.5.
CREATE INDEX IF NOT EXISTS idx_cards_claimed ON cards (claimed_by);
