# File app and editor

How to open, read and edit files: the file window, the editor in the Studio, reading view, Vi mode, search, language servers (LSP) and jump history.

## What it is

Files open in two places with the same editor:

- in the **floating file window** — from the Second Brain, with `/open <path>`, ⌘K or when an agent shows you
  something; with “Open in the file app” you switch to the Studio;
- in the **Studio**, mode “Editor”, with file tree, tabs and split groups (see [Studio](studio.md)).

Markdown and HTML can be shown as source or rendered; images appear as images. For code, language servers start by
themselves (errors, jump to definition, completion, rename, formatting), provided they are installed on the Mac.

## Where the buttons are

- **View:** ⇧⌘V switches “Source” / “Preview” / “Side by side”. ⌥Z toggles line wrapping per file.
- **Sidebar in the Studio:** “Files” (right-click: “New file”, “New folder”, “Rename”, “Delete…”), “Search”, “Git”;
  plus “Filter symbols” in the outline.
- **Search:** ⌘F in the file (toggles “Match case”, “Whole word”, “Regular expression”, “Find in selection”),
  ⌥⌘F find and replace, ⌘G / ⇧⌘G next/previous; ⇧⌘F in the whole project (with “Include” / “Exclude”); ⌘P file by
  name (“:12” jumps to the line).
- **Language servers:** F12 or ⌘-click definition, ⇧F12 all references, ⌘F12 implementations, rest the mouse for
  help, F8 / ⇧F8 next/previous error, ⌘. code actions, F2 rename, ⇧⌥F format, ⌃Space complete. In Vi: `gd`, `K`,
  `grr`, `gri`, `grt`, `gra`, `grn`, `]d` / `[d`.
- **Jump history:** ⌃- back, ⌃⇧- forward (Windows/Linux: Ctrl+Alt+← / →); in Vi also `Ctrl-o` / `Ctrl-i`.
- **Settings:** in the Studio “Editor settings” (tabs “Settings” / “Shortcuts”): “Keys” (“Normal (Mac)” or “Vi”),
  “Vi clipboard”, font, “Line numbers”, “Soft wrap”, “Indent”, “Cursor”, “Editor theme”, “Minimap”, “Sticky scroll”,
  “Format on save”, “Autosave” and more. “Keyboard shortcuts” in the Studio's head lists all keys.

## Common tasks

1. **Find a file quickly:** ⌘P, type the name, Enter (in the Studio) — or ⌘K anywhere.
2. **Switch Vi on:** Studio → “Editor settings” → “Keys” → “Vi”. `:w` saves, `:q` closes.
3. **Split the editor:** ⌘\ puts the tab to the right, ⇧⌘\ below; or drag a tab to the edge.

## Traps

- Without an installed language server there are no errors and no definitions — the app then shows only a quiet hint
  about installing one.
- “Autosave” can be set to “After 1 s pause” or “When leaving the editor”; it still never saves a **new note** (see
  [Notes](notizen.md)).
- Without a language server ⌘L selects the line (again: adds the next one); when a language server runs, ⌘L opens
  the code actions like ⌘.
- On Windows/Linux some shortcuts take Ctrl+Shift instead of Ctrl (⌘P/N/O/B/W/R); the list under “Keyboard
  shortcuts” shows them for the keyboard you have set.
- A click in the tree opens a preview that the next click replaces; a double-click opens the tab for good.

## See also

[Notes](notizen.md) · [Tags and links](tags-links.md) · [Studio](studio.md) · [Settings](einstellungen.md)
