-- Durable runs (docs/plans/durable.md, CP3): a run's row is written when it starts, as status 'running', and finished
-- when it ends; a row still 'running' whose process is gone becomes 'interrupted'. `pid` is the process that runs it
-- (the app or the CLI — both may run skills at once), `journal` the id of a Glut run's journal, which makes an
-- interrupted run resumable. NULL for rows from before, and for runs without either.
ALTER TABLE runs ADD COLUMN pid INTEGER;
ALTER TABLE runs ADD COLUMN journal TEXT;
