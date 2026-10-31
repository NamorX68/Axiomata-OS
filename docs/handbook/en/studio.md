# Studio

How the workbench for code projects is built: projects, the Editor, Canvas and Flow modes, panes, agents with their own worktree, Git, Run/Tasks and Debug.

## What it is

The Studio is Axiomata's development environment. It opens large over everything and has three **modes**:

- **“Editor”** — file tree, tabs, split groups ([Editor](editor.md)).
- **“Canvas”** — the panes of the running agents and terminals side by side.
- **“Flow”** — plans, agent team and graph ([Flow](flow.md)).

A mode you are not looking at keeps running. **Agents** are sessions in the project: a **role** (what for) on a
**tier** (Light · Medium · Heavy) and an **engine** (harness + model), with their own branch in their own worktree
under `~/Worktrees/axiomata-agents`. The role names no engine — that comes from the tier: your standard engine of the
tier, otherwise the cheapest ([Models and roles](modelle-rollen.md)). An agent is named after them, for example
“Glut Medium Allrounder (Sonnet 5.5)”, shown on two lines (role · tier on top, harness · model below); a second
session of the same agent gets “#2”.
A **master**, by contrast, is the engine as you use it yourself — in the real project folder, without a worktree.
Another click on the master tile while the master is open starts a second one beside it (`master-glut-2`, …); it
appears as a card at the very top of the agent list and can be removed there.

## Where the buttons are

- **Open:** App Ring or dock “Studio”; “Back to the OS” at the top leads back. Next to it “Editor settings” and
  “Keyboard shortcuts”.
- **Project bar** (top of the sidebar): “Open project…”, “New project…” (“Project name”, “Start a git repository”,
  “Choose where…”), “Close project”, “Change folder…”, “Remove from list” (the folder stays), colour mark.
- **Bar on the left:** “Files”, “Search the project” (⇧⌘F), “Git”, “Run — build, test, start”, “Debug”, “Agents”;
  plus “New terminal” and “Open a file…” (⌘O).
- **“Agents”** (the same tab in every mode), from top to bottom:
  - **“Master”** — the master tiles Glut, Claude Code and Opencode.
  - **“Favorites”** — your remembered agents as tiles; a click starts one. A favourite whose engine cannot run right
    now is dimmed and is never silently moved to another engine.
  - **“Start an agent”** — the seven roles as tiles (hexagon and name). A click opens the menu below, a second click on
    the same tile or “Start” closes it: Light · Medium · Heavy (preselected the role's standard tier), then the engine
    from a list that scrolls when there are many models (“Default of the tier” or an engine of the tier, the standard
    engine marked “standard”), “Start”. Shortest path: role → “Start”. The star (“Keep this choice as a favorite”)
    remembers the choice.
  - **“Sessions”** — only what concerns you: running and waiting sessions and, under “Waiting for you: take over or
    discard”, finished ones with work not yet taken over. If there is none of that: “No session runs or waits for
    you.” Every session has a star (“Keep this agent as a favorite”).
  - at the bottom “New agent…” and **“Models & roles…”** (tabs “Models” and “Roles”,
    [Models and roles](modelle-rollen.md)).
- **Agent pane:** tabs “Terminal”, “Plan”, “Diffs”, “Inbox” (messages to the session, ⌘↩ sends); “Restart the agent”.
  Under “Diffs”: “Discard file”, “Commit…”, “Take over…” (squash or merge, “Never pushes.”).
- **“Git”:** switch branch, “New branch from here”, “Fetch …”, “Stage all”, commit message (⌘↩), ✨ writes a proposal
  (a paid call), “Commit”, “Commit & Push”, “Push” / “Publish branch”.
- **“Run”:** tasks from Cargo.toml, package.json, pyproject.toml and Makefile, grouped by Run / Build / Test / Check /
  Other; “New task”.
- **“Debug”:** Python, Rust, C/C++ and Swift; “What to debug”, “Debug”, “New configuration…”, step buttons,
  variables, watch, breakpoints, console.
- **Keys:** ⌘\ / ⇧⌘\ split, ⌘1–9 tabs, ⌃⇥ next tab, ⌘P file, ⌘B tree, ⌘W close tab, ⌘N new note (Windows/Linux the
  letters with Ctrl+Shift).

## Common tasks

1. **Create a project:** project bar → “New project…” → name, place, if wanted “Start a git repository”.
2. **Start an agent:** “Agents” → “Start an agent” → role → if wanted tier and engine → “Start” — or click a
   favourite, or start a card in Kanban with “Start …” ([Kanban](kanban.md)).
3. **Remember an agent:** click the star in “Start an agent” or on a session; it then appears under “Favorites”. A
   favourite with “Default of the tier” follows along when you change the tier's standard engine.
4. **Take over an agent's work:** agent pane → “Diffs” → check → “Take over…” → squash or merge.

## Traps

- Tasks from a project's `.axiomata/tasks.json`, debug setups and roles from its `.axiomata/agents/` run only after
  you have checked them (“… need your review” → “Review…” → “Allow …”). If the file changes, the app asks again.
- Removing an agent takes its **worktree and branch** with it; unsaved work is then gone (the app asks).
- A finished session **without work of its own** (nothing unsaved, no commit beyond its base, no open card) no longer
  appears in the list. The weekly maintenance checks it again and then clears it away — worktree, empty branch,
  entry — and names it under Settings → Storage ([Storage](storage.md)). Work is never deleted by itself.
- A model without a tier is not an agent and is missing from “Start an agent”; switch it on under “Models” and give it
  a tier.
- “Take over…” and “Commit & Push” never force-push; “Take over…” does not push at all.
- The Opencode master is only your own `opencode` in the project folder, without team tools.
- Several masters work in the **same** folder: if two change the same file, they get in each other's way. For
  separate work, start an agent with its own worktree.
- The project root must not be `$HOME`, `/Users` or `/`.
- Debugging Node/TypeScript does not exist.

## See also

[Models and roles](modelle-rollen.md) · [Flow](flow.md) · [Kanban](kanban.md) · [Glut](glut.md) ·
[Editor](editor.md) · [Costs](kosten.md)
