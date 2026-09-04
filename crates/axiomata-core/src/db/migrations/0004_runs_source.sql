-- Distinguishes a manually-triggered run (the CLI's `run-skill`, the Tauri
-- dashboard's "run now", a chat/instruct turn) from one a routine (M3) fired
-- unattended. Added retroactively -- every row from before this migration
-- predates the distinction, so the default ('manual') is the correct
-- backfilled value for all of them, not a guess: routines could not have
-- fired any of those runs.
ALTER TABLE runs ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
