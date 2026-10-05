-- Where the studio left a plan's line (docs/plans/a2a.md, CP-A8).
--
-- `line_tip` is the commit the studio last made on the line. Every integration checks that the line still is there: a
-- branch of that name is a branch like any other, and anything that moved it (a stray `git branch`, `git update-ref`) would put work on
-- the line that no reviewer saw.
ALTER TABLE plans ADD COLUMN line_tip TEXT;
