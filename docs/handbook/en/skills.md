# Skills and the Skills Deck

What a skill is, where it lives, how to run it and how the Skills Deck keeps the bundled skills up to date.

## What it is

A **skill** is working instructions for an agent: a file `~/.axiomata/skills/<name>/SKILL.md` with header lines
(frontmatter) and the text the agent works with. There is only this one place — no skills in the vault. Every run is
a real agent turn and can cost money (see [Costs](kosten.md)).

The header lines: `name` and `description` (required); optionally `backend:` (`claude_code`, `axh` = Glut,
`ollama` = without tools; if it is missing, the harness for skills from the settings applies), `model:` (beats the
provider's skills model), `allowed_tools` (the run's **whole** rights), `prepend_files:` (vault files attached at the
front as context), `timeout_secs`, `output: json` (for tiles that read the answer), `access: read`.

**Bundled skills:** `calendar-digest`, `mail-digest`, `reminders-digest` (feed the tiles, see
[Connections](verbindungen.md)), `cleanup` (clears away old mail notes, done ToDos and old cleanup reports after
3 days), `inbox-sort` (sorts the inbox into the areas, see [Notes](notizen.md)), `todo-to-kanban` (turns open ToDos
into Kanban cards), `news-rss`, `news-web`, `model-news` (AI news and new models into the folder `News/`) and
`vault-tagging` (the vault run, see [Tags and links](tags-links.md)).

## Where the buttons are

- **Front of the “Skills Deck” tile:** per skill a card with `/name`, the backend and ▶ (“Run”). Beside it the
  status of the last run; an interrupted run shows “resume” and “drop”. A broken skill appears as “⚠ skipped …”.
  If there are new versions of bundled skills, “… bundled skill(s) has/have an update” stands at the top with
  “Review”.
- **Back (gear):** “Backend” (filter), “Order” (“by name” / “recently run first”) and the section
  **“Bundled skills”**: per skill the state (“up to date”, “outdated”, “missing”, “changed by you”, “differs — may
  be yours”) with “Update”, at the top “Update all outdated”.
- Elsewhere: ⌘K finds skills (Enter runs), `/skills` or `/skills run <name>` in the chat.

## Common tasks

1. **Run a skill:** ▶ in the Skills Deck — or ⌘K, type the name, Enter.
2. **Update bundled skills:** “Review” (or the back) → “Update” at the skill or “Update all outdated”. With
   “changed by you” the app asks: “Replace” or “Keep mine”.
3. **Create a skill of your own:** ask the chat to write it, or create a folder `~/.axiomata/skills/<name>/` with
   `SKILL.md` yourself; the deck rereads the skills every few seconds.

## Traps

- The deck shows **no output** of a run, only status and time; the result is wherever the skill writes (tile, vault
  note).
- Bundled skills are created only **once**; a newer version in the build does not overwrite your copy.
  “Update all outdated” never touches a copy you changed; in the terminal
  `axiomata-cli skills reseed --force` overwrites all bundled ones (your own skills stay).
- `allowed_tools` are the whole rights: if a tool is missing there, the run may not use it.
- A skill with `backend: axh` (Glut) needs a skill provider with an endpoint (OpenRouter or Ollama), otherwise it
  fails with a clear message.

## See also

[Routines](routinen.md) · [Connections](verbindungen.md) · [Providers](provider.md) · [Chat](chat.md)
