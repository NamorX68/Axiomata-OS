-- Who proposed a card (docs/plans/a2a.md, A7, "Plan bearbeiten und grillen", Q2).
--
-- A session may change or take back only the proposals it made itself. That ownership must not rest on a history line: a
-- note is anyone's to write, and a session that wrote "proposed by the session" on another's card would have been given it.
-- This table is written by the one function that makes a proposal (`flow::propose_card_from`) and by nothing else.
CREATE TABLE IF NOT EXISTS card_proposers (
    card_id INTEGER PRIMARY KEY REFERENCES cards (id) ON DELETE CASCADE,
    actor   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_card_proposers_actor ON card_proposers (actor);

-- Proposals made before this table existed: their only trace is the line `propose_card` wrote, and until now nothing could
-- act on it. Taken over once, as it stands.
INSERT OR IGNORE INTO card_proposers (card_id, actor)
    SELECT card_id, actor FROM card_events WHERE kind = 'note' AND text = 'proposed by the session';
