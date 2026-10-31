# Terminal

How to open and set up the Terminal tile (shell, font, colours) and which keys behave differently there.

## What it is

The “Terminal” tile is a real shell in Axiomata. Each tile has its own shell; you can open as many as you like. The
same terminal technology sits in the Studio's panes (agents, Run, master panes, Glut). The settings apply to **all**
Terminal tiles at once.

## Where the buttons are

Flip side of the tile (gear):

- **“Session”:** “Shell” (empty = `$SHELL`), “Start directory”, “Scrollback (lines)”, “Environment variables” (one
  line `KEY=value` per variable), “Visual bell”. Applies only to the **next** terminal you open.
- **“Font”:** font (bundled ones, “Installed on this Mac (monospaced)” or “Custom…”), “Font family”, “Size
  (px)”, “Weight”.
- **“Appearance”:** “Theme” (among others “xterm (default)”, “Dracula”, “Nord”, “Catppuccin …”, “Tokyo Night”,
  “GitHub …”, “Rosé Pine”, “One Dark”), “Background opacity”, “Cursor style”, “Cursor blink”, “Bold text in bright
  colour”. Applies immediately.

## Common tasks

1. **Open a terminal:** “Terminal” in the App Ring (a new tile every time), Dock → “Add module”, or ask the chat
   (“open a terminal”).
2. **Copy text:** select it with the mouse — it is copied when you let go. Holding Shift, this also works while a
   program is using the mouse.
3. **Scroll back:** mouse wheel, ⌘⇧↑/↓ or Shift+PageUp/PageDown (on the Mac Shift+fn+arrows).

## Traps

- On the Mac ⌘ never reaches the shell, so the app shortcuts always work. On Windows/Linux Ctrl belongs to the shell:
  there the app shortcuts in the terminal use Ctrl+Shift (copy/paste Ctrl+Shift+C/V, close Ctrl+Shift+W).
- ⌘W closes nothing in the terminal.
- For prompts with symbols (Powerline, Starship) choose the font “JetBrainsMono Nerd Font Mono” — it is the only
  real Nerd Font among the bundled ones.
- Option stays Option (no Alt/Meta).
- A program may write to the clipboard only while its terminal is in front, and may never read it.
- ⌘+click opens links in the terminal.

## See also

[Studio](studio.md) · [Glut](glut.md) · [Settings](einstellungen.md)
