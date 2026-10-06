-- Where a spending reading came from (a2a.md, CP-A8c, found by the second ultrareview).
--
-- A session's usage is read from a harness record: Claude Code's transcripts (all of the session's ids together, one ever
-- growing figure) or one Opencode session. An escalation changes a session's engine in place and starts a new Opencode
-- session, whose figure begins near zero; measured against everything the agent ever spent, the first looks of the new
-- session would write nothing. The increase is therefore taken against what the ledger holds for the same *source* — the
-- literal 'claude', or the Opencode session id. Rows written before this column existed (source '') count toward every
-- source of their agent: their origin is not known, and counting them twice would be the worse mistake.
ALTER TABLE session_spend ADD COLUMN source TEXT NOT NULL DEFAULT '';
