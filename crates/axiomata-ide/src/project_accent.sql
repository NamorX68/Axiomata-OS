-- The marker colour of a project (docs/plans/a2a.md is not involved; card #70).
--
-- A slot into the twelve hues the theme draws areas in (`themeSwatches` in the frontend), not a colour: the colour comes
-- from the theme in effect, so the marker follows a theme change and is readable in every one. NULL = no marker.
ALTER TABLE projects ADD COLUMN accent_slot INTEGER CHECK (accent_slot IS NULL OR accent_slot BETWEEN 0 AND 11);
