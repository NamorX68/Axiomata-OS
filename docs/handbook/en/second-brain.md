# Second Brain

How to view the vault as a picture: the Bundle, Map and Hex views, the filter field, “How your vault grew”, time travel and a note's detail panel.

## What it is

The Second Brain shows the whole vault as a graphic: every note a point or a cell, every area (top-level folder) a
colour of its own, links as lines, the skills around the outside. It lies as a disc over the [Orbit](orbit.md), not
as a screen of its own.

- **Bundle:** all notes on a ring, one arc per area, the newest first; links run bundled through the middle, those
  of the last 21 days glow.
- **Map:** a map — every area a country, every note a field, the oldest in the middle; links are roads (zoom in).
- **Hex:** a honeycomb — every note a cell of its own, every area a wedge, the newest near the middle.

## Where the buttons are

- **Open:** dock “Brain”, a click into the Orbit cloud, ⌘K “Open Second Brain” or `/brain` in the chat.
- **At the top of the view:** “Layout” (“Bundle” / “Map” / “Hex”), “Group by” (“Areas” / “Folders”), the search
  field “Find notes or #tag”, “▶ How your vault grew”, “Reset view”, “Reload” and “?” (help with legend); below it
  “… notes · … areas · … links”.
- **Time travel:** the clock “Time travel: how the vault grew” opens a slider; “Grow the vault” plays it,
  “Back to today” closes it.
- **Detail panel** (after a click on a note, movable and resizable): “Folder”, “Size”, “Modified”, “Path”, “Links”,
  “Tags” as chips, a preview; buttons “Open”, “Copy path”, “Fly to”, “Delete”; the lists “Links to” and
  “Linked from”. For an area “Notes in this area”, for a skill “▶ Run”.
- **Set permanently:** Settings → Appearance → “Orbit & Second Brain” (“Speed”, “Labels”, “File names”, “Shimmer”,
  “Layout”).

## Common tasks

1. **Find notes:** type words into the search field (each must match; name, path or area) or `#tag` — also matches
   nested tags below it. Everything else is dimmed.
2. **Open a note:** click it → “Open” (opens it for reading in the file window).
3. **Watch the growth:** “▶ How your vault grew” plays the vault's history in 20 seconds; Esc ends it.

## Traps

- “Delete” in the detail panel deletes the **file in the vault** (after the question “Really delete”) — not just the
  point.
- Esc works in steps: first it ends the playback, then it clears the selection, then it closes the view.
- If a note you just created is missing, “Reload” reads the vault in again.
- There is no `tag:x` syntax — only `#x`.

## See also

[Orbit](orbit.md) · [Tags and links](tags-links.md) · [Notes](notizen.md) · [Editor](editor.md)
