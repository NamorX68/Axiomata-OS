# An agent waits for the owner — seen everywhere (board card #87)

Status: decided 2026-10-12 (owner), being built.

## Problem

Dock, Orbit and the Kanban only know how many card sessions are **at work** (`core/studioActivity.ts`,
`open_card_sessions`). Whether an agent **waits for the owner** — a permission prompt or a question — is known only
inside the Studio, for the open project and only while the Studio is on screen (`ide/agentStatus.ts`). Outside the
Flow the owner cannot see that an agent stands still because of them.

## Decisions

- **W1 — the sign.** Not a second orange: orange stays "something works here". A waiting agent gets a **hexagon badge
  with the number of waiting agents** in the existing `warn` tone (`--ax-warning`, "waits for the owner",
  `ui/status.ts`). It nudges (a short bump every few seconds, `--ax-anim-nudge`) under motion Full and stands still
  under Calm and Off, like every other animation (`core/motion.ts`).
- **W2 — where.** Dock (beside the Studio mark), Orbit (over the canvas, a DOM badge, not drawn into the graph) and
  the Kanban (the card whose session waits carries the badge instead of only the glow).
- **W3 — click.** One waiting agent: the click opens its pane in the Studio (`shell:agent`). Several: a small list
  (agent, card, since when), each row opens that pane.
- **W4 — who counts.** Every Studio agent of every project, card sessions and the owner's own sessions alike.
- **W5 — macOS notification.** Once when an agent starts waiting and the app's window is not focused; no sound, no
  repeat. Text built in Rust from the database (agent name, card number and title), never from the webview.
- **W6 — the plugin.** `tauri-plugin-notification`, called from Rust only; the webview gets **no** capability for it.

## Design

- **Backend** (`axiomata_core::ide_status::waiting_agents`): one pass over all projects' agents — list under a brief
  database lock, then read the status channels and lay the Opencode service's view over them (the same as
  `ide_agent_states`, but for every project), keep those in `waiting`.
- **Watcher** (`src-tauri/src/waiting_watch.rs`, like `limit_watch.rs`): every 2 s; emits `agents:waiting` with the
  whole list whenever it changed; for an agent that is new in the list, posts the notification when the main window
  is not focused. Pushing from Rust (instead of a frontend poll) keeps working while WebKit throttles the timers of a
  window in the background — exactly when the notification matters.
- **Command** `waiting_agents`: the list once, for a page that has just loaded.
- **Frontend** (`core/agentWaiting.ts`): one store for the whole app, fed by the command and the event;
  `ui/WaitingBadge.svelte` draws the badge and the list.

## Known limit

A Claude Code harness killed without its `SessionEnd` hook leaves its channel at `waiting`; the Studio shows that
too. The live test checks how often it happens before anything is built against it.
