# Kanban

How the Kanban board works: boards, columns with the fixed columns Proposal and Review, card fields, a card's buttons and what agents may do on the board.

## What it is

The board is Axiomata's task list for everything — your own items as well as the cards the agents in the Studio
work on. It lives in the app's database; in addition the app writes every board as a note to
`Kanban/<id>-<name>.md` in the vault, so that Obsidian, the search and the agents see it. Kanban is **no tile** but a
large view.

Besides your columns every board has two fixed ones: **Proposal** (agents' proposals, waiting for your yes; hidden
when empty) and **Review** (finished work waiting to be checked and taken over). Neither can be removed.

## Where the buttons are

- **Open:** app ring or dock “Kanban”, ⌘K “Open Kanban”; it opens the board used last.
- **Toolbar:** “Choose a board”, “Filter cards …”, label chips, “Show/Hide archived cards”, “Reload the board”, “Add
  a column”, “Boards & view” (“New board …”, rename the name in place, ✕ “Delete the board”, “Colour stripe by the
  first label”).
- **Column:** menu with the role (“open” / “in progress” / “done”), “Rename”, “Remove”; at the bottom “+ Card”.
- **Open a card:** fields title, text, labels, “Due”, “Assigned to”; the section “Agent flow” (kind, level, role,
  dependencies “Needs first”); “History” with “Write a note …”; for Studio work “Usage”.
- **A card's buttons** (depending on its state): “Start …”, “Start review …”, “Take over …”, “Open in the Flow” or
  “Take over in the Flow …”, “Release”, “Accept proposal”, “Reopen”, “Call off”, “Archive” / “Restore”, “Delete”.

## Common tasks

1. **Create a card:** in the column “+ Card” → title → Enter.
2. **Have agents do a card:** open the card → “Start …” → project → “Start as plan” (becomes a plan in the
   [Flow](flow.md), up to $5). A card that is already a step of a plan starts on an engine with “Start …”.
3. **Accept an agent's proposal:** open the card in “Proposal” → “Accept proposal” (or “Call off”).

## Traps

- **Agents create cards only in “Proposal”.** Releasing, finishing a review, taking over, archiving and deleting
  are your steps; the chat moves only plain cards.
- A new card belongs in the first plain open column, not in “Proposal” — there it waits for a release nobody
  expects.
- The notes in `Kanban/` are only a mirror: what you change there the app **never** reads back, and it overwrites
  it at the next change to the board.
- “Take over …” takes the work over into the project's branch, but pushes nothing.
- “Release” releases a running card: it waits open again, the session loses its access.
- A column with cards asks, when you remove it, where the cards should go.

## See also

[Flow](flow.md) · [Studio](studio.md) · [Chat](chat.md) · [Notes](notizen.md)
