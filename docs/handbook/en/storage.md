# Storage and maintenance

How Axiomata cleans up its own data in `~/.axiomata`, what stays for how long and how to use “Clean up now” and the orphaned worktrees.

## What it is

Axiomata keeps **only its own storage** tidy: `~/.axiomata` and the worktrees it creates itself for Studio agents
(`~/Worktrees/axiomata-agents`). Projects, their build folders, the vault, skills, your own background image,
`theme.css` and Glut's folder `~/.glut` are never touched here. Cleaning up runs without an agent and never costs
money.

Once a week the app cleans up by itself at start, in the background. What stays for how long (defaults, changeable):

| Item | stays |
|---|---|
| Runs (skill runs with their output) | 90 days, at least the last 200 |
| Routine firings | 30 days |
| Chat turns, Web searches | 90 days each |
| Archived card history | 90 days after archiving (live cards stay untouched) |
| Session spend | 1 year |
| App log | 14 days |
| Run log | rotates from 10 MB, the last 2 stay |
| Backups | the newest 2 per file |
| Agent status folders | as long as their agent exists |
| Claude sessions | 30 days (never the whole folder — the sign-in lives there) |
| Database file | is compacted when more than 10 % is free |
| Orphaned worktrees | go with their agent; one with changes waits for you |
| Ended sessions without work | go after at least a day of quiet, after a fresh check |
| Backups of the old roles and engines | until you delete them — never by themselves |

## Where the buttons are

**Settings → Storage** (gear in the Dock or ⌘,):

- at the top the total size of `~/.axiomata`, “Last clean-up” and “Would free”;
- per item its size, the period as a number field (“days” or “kept”) and “frees …”;
- a line **Glut** with Glut's own space use (snapshots, run journals, sessions, tool results …) — Glut cleans up
  `~/.glut` by its own rules itself, once a day and with “Clean up now”; if the line is missing, the installed
  Glut cannot report it yet;
- “Clean up once a week at start” (turns off only the weekly run, never the button);
- “Save rules” and “Clean up now”;
- “Orphaned worktrees waiting for you” with a bin per worktree (only visible when there are any);
- “Ended sessions a clean-up removes” (before cleaning up) or “Ended sessions removed” (after): the ended
  Studio sessions without work of their own, which the clean-up takes away with their worktree and empty branch;
- “Backups of the old roles and engines”: per backup folder of the switch to roles with tiers
  (`agents-backup-<date>`, possibly with `-2` …) path, size and a bin “Delete backup …” (only visible when there are
  any; [Models and roles](modelle-rollen.md)).

In the terminal: `axiomata-cli maintenance status` (only shows) and `axiomata-cli maintenance clean [--dry-run]`.

## Common tasks

1. **Clean up now:** Settings → Storage → “Clean up now”. The confirmation says how much is freed; worktrees with
   changes stay.
2. **Change a period:** change the number at the item → “Save rules”. Only then “Clean up now” — the button cleans
   up by the saved rules and is locked while changes are pending.
3. **Throw away an orphaned worktree:** the bin under “Orphaned worktrees waiting for you” → confirm.
   The unsaved work in it is then gone for good.
4. **Delete a backup of the old roles:** the bin under “Backups of the old roles and engines” →
   confirm (“Delete the backup …? The old role files and engines in it are gone for good.”) → “Backup deleted”.

## Traps

- A worktree with unsaved work is **never** deleted by itself; it waits until you throw it away. If you want to
  keep it, take the work over in the Studio first (see [Studio](studio.md)).
- Deleting an agent in the Studio takes its worktree and branch with it; if it has changes, the app asks first.
- Projects' build folders (`target/`, `node_modules`) are the projects' business — “Storage” does not show them.
- A Studio session with work — anything unsaved, a commit beyond its base, an open card or a plan it was
  started for — is never removed by the clean-up; it appears in the Studio under “Waiting for you: take over or
  discard”. Master sessions always stay.
- A backup of the old roles is never deleted by itself; nothing uses it any more, it is only your way back.
- The database is compacted only when it is worth it; a run without due items still does that if needed.

## See also

[Settings](einstellungen.md) · [Costs](kosten.md) · [Glut](glut.md) · [Studio](studio.md) ·
[Models and roles](modelle-rollen.md)
