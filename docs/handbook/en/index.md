# Axiomata at a glance

What Axiomata is, how its areas fit together and which handbook page answers which question.

## What it is

Axiomata-OS is the owner's command centre and second brain: a Mac app that brings together in one place your own vault
of Markdown notes, the agents that work in it and in code projects, and the daily overviews. The areas:

- **Orbit** — the home screen: the vault as a cloud in the background, the App Ring, the tiles and the dock.
- **Second Brain** — the vault as a picture (Bundle, Map, Hex), with search, tags and links; when you save a note, an
  agent puts it in the right place.
- **Studio** — the workbench for code: editor, agent panes with their own worktree, Git, Run, Debug and the Flow, in
  which a planner breaks a goal into cards that a team of agents works through.
- **Kanban** — the board for all tasks, your own and the agents'.
- **Skills and Routines** — working instructions for agents, started by hand or on a schedule; they also feed the
  tiles for calendar, mail and reminders.
- **Glut** — Axiomata's own harness, next to Claude Code; it runs chat, skills and cards on models from OpenRouter or
  Ollama.
- **The chat** at the bottom of the bar and **⌘K** reach all of it.

## How the pages fit together

The app's data lives in `~/.axiomata` (settings, database, skills, logs), separate from the vault you choose under
Settings → Workspace. Every agent run — chat, skill, routine, tile refresh, card — runs on a **harness** and a
**provider** ([Providers](provider.md)) and counts against the **limits** ([Costs](kosten.md)). A task typically
travels like this: card in [Kanban](kanban.md) → “Start …” → plan in [Flow](flow.md) → agents in the
[Studio](studio.md) → “Take over plan”. Which role works on which model is under [Models and roles](modelle-rollen.md).
What you see you set under [Settings](einstellungen.md); cleaning up happens under [Storage](storage.md).

The chat answers from these pages. If an answer is not here, it says so — then it belongs in the handbook.

To read it yourself: `?` in the dock opens the handbook as a window (the pages on the left, the reading view on the
right), ⌘K finds pages in the group “Handbook”, and `/help <topic>` in the chat opens the matching page.

## All pages

- [Orbit, App Ring, tiles and dock](orbit.md) — How the home screen is built: the Orbit in the background, the App Ring around it, the tiles on the surface and the dock with its buttons.
- [Second Brain](second-brain.md) — How to view the vault as a picture: the Bundle, Map and Hex views, the filter field, “How your vault grew”, time travel and a note's detail panel.
- [Creating and filing notes, and the Inbox](notizen.md) — How to write a new note, how the agent files it in the right area of the vault when you save it, and what happens with the Inbox.
- [File app and editor](editor.md) — How to open, read and edit files: the file window, the editor in the Studio, reading view, Vi mode, search, language servers (LSP) and jump history.
- [Tags and wikilinks](tags-links.md) — How to connect notes with `#tags` and `[[Wikilinks]]`, how the tag list in TAGS.md is kept and how the one-time vault run proposes tags and links for every note.
- [Studio](studio.md) — How the workbench for code projects is built: projects, the Editor, Canvas and Flow modes, panes, agents with their own worktree, Git, Run/Tasks and Debug.
- [Flow: plans, agent team and limits](flow.md) — How to create a plan in the Studio's “Flow” mode, have the planner break it into cards, approve it, let it run by itself and take it over at the end — with limits, spend, the agents' questions and escalation.
- [Models, tiers and roles](modelle-rollen.md) — How the Studio switches models on and sorts them into the tiers Light, Medium and Heavy, which engine a tier starts, what a role is and what the one-time switch to roles with tiers did.
- [Kanban](kanban.md) — How the Kanban board works: boards, columns with the fixed columns Proposal and Review, card fields, a card's buttons and what agents may do on the board.
- [The chat in the bar](chat.md) — How to talk to the agent at the bottom of the bar, which slash commands exist, what Plan and Build mean and what the chat may do in the app.
- [Spotlight (⌘K)](spotlight.md) — How to find and start everything in Axiomata with ⌘K: commands, tiles, skills, routines, cards, plans, projects, tags and files — and how to ask the chat from there.
- [Skills and the Skills Deck](skills.md) — What a skill is, where it lives, how to start it and how the Skills Deck keeps the bundled skills up to date.
- [Routines and their schedule](routinen.md) — How to run a skill or a prompt on a schedule, how the interval is set and what happens when the app is closed.
- [Calendar, Mail and Reminders](verbindungen.md) — How the “Calendar”, “Mail” and “Reminders” tiles are connected to Apple's apps, when they refresh and what that costs.
- [Terminal](terminal.md) — How to open and set up the Terminal tile (shell, font, colours) and which keys behave differently there.
- [Glut in Axiomata](glut.md) — What Glut is and where it works in Axiomata: as the harness for chat and skills, as a card run in the Studio pane, as the Glut master in a project — and how to answer its questions.
- [Costs, prices and daily limits](kosten.md) — What agent runs cost, where the prices come from and which limits stop the chat, skills and the Studio.
- [Settings and appearance](einstellungen.md) — What is in the settings and how to adjust the look: theme, font, motion, glass with a background picture, UI size and keyboard.
- [AI providers and harness per role](provider.md) — Which harness (Claude Code or Glut) and which provider (Anthropic, OpenRouter, Ollama) serve the chat and the skills, how to choose models, sign in to Claude Code and set up Glut's web search.
- [Storage and maintenance](storage.md) — How Axiomata cleans up its own data in `~/.axiomata`, what stays for how long and how to use “Clean up now” and the orphaned worktrees.
