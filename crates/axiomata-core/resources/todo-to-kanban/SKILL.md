---
name: todo-to-kanban
description: Turns the open tasks of the ToDo tile (ToDo.md) into cards on the Kanban board, with labels and due dates where the task says so; skips tasks already on the board, asks about unclear ones, and marks every task that is on the board now as done in ToDo.md.
prepend_files: ["ToDo.md"]
timeout_secs: 600
allowed_tools: "Read Edit(ToDo.md) Bash(axiomata-cli board list:*) Bash(axiomata-cli board add:*)"
---

# ToDo to Kanban

The owner's ToDo list is inlined above (`ToDo.md`: open tasks as `- [ ] …`, done ones under a Done
heading). Turn every **open** task into a card on the Kanban board.

Use the app's command line, `axiomata-cli`, by that name (the app puts it on the `PATH`):

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
4. Do not move, edit or delete existing cards.
5. Then mark as done in `ToDo.md` every open task that is on the board now — the ones you just added a card for and
   the ones you skipped because a card was there already. Only after its card exists: a task whose `board add`
   failed, and a task you asked about, stay open. Edit only `ToDo.md`, in the ToDo tile's own format:
   - take the task's line out of the open list, exactly as it stands;
   - put it at the top of the list under `## Done` (make that heading, after a blank line, if there is none) as
     `- [x] <the task's text as it stood> (done: YYYY-MM-DD)`, with today's date from the top of this prompt;
   - leave every other line as it is — the `# ToDo` heading, the other open tasks and the done ones in their order.

Finish with a short report in German, exactly these parts:

**Neue Karten** — one line per card: title and labels. **Übersprungen** — tasks already on the board.
**Erledigt in ToDo** — how many tasks you marked as done there. **Fragen** — one line per task you did not turn into
a card, with one question. Write "Keine" if there are none.
