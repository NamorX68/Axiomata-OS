-- The card a session was started for (docs/plans/a2a.md, CP-A6b).
--
-- Until now "this session works on a card" was read off the card's claim. A reviewer holds no claim — it judges the
-- card of the session that worked on it — and a session of the owner's own making that took a card by itself must not
-- count as one the studio started. So the studio writes it down: the card, and whether this session works on it or
-- reviews it. No foreign key, like the engine and the role: the card lives in the board's tables, which this crate does
-- not know (A11).

-- NULL = a session of the owner's own making, started by hand.
ALTER TABLE ide_agents ADD COLUMN card_id INTEGER;

-- 1 = a reviewer: its worktree is a detached checkout of the state it reviews, not a branch of its own.
ALTER TABLE ide_agents ADD COLUMN card_review INTEGER NOT NULL DEFAULT 0;

-- The commit a reviewer's worktree is cut from (the snapshot of the work under review). NULL for everything else.
ALTER TABLE ide_agents ADD COLUMN start_ref TEXT;
