# Flow: plans, the agent team and limits

How to create a plan in the Studio's “Flow” mode, have the planner cut it into cards, release it, let it run by itself and take it over at the end — with limits, spend, the agents' questions and escalation.

## What it is

A **plan** is a goal for a project. A **planner** agent cuts it into cards (proposals) with a role, a level and
dependencies; you accept them and release the plan. With “Run by itself” the cards start by themselves as soon as
their predecessors are done; every card gets a worker and then a reviewer, and the finished work gathers on the
**plan's own line** (an integration branch). At the end you take the whole plan over into the main branch with one
click.

**Who works on a card:** the card's role at the card's level — if the card has no level, the role's default tier —
on that tier's default engine; if that one cannot run, the next engine of the same tier
([Models and roles](modelle-rollen.md)). **The reviewer** is the role of kind “review” at at least the worker's
tier and always runs on **a different engine** than the worker — a model that checks its own work agrees with
itself. If there is no such engine, no reviewer starts, and the app tells you what is missing.

A card that is returned twice goes to **the same role one tier higher** (**escalation**, once per card): the session
switches in place to an engine of the higher tier, worktree and branch stay. If the card was already Heavy or no
engine of the next tier runs, the Studio does not pass it on and waits for you.

## Where the buttons are

- **Flow areas:** “Planning” (the plan), “Agents” (live tiles of the team), “Flow view” (the graph); “Reset
  layout” puts them back.
- **Create a plan:** “New plan…” → name and goal → “Create”. Then “Role of the planner”, “Grill afterwards” (have
  the goal questioned first) and “Start planner”.
- **Proposals:** “Proposals (…)” with “Accept”, “Edit”, “Discard”; “Add card” for your own.
- **Release:** “Release plan” with “Run by itself” and “Auto mode” (workers may run commands and edit without
  asking — only for cards you have read).
- **Limits:** “Cost limit (USD)” above the goal; the line “Spend: …”; when paused, “Continue”.
- **End:** “Take over plan” (everything into the main branch, nothing is pushed, a conflict is rolled back);
  “Close”, “Delete”.
- **Live tiles:** ✉ for unread mail, “Terminal”, for Glut “Watch” (follows the session read-only).
- **The agents' questions:** the badge “… is waiting for you” in the dock, in the Orbit and on cards; answer with
  “Allow” / “Deny” / “In terminal”, also as a strip at the top of the agent pane.

## Common tasks

1. **Plan from a goal:** “New plan…” → goal → “Create” → “Start planner” → accept proposals → “Release plan” with
   “Run by itself”.
2. **Card as a plan:** in the Kanban on the card “Start …” → project → “Start as plan”. The card becomes the goal;
   the Flow opens with “Grill afterwards” preset, the planner starts only on your click.
3. **Take over a finished plan:** once everything is on the plan's line → “Take over plan”.

## Traps

- A plan pauses at its limit (default $15 and 6 million tokens, a card plan $5); “Continue” gives new room. The
  daily limit of all Studio sessions ($20) stops everything new until tomorrow ([Costs](kosten.md)).
- A session that reaches its own limit stops after the running step; the card stays with it.
- If the Studio gives up on a card (“the Studio gave up”), you decide: “Integrate again” or “Redo”.
- “Auto mode” is no sandbox: a push, the CLI, the database and config files stay blocked in their plain forms,
  nothing more.
- Plans that run by themselves start at most four card sessions at once between them; the rest waits.
- When an agent is waiting for you, it stands still until you answer.
- If a tier has only **one** engine that can run, the reviewer finds no other one at that tier and moves up; if
  there is none there either, the card stays without review. Remedy: switch on a second model of the tier under
  “Models”.
- A card without a fitting engine of its tier does not start (“no engine of the tier … can run”); then switch on a
  model of that tier or set a default engine.

## See also

[Studio](studio.md) · [Models and roles](modelle-rollen.md) · [Kanban](kanban.md) · [Glut](glut.md) ·
[Costs](kosten.md)
