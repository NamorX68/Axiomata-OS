-- The agent flow on the board (docs/plans/a2a.md, CP-A2, decisions A12–A19).
--
-- Version 1 stays untouched; this is an ALTER-and-ADD migration of its own. What it adds, and why each piece is shaped
-- the way it is:
--
--   * `board_columns.stage` — a column's ROLE ('proposal' | 'review'), refining its status instead of adding statuses.
--     The status CHECK of version 1 cannot be widened without rebuilding a table that `cards` references; a role next to
--     the status needs no rebuild and keeps the rule that the column carries the status. 'proposal' columns are open,
--     'review' columns are doing — the application enforces the pairing (a CHECK across two columns would be
--     redundant with it and unreadable in an error message).
--   * `plans` — the unit of approval, automation, limits and the graph.
--   * the new `cards` columns — only what is NOT derivable from the column and the signatures (A12).
--   * `card_deps` — "needs first" edges. Acyclicity and "same plan" are checked in Rust inside one transaction; the
--     schema only rules out the trivial self-edge.
--   * `card_events` — append-only history (A19). Never updated.

ALTER TABLE board_columns ADD COLUMN stage TEXT
    CHECK (stage IS NULL OR stage IN ('proposal', 'review'));

-- A board has at most one column per role: "the review column" and "the proposal column" are looked up by role, so a
-- second one would make that lookup ambiguous.
CREATE UNIQUE INDEX IF NOT EXISTS idx_board_columns_stage ON board_columns (board_id, stage) WHERE stage IS NOT NULL;

CREATE TABLE IF NOT EXISTS plans (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id       INTEGER NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved', 'closed')),
    -- NULL = cards are started by hand. A number = start ready cards by themselves, up to that many at once.
    auto_start_max INTEGER CHECK (auto_start_max IS NULL OR auto_start_max > 0),
    -- Plan-wide limits (A9); NULL = the global default applies.
    max_cost_usd   REAL CHECK (max_cost_usd IS NULL OR max_cost_usd > 0),
    max_tokens     INTEGER CHECK (max_tokens IS NULL OR max_tokens > 0),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    approved_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_plans_board ON plans (board_id);

-- SET NULL: deleting a plan must not delete its cards.
ALTER TABLE cards ADD COLUMN plan_id INTEGER REFERENCES plans (id) ON DELETE SET NULL;
-- The role the card is meant for (a slug under ~/.axiomata/agents) and why — an intention, not an identity. The
-- session that actually works on it is `claimed_by`.
ALTER TABLE cards ADD COLUMN agent TEXT;
ALTER TABLE cards ADD COLUMN agent_reason TEXT;
ALTER TABLE cards ADD COLUMN tier TEXT CHECK (tier IS NULL OR tier IN ('light', 'medium', 'heavy'));
ALTER TABLE cards ADD COLUMN kind TEXT;
-- Acceptance criteria as Markdown. Free text on purpose: the reviewer reads it as text.
ALTER TABLE cards ADD COLUMN acceptance TEXT NOT NULL DEFAULT '';
-- How often a reviewer sent the card back; two returns escalate it (A26).
ALTER TABLE cards ADD COLUMN returned_count INTEGER NOT NULL DEFAULT 0;
-- What the working agent asked and waits for an answer to; NULL = nothing pending.
ALTER TABLE cards ADD COLUMN input_required TEXT;
ALTER TABLE cards ADD COLUMN taken_over_at TEXT;
ALTER TABLE cards ADD COLUMN failed_at TEXT;
ALTER TABLE cards ADD COLUMN canceled_at TEXT;

CREATE TABLE IF NOT EXISTS card_deps (
    card_id       INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    depends_on_id INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    PRIMARY KEY (card_id, depends_on_id),
    CHECK (card_id <> depends_on_id)
);

-- "Who is waiting for this card" — the other direction of the primary key.
CREATE INDEX IF NOT EXISTS idx_card_deps_on ON card_deps (depends_on_id);

CREATE TABLE IF NOT EXISTS card_events (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    at      TEXT NOT NULL,
    -- Same actor shape as the signatures ('human:<name>' | 'agent:<name>').
    actor   TEXT NOT NULL,
    kind    TEXT NOT NULL,
    text    TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_card_events_card ON card_events (card_id, id);
