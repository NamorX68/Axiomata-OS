-- The mailbox between agent sessions and the owner (docs/plans/a2a.md, CP-A3, decisions A8 and A29).
--
-- Two tables, because one message can reach several inboxes (a message to a role is resolved to every live session
-- of that role) while its text must exist once:
--
--   * `mail_messages` — the message as it was written: who sent it, who it was addressed to, the parts, the chain
--     counter. Append-only apart from `status` (a held message is released by the owner) and `chain` (reset on release).
--   * `mail_deliveries` — one row per concrete inbox. Read and nudge bookkeeping live here, so two sessions of one
--     role read the same message independently.
--
-- There is no foreign key to `cards` or to `ide_agents`: the board and the IDE are separate crates that must stay
-- extractable on their own (A11), a card may be archived long before its messages expire, and an agent row may be
-- deleted while its mail is still the owner's record of what happened. Retention (A29) is `mailbox::purge`.
--
-- Addresses are stored as text so a human reading the database sees what they say:
--   sender   'session:<id>' | 'owner' | 'system'
--   to_addr  'session:<id>' | 'role:<name>' | 'owner'
--   inbox    'session:<id>' | 'owner'

CREATE TABLE IF NOT EXISTS mail_messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at  TEXT NOT NULL,
    sender      TEXT NOT NULL,
    to_addr     TEXT NOT NULL,
    -- The card (task) this is about. A plain number, see above.
    card_id     INTEGER,
    -- 'message' | 'ack' | 'notice'. An ack (a bare confirmation) and a notice (written by the system, e.g. a bounce)
    -- are never answered, which is what ends a conversation instead of letting it echo (A8).
    kind        TEXT NOT NULL DEFAULT 'message' CHECK (kind IN ('message', 'ack', 'notice')),
    -- JSON array of parts: {"kind":"text","text":…} | {"kind":"file","name":…,"uri":…} | {"kind":"data","data":…}.
    parts       TEXT NOT NULL,
    in_reply_to INTEGER REFERENCES mail_messages (id) ON DELETE SET NULL,
    -- How many hops this message is into a chain of replies (A8): 1 for a fresh message, parent + 1 for a reply.
    chain       INTEGER NOT NULL DEFAULT 1 CHECK (chain >= 0),
    -- 'delivered' | 'held' (the chain limit stopped it, the owner decides) | 'undeliverable' (no live recipient).
    status      TEXT NOT NULL DEFAULT 'delivered' CHECK (status IN ('delivered', 'held', 'undeliverable'))
);

-- The per-sender-per-card limit counts exactly these.
CREATE INDEX IF NOT EXISTS idx_mail_messages_sender ON mail_messages (sender, card_id);
CREATE INDEX IF NOT EXISTS idx_mail_messages_card ON mail_messages (card_id);

CREATE TABLE IF NOT EXISTS mail_deliveries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id  INTEGER NOT NULL REFERENCES mail_messages (id) ON DELETE CASCADE,
    inbox       TEXT NOT NULL,
    read_at     TEXT,
    -- How often the studio typed a "you have mail" line into the session's terminal for this delivery (A8, way 2).
    -- Capped by the application so an agent that ignores it is not nagged forever.
    nudge_count INTEGER NOT NULL DEFAULT 0 CHECK (nudge_count >= 0)
);

-- "What is waiting in this inbox" — the hot query of both read_inbox and the nudge check.
CREATE INDEX IF NOT EXISTS idx_mail_deliveries_inbox ON mail_deliveries (inbox, read_at);
CREATE INDEX IF NOT EXISTS idx_mail_deliveries_message ON mail_deliveries (message_id);
