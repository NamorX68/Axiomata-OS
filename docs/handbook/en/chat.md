# The chat in the bar

How to talk to the agent at the bottom in the bar, which slash commands there are, what Plan and Build mean and what the chat may do in the app.

## What it is

At the bottom of the screen lies the input “Ask the agent, or /help for commands” (in German “Frage den Agenten, oder
/help für Kommandos”). What you write there goes to the
chat agent — on the harness and provider chosen for the chat under Settings → AI providers. It works in the vault,
knows the actions of the open tiles and the Kanban actions and answers from this handbook; if something is not in
it, it says so. Its answers appear in the “Agent” window with the actions used and the cost of the turn.

## Where the buttons are

- **The bar:** the input field; the button “Plan” / “Build” (also ⇧⇥ in the field); for Glut the switch “Effort”
  (only for this conversation); “New session” (⌘N in the field; Windows/Linux Ctrl+Shift+N); on the right “chat” /
  “hide chat” with the number of messages, “working…” while it works.
- **Slash commands** (`/` opens the list; ↑/↓, Tab takes it, Esc closes):
  - Conversation: `/help`, `/clear` (new conversation), `/compact`, `/model [model]` (`default` = back), `/effort
    [level]` (Glut only), `/plan`, `/build`, `/skills [run <name>]`, `/cost`, `/resume [session]`, `/undo` (takes
    back the file changes of the last turn), `/do <text>` (a one-shot instruction outside the conversation).
  - App: `/tile add|remove <type>` or `/tile <type> <action> [json]`, `/theme <id>`, `/open <path>`, `/newfile
    <path>`, `/brain [path | ? search]`.
  - On top, the harness's own commands, under their name or with `/glut:` or `/cc:` in front.
- **The panel above the bar:** as soon as `/model`, `/skills`, `/resume`, `/help` or `/effort` have their space,
  the list opens above it (“Models”, “Skills”, “Earlier conversations”, “Commands and handbook”, “Effort”); what you
  keep typing filters it (`/model deep`). ↑/↓ selects, ⏎ takes it, Esc or a click beside it closes. A skill first
  asks “Run ‹name›? ⏎ yes · Esc no”. If no entry fits for `/model`, “Use ‹id›” takes the typed ID. `/cost`, `/undo`
  and the output of a tile action appear in the same panel — no command writes into the conversation any more.
- ↑/↓ without an open list page through the last 50 inputs.

## Common tasks

1. **Only let it plan:** switch on “Plan” (⇧⇥) — the agent reads and plans, changes nothing; “Build” lets it
   change things.
2. **Start over:** “New session” or `/clear`; `/resume` brings back an earlier conversation.
3. **Another model for this conversation:** `/model <id>`; `/model default` returns to the setting.

## What it may do

- **Look things up and know the state:** it reads this handbook and the app's state — running agents and waiting
  questions in the Studio, costs, recent runs, routines, waiting cards, storage, settings (keys never, only “set” /
  “not set”) and the tag list. It answers only from these; if something is not in them, it says so.
- **Right away:** open views (Studio, a project, Kanban, a card, the Second Brain with a `#tag` filter, a handbook
  page, a Settings tab, a file, a new terminal tile) and change the look (theme, Second Brain layout,
  glass blur and dimming, motion); create and move plain cards, ToDo, the tiles' reading actions. Say what is open,
  and close what loses nothing by it: a Kanban or file panel, a terminal tile with a waiting shell, the Second
  Brain, the handbook, the settings.
- **Only after your yes:** run a skill, create or change a routine, start a card, refresh a digest, write to the
  calendar or reminders; close something that would lose something by it — a file with unsaved text (then the
  panel itself asks: save or discard), a typed card title or history entry not yet sent, an open form of a card, a
  terminal in which a program runs, also in the background (it ends with the tile). The question appears as a bar
  in the app with everything it wants to do; “Allow” does exactly that, “Deny” nothing. A question expires after 2
  minutes.
- **Never:** delete anything, change keys or providers, steer agents in the Studio or close their panes, your steps
  on the board (releasing, review, taking over, integrating).
- In Plan mode it changes nothing.

## Traps

- An unknown `/command` is **not** sent, only reported (“unknown command …”). For a free instruction:
  `/do <text>`.
- `/clear`, `/compact`, `/resume` and `/undo` do not work while the agent is working.
- With a paid provider every turn costs money and counts against the daily limit ([Costs](kosten.md)).
- If the app ends in the middle of a turn, the chat window offers “Resume” or “Drop” at the next start.
- It knows a tile's actions only when the tile lies on the canvas; views, the look and Kanban always work.
- What it does not know, you look up in the handbook yourself: `?` in the dock, ⌘K (group “Handbook”) or
  `/help <topic>`.

## See also

[Spotlight](spotlight.md) · [Providers](provider.md) · [Kanban](kanban.md) · [Skills](skills.md)
