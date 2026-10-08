---
name: todo-to-kanban
description: Turns the open tasks of the ToDo tile (ToDo.md) into cards on the Kanban board, with labels and due dates where the task says so; skips tasks already on the board and asks about unclear ones.
backend: opencode
prepend_files: ["ToDo.md"]
timeout_secs: 600
---

# ToDo to Kanban

The owner's ToDo list is inlined above (`ToDo.md`: open tasks as `- [ ] …`, done ones under a Done
heading). Turn every **open** task into a card on the Kanban board.

Use the app's command line — `$AXIOMATA_CLI` if that variable is set, else `axiomata-cli`:

- `axiomata-cli board list` — the boards; `axiomata-cli board list --board <id>` — one board's columns
  (with ids) and cards.
- `axiomata-cli board add --column <column id> "<title>" --label <label> --label <label>` — a new card.

Steps:

1. List the boards. With one board, use it. With several, use the one whose name fits the tasks best; if
   none clearly does, create no cards and ask which board to use.
2. List that board: its columns and its cards. New cards go into the column named "Backlog": the owner
   collects ideas there, and the "Offen" column is for cards picked to be worked on soon. If the board has
   no Backlog column, use the first plain column whose status is open. Never the "Vorschlag" column: that
   one holds proposals that wait for the owner's yes, and a card put there would sit unnoticed.
3. For each open task: skip it if a card with the same meaning is already on the board (same title, or
   plainly the same thing). Otherwise add a card:
   - the title: the task, cleaned of checkbox, dates and tags, in the owner's words;
   - labels: one or two short lowercase topics when the task makes them obvious (e.g. `rust`, `steuer`,
     `auto`); none if not;
   - a date or deadline in the task stays in the title for now.
   A task you cannot understand, or one that is really several tasks, gets no card — ask about it.
4. Do not change `ToDo.md` and do not move, edit or delete existing cards.

Finish with a short report in German, exactly these two parts:

**Neue Karten** — one line per card: title and labels. **Übersprungen** — tasks already on the board.
**Fragen** — one line per task you did not turn into a card, with one question. Write "Keine" if there are none.
