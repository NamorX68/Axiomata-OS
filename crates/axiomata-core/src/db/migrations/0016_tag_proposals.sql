-- Tags an agent wanted that are not on the vault's tag list yet (docs/plans/tags-links.md Q6, Q10, V1).
--
-- An agent never adds to the list itself: it proposes, and the owner accepts (the tag goes onto TAGS.md and onto the
-- notes that wanted it, `notes`, a JSON array of workspace-relative paths) or turns it down. `decision` is NULL while
-- the proposal is open, otherwise 'accepted' or 'rejected'. One open proposal per tag: a second agent asking for it
-- joins its notes to the first.
CREATE TABLE tag_proposals (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    tag          TEXT    NOT NULL,
    description  TEXT    NOT NULL DEFAULT '',
    notes        TEXT    NOT NULL DEFAULT '[]',
    proposed_by  TEXT    NOT NULL,
    proposed_at  TEXT    NOT NULL,
    decision     TEXT,
    decided_at   TEXT
);
CREATE UNIQUE INDEX tag_proposals_open_tag ON tag_proposals (tag) WHERE decision IS NULL;
