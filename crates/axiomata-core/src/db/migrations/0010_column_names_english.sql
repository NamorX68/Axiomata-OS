-- The default board columns get English names (the whole interface is English now).
--
-- Only a column that still has its German default name *and* the role or status that default had is renamed: a column the
-- owner named himself, or reused for something else, keeps its name. There is no unique constraint on a column's name,
-- so no rename can collide. "Review" is the same word in both languages and stays.
UPDATE board_columns SET name = 'Proposal'    WHERE name = 'Vorschlag' AND stage = 'proposal';
UPDATE board_columns SET name = 'Open'        WHERE name = 'Offen'     AND stage IS NULL AND maps_to_status = 'open';
UPDATE board_columns SET name = 'In Progress' WHERE name = 'In Arbeit' AND stage IS NULL AND maps_to_status = 'doing';
UPDATE board_columns SET name = 'Done'        WHERE name = 'Fertig'    AND stage IS NULL AND maps_to_status = 'done';
