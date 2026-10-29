-- The one run over the whole vault (docs/plans/tags-links.md Q9, TL6).
--
-- `tag_run` holds the step a run is in ('list' proposes the tag list, 'notes' plans every note), at most one row;
-- `notes tags todo` tells the skill which step it does. `tag_plan` is the preview of the notes step: what a dry run of
-- `notes tag` would set on each note (JSON arrays of tags, link names and proposed tags, the proposals also as
-- [tag, description] pairs), or why the run skipped the note (`skipped`). The owner writes it into the notes, or throws
-- it away; nothing is written before.
CREATE TABLE tag_run (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    phase       TEXT    NOT NULL,
    started_at  TEXT    NOT NULL
);
CREATE TABLE tag_plan (
    note        TEXT    PRIMARY KEY,
    tags        TEXT    NOT NULL DEFAULT '[]',
    related     TEXT    NOT NULL DEFAULT '[]',
    proposed    TEXT    NOT NULL DEFAULT '[]',
    proposals   TEXT    NOT NULL DEFAULT '[]',
    skipped     TEXT,
    planned_at  TEXT    NOT NULL
);
