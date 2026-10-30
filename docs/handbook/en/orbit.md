# Orbit, App Ring, tiles and dock

How the home screen is built: the Orbit in the background, the App Ring around it, the tiles on the surface and the dock with its buttons.

## What it is

- **The Orbit** is the cloud in the background: the vault as a rotating 3-D picture behind all tiles. A click into
  the cloud opens the [Second Brain](second-brain.md) in the circle, a click on empty ground closes it again.
  The most recently used files lie on the inner ring; a click opens the file.
- **The App Ring** lies outside around the Orbit. Left of the “+” are the built-in entries: the tiles “Memory”,
  “Skills Deck”, “Routines”, “ToDo”, “Calendar”, “Reminders”, “Mail”, “Terminal” and the two large views “Studio”
  and “Kanban”. Right of the “+” are Mac apps you added yourself; a click starts them.
- **Tiles** are the small windows on the surface. Most exist only once (a second click brings them to the front);
  “Terminal” opens a new tile every time.
- **The dock** is the bar with “Axiomata” and the vault's name, the jumps “Studio”, “Kanban”, “Brain”, “Note”, the
  daily heartbeat (today's runs and “Spent today: … of …”, red from 80 % of the daily limit), the search (“Search or
  run”, ⌘K) and the buttons “Add module”, “Theme” and “Settings” (⌘,).

## Where the buttons are

- **Tile:** dragging the head (handle “⠿”) moves it; the sides, the bottom and the lower corners change its size
  (never the top). It snaps to the dot grid and sticks to neighbours (both in the settings under “Canvas”). The gear
  “Settings” flips the tile: the back “<Title> · Settings” carries its settings, “×” (“Back”) flips it back. The “×”
  on the head (“Remove”) removes the tile.
- **App Ring, right-click:** “Add to a new group”, “Add to group …”; for Mac apps also “Change icon” and “Remove”;
  on a group circle “Rename” and “Change icon”; inside a group “Remove from group”.
- **“+” in the ring (“Add app”):** switch “Internal” / “External” with a search field. Internal hides or shows
  built-in entries (“Hide” / “Show”), External adds Mac apps (“Add” / “Remove”).
- **Corner of the Orbit:** ⏸/▶ pauses or starts the motion; “×” removes the background.
- **Focus mode:** ⌘⇧. — all other tiles and the Orbit go blurred; the chip “Focus” in the dock ends it.

## Common tasks

1. **Add a tile:** in the dock “Add module” → choose a module (or click the entry in the App Ring).
2. **Set up a tile:** gear on the head → change the back → “×” flips it back.
3. **Put a Mac app in the ring:** “+” in the ring → “External” → search for the app → “Add”.

## Traps

- The “×” on a tile's head removes the tile **without asking**; its data (skills, routines, ToDo.md …) stays, only
  the tile is gone — “Add module” brings it back.
- Built-in entries cannot be deleted from the ring, only hidden under “Add app” → “Internal”.
- Kanban and Studio are **not tiles** but large views; they are not in “Add module”.
- With “Motion: Off” (or macOS “Reduce motion”) the Orbit does not rotate.

## See also

[Second Brain](second-brain.md) · [Settings](einstellungen.md) · [Spotlight](spotlight.md) ·
[Chat](chat.md)
