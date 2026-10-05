-- How deep a proposal lies (docs/plans/a2a.md, A7, CP-A8c-3).
--
-- A planner's cards are the plan's roots. A session that works a card may propose follow-up cards of its own, and those
-- sessions may propose again: without a bound, one confused agent could grow a tree of cards nobody asked for. A card a
-- *working* session proposed gets depth = its parent's depth + 1 here; a card without a row (a planner's, the owner's)
-- is depth 0. Kept beside `cards` rather than in it: the card model stays as it was, and a row exists only where a
-- session proposed from a card.
CREATE TABLE IF NOT EXISTS card_proposal_depth (
    card_id INTEGER PRIMARY KEY REFERENCES cards (id) ON DELETE CASCADE,
    depth   INTEGER NOT NULL CHECK (depth >= 1)
);
